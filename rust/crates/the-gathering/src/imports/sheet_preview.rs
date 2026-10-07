//! Previewing a Google Sheet reconciliation.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqliteConnection;
use time::{Duration, Time};

use crate::db::UtcDateTime;
use crate::games::{deck, load_games, player};

use super::ImportError;
use super::csv::hex;
use super::google_sheet::{self, SheetRow};
use super::sheet_resolution::{self, Choice, Context, ResolvedRow};

/// A player choice in the preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerRef {
    /// Id.
    pub id: i64,
    /// Name.
    pub name: String,
}

/// A deck choice in the preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeckRef {
    /// Id.
    pub id: i64,
    /// Owner.
    pub player_id: i64,
    /// Name.
    pub name: String,
    /// Commander.
    pub commander_name: String,
}

/// A candidate game's seat, with every persisted field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CandidateSeat {
    /// Seat row id.
    pub id: i64,
    /// Player.
    pub player_id: i64,
    /// Deck.
    pub deck_id: Option<i64>,
    /// Seat number.
    pub seat: i64,
    /// Result.
    pub result: String,
    /// Kills.
    pub kills: Option<i64>,
    /// Notes.
    pub notes: Option<String>,
    /// MVP card id.
    pub mvp_card_id: Option<String>,
    /// MVP card name.
    pub mvp_card_name: Option<String>,
    /// Elimination turn.
    pub eliminated_turn: Option<i64>,
    /// Eliminator.
    pub eliminated_by_player_id: Option<i64>,
    /// Player name.
    pub player: String,
    /// Deck name.
    pub deck: Option<String>,
}

/// A recorded game near a row's date (every persisted field, so the revision notices
/// any edit, not only second-resolution `updated_at`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    /// Game id.
    pub id: i64,
    /// When.
    pub played_at: UtcDateTime,
    /// Notes.
    pub notes: Option<String>,
    /// Turns.
    pub turns: Option<i64>,
    /// Minutes.
    pub duration_minutes: Option<i64>,
    /// Provenance.
    pub source: String,
    /// Import identity.
    pub external_id: Option<String>,
    /// Seats.
    pub seats: Vec<CandidateSeat>,
}

/// The reconciliation preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SheetPreview {
    /// Resolved rows.
    pub rows: Vec<ResolvedRow>,
    /// Every player.
    pub players: Vec<PlayerRef>,
    /// Every deck.
    pub decks: Vec<DeckRef>,
    /// Something is selected and every selected row is error-free.
    pub valid: bool,
    /// Fingerprint of the input and this preview.
    pub revision: String,
}

/// `SheetPreview.run/1`; parse failures are `ImportError::Message`.
pub async fn run(conn: &mut SqliteConnection, params: &Value) -> Result<SheetPreview, ImportError> {
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| ImportError::Message("File contents must be text.".to_owned()))?;
    let rows = google_sheet::parse(text).map_err(ImportError::Message)?;
    let players: Vec<PlayerRef> = player::list_players(conn, true)
        .await?
        .into_iter()
        .map(|player| PlayerRef {
            id: player.id,
            name: player.name,
        })
        .collect();
    let decks: Vec<DeckRef> = deck::list_decks(conn, true, None)
        .await?
        .into_iter()
        .map(|(deck, _)| DeckRef {
            id: deck.id,
            player_id: deck.player_id,
            name: deck.name,
            commander_name: deck.commander_name,
        })
        .collect();
    let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    let keys = serde_json::to_string(&keys).unwrap_or_else(|_| "[]".into());
    let imported: HashMap<String, i64> = sqlx::query!(
        r#"SELECT key AS "key!", game_id FROM sheet_import_receipts WHERE key IN (SELECT value FROM json_each(?))"#,
        keys
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|receipt| (receipt.key, receipt.game_id))
    .collect();
    let candidates = candidates(conn, &rows).await?;
    let context = Context {
        params,
        players: &players,
        decks: &decks,
    };
    let mut resolved = Vec::with_capacity(rows.len());
    for row in &rows {
        let nearby: Vec<Candidate> = candidates
            .iter()
            .filter(|game| {
                row.date
                    .is_some_and(|date| (game.played_at.date() - date).whole_days().abs() <= 1)
            })
            .cloned()
            .collect();
        resolved.push(
            sheet_resolution::resolve(conn, row, &context, nearby, imported.get(&row.key).copied())
                .await?,
        );
    }
    let resolved = reject_duplicate_targets(resolved, params);
    let selected: Vec<&ResolvedRow> = resolved
        .iter()
        .filter(|row| row.action != Choice::Text("skip".to_owned()))
        .collect();
    let valid = !selected.is_empty() && selected.iter().all(|row| row.errors.is_empty());
    let revision = fingerprint(params, &resolved, &players, &decks, valid);
    Ok(SheetPreview {
        rows: resolved,
        players,
        decks,
        valid,
        revision,
    })
}

