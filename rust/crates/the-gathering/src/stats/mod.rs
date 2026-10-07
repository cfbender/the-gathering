//! Read-only statistics derived from games and their seats.
//!
//! Every win/loss/draw figure uses all games. Figures built from data a playgroup may only
//! have started recording later (seat positions, duration, turns, MVP cards) use only games
//! played on or after the administrator's `detailed_stats_from` date; each payload reports
//! that date so the UI can label those figures. Payloads are the JSON `StatsJSON` renders.

pub mod commanders;
pub mod deck;
pub mod elo;
pub mod outcomes;
pub mod overview;
pub mod player;
pub mod query;
pub mod records;
pub mod summaries;

use serde_json::Value;

use crate::db::Pool;

/// Games a player needs before they are ranked. Must match `LEADERBOARD_MIN_GAMES` in
/// `assets/react/src/lib/stats.ts`.
pub const MIN_GAMES: usize = 3;

/// `Stats.overview/1`.
pub async fn overview(pool: &Pool, params: &Value) -> Result<Value, sqlx::Error> {
    overview::get(&mut *pool.acquire().await?, params).await
}

/// `Stats.player/2`; `None` for an unknown player.
pub async fn player(
    pool: &Pool,
    player_id: i64,
    params: &Value,
) -> Result<Option<Value>, sqlx::Error> {
    player::get(&mut *pool.acquire().await?, player_id, params).await
}

/// `Stats.deck/2`; `None` for an unknown deck.
pub async fn deck(pool: &Pool, deck_id: i64, params: &Value) -> Result<Option<Value>, sqlx::Error> {
    deck::get(&mut *pool.acquire().await?, deck_id, params).await
}

/// `Stats.commanders/1`: every commander played, most played first.
pub async fn commanders(pool: &Pool, params: &Value) -> Result<Vec<Value>, sqlx::Error> {
    commanders::list(&mut *pool.acquire().await?, params).await
}

/// `Stats.commander/2`: one commander by Scryfall id or card name; `None` when never played.
pub async fn commander(
    pool: &Pool,
    id: &str,
    params: &Value,
) -> Result<Option<Value>, sqlx::Error> {
    commanders::get(&mut *pool.acquire().await?, id, params).await
}
