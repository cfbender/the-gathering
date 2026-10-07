//! Committing a reviewed Google Sheet reconciliation.

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::db;
use crate::games::DeckLinks;
use crate::games::{deck, load_game, player, record_game};
use crate::state::AppState;

use super::ImportError;
use super::csv::noon;
use super::sheet_preview;
use super::sheet_resolution::{Choice, ResolvedRow, ResolvedSeat, SeatPlayer};

const STALE: &str = "Preview is stale or invalid. Preview again before importing.";

/// The commit's counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SheetResult {
    /// Games created.
    pub created: i64,
    /// Games updated.
    pub updated: i64,
    /// Rows skipped.
    pub skipped: i64,
    /// Written games, most recent first (as Elixir accumulated them).
    pub game_ids: Vec<i64>,
}

/// `SheetCommit.run/3`: re-runs the preview inside the transaction and commits only when
/// it still has the reviewed revision and is valid.
pub async fn run(
    state: &AppState,
    params: &Value,
    revision: Option<&str>,
    user_id: Option<i64>,
) -> Result<SheetResult, ImportError> {
    let mut tx = db::begin(&state.pool).await?;
    let preview = match sheet_preview::run(&mut tx, params).await {
        Ok(preview) => preview,
        Err(ImportError::Database(error)) => return Err(error.into()),
        Err(_) => return Err(ImportError::Message(STALE.to_owned())),
    };
    if Some(preview.revision.as_str()) != revision || !preview.valid {
        return Err(ImportError::Message(STALE.to_owned()));
    }
    let mut result = SheetResult::default();
    for row in &preview.rows {
        commit_row(&mut tx, state.games.deck_links(), row, &mut result, user_id).await?;
    }
    tx.commit().await?;
    Ok(result)
}

async fn commit_row(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    row: &ResolvedRow,
    result: &mut SheetResult,
    user_id: Option<i64>,
) -> Result<(), ImportError> {
    let game_id = match &row.action {
        Choice::Text(action) if action == "skip" => {
            result.skipped += 1;
            return Ok(());
        }
        Choice::Text(_) => {
            let mut seats = Vec::with_capacity(row.seats.len());
            for (seat, index) in row.seats.iter().zip(1_i64..) {
                seats.push(create_seat(conn, links, seat, index).await?);
            }
            let attrs = json!({
                "played_at": row.date.map(|date| noon(date.0)),
                "notes": row.notes,
                "source": "csv",
                "external_id": format!("sheet:{}", row.key),
                "seats": seats,
            });
            let game = record_game::create(conn, &attrs, user_id).await?;
            result.created += 1;
            game.id
        }
        Choice::Id(id) => {
            let game = load_game(conn, *id)
                .await?
                .ok_or(ImportError::Message(STALE.to_owned()))?;
            let mut seats = Vec::with_capacity(game.seats.len());
            for existing in &game.seats {
                let seat = row
                    .seats
                    .iter()
                    .find(|seat| seat.player_id == Some(SeatPlayer::Id(existing.player_id)))
                    .ok_or(ImportError::Message(STALE.to_owned()))?;
                seats.push(json!({
                    "id": existing.id,
                    "player_id": existing.player_id,
                    "seat": existing.seat,
                    "deck_id": deck_id(conn, links, seat, existing.player_id).await?,
                    "result": seat.result,
                    "kills": seat.kills,
                }));
            }
            let notes = if row.notes.is_empty() {
                json!(game.notes)
            } else {
                json!(row.notes)
            };
            let attrs = json!({"seats": seats, "notes": notes});
            let saved = record_game::update(conn, &game, &attrs).await?;
            result.updated += 1;
            saved.id
        }
    };
    sqlx::query!(
        "INSERT INTO sheet_import_receipts (key, game_id) VALUES (?, ?)",
        row.key,
        game_id
    )
    .execute(&mut *conn)
    .await?;
    result.game_ids.insert(0, game_id);
    Ok(())
}

async fn create_seat(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    seat: &ResolvedSeat,
    index: i64,
) -> Result<Value, ImportError> {
    let player_id = match &seat.player_id {
        Some(SeatPlayer::Id(id)) => *id,
        _ => {
            player::find_or_create_player_by_name(conn, &seat.player, &json!({}))
                .await?
                .id
        }
    };
    Ok(json!({
        "player_id": player_id,
        "deck_id": deck_id(conn, links, seat, player_id).await?,
        "seat": index,
        "result": seat.result,
        "kills": seat.kills,
    }))
}

async fn deck_id(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    seat: &ResolvedSeat,
    player_id: i64,
) -> Result<Value, ImportError> {
    Ok(match &seat.deck_id {
        Some(Choice::Text(text)) if text == "new" => {
            let attrs = json!({"commander_name": seat.deck});
            json!(
                deck::find_or_create_deck(conn, links, player_id, &seat.deck, &attrs)
                    .await?
                    .id
            )
        }
        Some(Choice::Id(id)) => json!(id),
        Some(Choice::Text(text)) => json!(text),
        None => Value::Null,
    })
}