async fn candidates(
    conn: &mut SqliteConnection,
    rows: &[SheetRow],
) -> Result<Vec<Candidate>, sqlx::Error> {
    let dates: Vec<time::Date> = rows.iter().filter_map(|row| row.date).collect();
    let (Some(first), Some(last)) = (dates.iter().min(), dates.iter().max()) else {
        return Ok(Vec::new());
    };
    let start = UtcDateTime::from_offset(
        (*first - Duration::days(1))
            .with_time(Time::MIDNIGHT)
            .assume_utc(),
    );
    let end = UtcDateTime::from_offset(
        (*last + Duration::days(2))
            .with_time(Time::MIDNIGHT)
            .assume_utc(),
    );
    let ids: Vec<i64> = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM games WHERE played_at >= ? AND played_at < ? ORDER BY played_at, id"#,
        start,
        end
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(load_games(conn, &ids)
        .await?
        .into_iter()
        .map(|game| Candidate {
            id: game.id,
            played_at: game.played_at,
            notes: game.notes,
            turns: game.turns,
            duration_minutes: game.duration_minutes,
            source: game.source.as_str().to_owned(),
            external_id: game.external_id,
            seats: game
                .seats
                .into_iter()
                .map(|seat| CandidateSeat {
                    id: seat.id,
                    player_id: seat.player_id,
                    deck_id: seat.deck_id,
                    seat: seat.seat,
                    result: seat.result.as_str().to_owned(),
                    kills: seat.kills,
                    notes: seat.notes,
                    mvp_card_id: seat.mvp_card_id,
                    mvp_card_name: seat.mvp_card_name,
                    eliminated_turn: seat.eliminated_turn,
                    eliminated_by_player_id: seat.eliminated_by_player_id,
                    player: seat.player.name,
                    deck: seat.deck.map(|deck| deck.name),
                })
                .collect(),
        })
        .collect())
}

fn reject_duplicate_targets(rows: Vec<ResolvedRow>, params: &Value) -> Vec<ResolvedRow> {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for row in &rows {
        if let Choice::Id(id) = row.action {
            *counts.entry(id).or_default() += 1;
        }
    }
    rows.into_iter()
        .map(|mut row| {
            let duplicated = match row.action {
                Choice::Id(id) => counts.get(&id).copied().unwrap_or_default() > 1,
                Choice::Text(_) => false,
            };
            if duplicated {
                row.errors.push(
                    "Two sheet rows target the same game. Choose which one to use.".to_owned(),
                );
                row.status = "review";
                let chosen = params
                    .get("actions")
                    .and_then(|actions| actions.get(&row.key))
                    .is_some_and(|value| !value.is_null());
                if !chosen {
                    row.action = Choice::Text("skip".to_owned());
                }
            }
            row
        })
        .collect()
}

/// SHA-256 of the input and the preview (JSON with sorted keys; Elixir hashed the
/// Erlang terms).
fn fingerprint(
    params: &Value,
    rows: &[ResolvedRow],
    players: &[PlayerRef],
    decks: &[DeckRef],
    valid: bool,
) -> String {
    let value = json!([params, {"rows": rows, "players": players, "decks": decks, "valid": valid}]);
    hex(&Sha256::digest(value.to_string()))
}
