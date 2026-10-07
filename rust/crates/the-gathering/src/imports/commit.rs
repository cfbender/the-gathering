//! Committing a CSV or Mythic Track import (`TheGathering.Imports.Commit`).

use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use crate::db;
use crate::games::DeckLinks;
use crate::games::{deck, record_game, resolve_player};
use crate::state::AppState;

use super::preview::{self, Source};
use super::{ImportError, ImportGame, ImportResult, ImportSeat};

/// A committed seat's `GamePlayer` attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatAttrs {
    /// Resolved player.
    pub player_id: i64,
    /// Found or created deck.
    pub deck_id: i64,
    /// Seat number.
    pub seat: i64,
    /// Result.
    pub result: String,
    /// Kills.
    pub kills: Option<i64>,
    /// MVP card name.
    pub mvp_card_name: Option<String>,
    /// MVP card id.
    pub mvp_card_id: Option<String>,
}

impl SeatAttrs {
    /// The seat params for `Games.create_game/2`.
    pub fn to_json(&self) -> Value {
        json!({
            "player_id": self.player_id,
            "deck_id": self.deck_id,
            "seat": self.seat,
            "result": self.result,
            "kills": self.kills,
            "mvp_card_name": self.mvp_card_name,
            "mvp_card_id": self.mvp_card_id,
        })
    }
}

/// `Commit.run/4`: previews, then commits every game in one transaction and links the
/// games' cards to the catalog.
pub async fn run(
    state: &AppState,
    source: Source,
    payload: &str,
    user_id: Option<i64>,
) -> Result<ImportResult, ImportError> {
    let preview = {
        let mut conn = state.pool.acquire().await?;
        preview::run(&mut conn, source, payload).await?
    };
    if !preview.valid {
        return Err(ImportError::Validation(Box::new(preview)));
    }
    let mut tx = db::begin(&state.pool).await?;
    let result = commit_games(
        &mut tx,
        state.games.deck_links(),
        &preview.games,
        source.as_str(),
        user_id,
    )
    .await?;
    tx.commit().await?;
    for id in &result.game_ids {
        state.games.link_catalog_cards(*id).await?;
    }
    Ok(result)
}

async fn commit_games(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    games: &[ImportGame],
    source: &str,
    user_id: Option<i64>,
) -> Result<ImportResult, ImportError> {
    let mut result = ImportResult::default();
    for game in games {
        let existing = sqlx::query_scalar!(
            r#"SELECT id AS "id!: i64" FROM games WHERE source = ? AND external_id = ?"#,
            source,
            game.external_id
        )
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = existing {
            result.skipped += 1;
            result.game_ids.push(id);
            continue;
        }
        let mut seats = Vec::with_capacity(game.seats.len());
        for seat in &game.seats {
            seats.push(commit_seat(conn, links, seat).await?.to_json());
        }
        let attrs = json!({
            "played_at": game.played_at,
            "duration_minutes": game.duration_minutes,
            "turns": game.turns,
            "win_condition": game.win_condition,
            "notes": game.notes,
            "source": source,
            "external_id": game.external_id,
            "seats": seats,
        });
        let created = record_game::create(conn, &attrs, user_id).await?;
        result.created += 1;
        result.game_ids.push(created.id);
    }
    Ok(result)
}

/// `Commit.commit_seat/1`: resolves the player (Discord identity first) and finds or
/// creates the deck by name or commander pairing.
pub async fn commit_seat(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    seat: &ImportSeat,
) -> Result<SeatAttrs, ImportError> {
    let player = resolve_player::run(conn, &seat.player, seat.discord_id.as_deref(), None).await?;
    let mut attrs = Map::new();
    for (key, value) in [
        ("commander_card_id", &seat.commander_card_id),
        ("partner_name", &seat.partner_name),
        ("partner_card_id", &seat.partner_card_id),
        ("color_identity", &seat.color_identity),
        ("decklist_url", &seat.decklist_url),
    ] {
        if let Some(value) = value.as_deref().filter(|value| !value.is_empty()) {
            attrs.insert(key.to_owned(), Value::String(value.to_owned()));
        }
    }
    attrs.insert(
        "commander_name".to_owned(),
        Value::String(seat.commander.clone()),
    );
    let found =
        deck::find_or_create_deck(conn, links, player.id, &seat.deck, &Value::Object(attrs))
            .await?;
    Ok(SeatAttrs {
        player_id: player.id,
        deck_id: found.id,
        seat: seat.seat,
        result: seat.result.clone(),
        kills: seat.kills,
        mvp_card_name: seat.mvp_card.clone(),
        mvp_card_id: seat.mvp_card_id.clone(),
    })
}
