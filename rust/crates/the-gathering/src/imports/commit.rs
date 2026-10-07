//! Committing a CSV or Mythic Track import.

use sqlx::SqliteConnection;

use crate::db;
use crate::games::{DeckInput, DeckLinks, GameInput, SeatInput};
use crate::games::{deck, record_game, resolve_player};
use crate::state::AppState;

use super::preview::{self, Source};
use super::{ImportError, ImportGame, ImportResult, ImportSeat};

/// A committed seat.
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
    /// The seat as a game input row.
    pub fn to_input(&self) -> SeatInput {
        SeatInput {
            player_id: Some(self.player_id).into(),
            deck_id: Some(self.deck_id).into(),
            seat: Some(self.seat).into(),
            result: Some(self.result.clone()).into(),
            kills: self.kills.into(),
            mvp_card_name: self.mvp_card_name.clone().into(),
            mvp_card_id: self.mvp_card_id.clone().into(),
            ..SeatInput::default()
        }
    }
}

/// Previews, then commits every game in one transaction and links the games' cards to the
/// catalog.
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
            seats.push(commit_seat(conn, links, seat).await?.to_input());
        }
        let input = GameInput {
            played_at: Some(game.played_at).into(),
            duration_minutes: game.duration_minutes.into(),
            turns: game.turns.into(),
            win_condition: game.win_condition.clone().into(),
            notes: game.notes.clone().into(),
            seats: Some(seats).into(),
            source: Some(source.to_owned()),
            external_id: Some(game.external_id.clone()),
            ..GameInput::default()
        };
        let created = record_game::create(conn, &input, user_id).await?;
        result.created += 1;
        result.game_ids.push(created.id);
    }
    Ok(result)
}

/// Resolves the player (Discord identity first) and finds or creates the deck by name or
/// commander pairing.
pub async fn commit_seat(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    seat: &ImportSeat,
) -> Result<SeatAttrs, ImportError> {
    let player = resolve_player::run(conn, &seat.player, seat.discord_id.as_deref(), None).await?;
    let given = |value: &Option<String>| match value.as_deref() {
        Some(value) if !value.is_empty() => Some(value.to_owned()).into(),
        _ => crate::patch::Patch::Unchanged,
    };
    let input = DeckInput {
        commander_card_id: given(&seat.commander_card_id),
        partner_name: given(&seat.partner_name),
        partner_card_id: given(&seat.partner_card_id),
        color_identity: given(&seat.color_identity),
        decklist_url: given(&seat.decklist_url),
        commander_name: Some(seat.commander.clone()).into(),
        ..DeckInput::default()
    };
    let found = deck::find_or_create_deck(conn, links, player.id, &seat.deck, &input).await?;
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
