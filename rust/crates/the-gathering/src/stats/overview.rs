//! The playgroup overview.

use super::query::DateRange;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::games::{Game, Seat};

use super::records::{self, group_by, grouped_records, player_entity, seat_entity};
use super::{commanders, elo, outcomes, query, summaries};

/// The group overview for a date range.
pub async fn get(conn: &mut SqliteConnection, params: &DateRange) -> Result<Value, sqlx::Error> {
    let games = query::games(conn, params, None, None).await?;
    let seats: Vec<&Seat> = games.iter().flat_map(|game| &game.seats).collect();
    let cutoff = query::detailed_stats_from(conn).await?;
    let detailed = query::detailed(&games, cutoff);
    let detailed_seats: Vec<&Seat> = detailed.iter().flat_map(|game| &game.seats).collect();

    // Ratings carry in from before the window, so a `date_from` bound replays every
    // earlier game too.
    let ratings = match query::window_start(params) {
        None => elo::ratings(&games, None),
        Some(window) => {
            let all = query::games(conn, &query::without_date_from(params), None, None).await?;
            elo::ratings(&all, Some(window))
        }
    };
    let by_month: Vec<Value> = group_by(games.iter(), |game| {
        let date = game.played_at.date();
        format!("{:04}-{:02}", date.year(), u8::from(date.month()))
    })
    .into_iter()
    .map(|(month, rows)| json!({"month": month, "games": rows.len()}))
    .collect();
    let mut top_commanders = commanders::list(conn, params).await?;
    top_commanders.truncate(8);
    let recent: Vec<&Game> = games.iter().take(6).collect();
    let all_games: Vec<&Game> = games.iter().collect();

    Ok(json!({
        "detailed_stats_from": cutoff,
        "games_count": games.len(),
        "kills": outcomes::kills(seats.iter().copied()),
        "win_conditions": outcomes::win_conditions(games.iter()),
        "average_duration_minutes": records::average(detailed.iter().map(|game| game.duration_minutes)),
        "average_turns": records::average(detailed.iter().map(|game| game.turns)),
        "game_lengths": summaries::game_lengths(&detailed, &|_| None),
        "game_times": games.iter().map(|game| game.played_at).collect::<Vec<_>>(),
        "leaderboard": grouped_records(seats.iter().copied(), player_entity, |seat| seat.player_id),
        "elo": ratings.iter().map(elo::Rating::to_json).collect::<Vec<_>>(),
        "matchups": records::matchups(all_games.iter().copied()),
        "games_by_month": by_month,
        "seat_win_rates": grouped_records(detailed_seats.iter().copied(), seat_entity, |seat| seat.seat),
        "color_win_rates": records::color_records(seats.iter().copied()),
        "color_exposure": records::color_exposure(seats.iter().copied()),
        "commanders": top_commanders,
        "recent_games": summaries::recent_games(conn, &recent).await?,
    }))
}
