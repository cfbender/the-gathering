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
pub use self::query::DateRange;
pub mod records;
pub mod summaries;

use serde_json::Value;

use crate::db::Pool;

/// Games a player needs before they are ranked. Must match `LEADERBOARD_MIN_GAMES` in
/// `assets/react/src/lib/stats.ts`.
pub const MIN_GAMES: usize = 3;

/// The group overview for a date range.
pub async fn overview(pool: &Pool, params: &DateRange) -> Result<Value, sqlx::Error> {
    overview::get(&mut *pool.acquire().await?, params).await
}

/// A player's statistics for a date range; `None` for an unknown player.
pub async fn player(
    pool: &Pool,
    player_id: i64,
    params: &DateRange,
) -> Result<Option<Value>, sqlx::Error> {
    player::get(&mut *pool.acquire().await?, player_id, params).await
}

/// A deck's statistics for a date range; `None` for an unknown deck.
pub async fn deck(
    pool: &Pool,
    deck_id: i64,
    params: &DateRange,
) -> Result<Option<Value>, sqlx::Error> {
    deck::get(&mut *pool.acquire().await?, deck_id, params).await
}

/// Every commander played, most played first.
pub async fn commanders(pool: &Pool, params: &DateRange) -> Result<Vec<Value>, sqlx::Error> {
    commanders::list(&mut *pool.acquire().await?, params).await
}

/// One commander by Scryfall id or card name; `None` when never played.
pub async fn commander(
    pool: &Pool,
    id: &str,
    params: &DateRange,
) -> Result<Option<Value>, sqlx::Error> {
    commanders::get(&mut *pool.acquire().await?, id, params).await
}
