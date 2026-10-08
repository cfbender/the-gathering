//! One deck's statistics.

use super::query::DateRange;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::games::{Game, GameResult, Seat, get_deck, get_player};

use super::records::{self, Record, grouped_records, player_entity};
use super::{query, summaries};

fn deck_seat(game: &Game, deck_id: i64) -> Option<&Seat> {
    game.seats.iter().find(|seat| seat.deck_id == Some(deck_id))
}

fn results(seat: Option<&Seat>) -> Vec<GameResult> {
    seat.map(|seat| seat.result).into_iter().collect()
}

/// A deck's statistics for a date range; `None` for an unknown deck.
pub async fn get(
    conn: &mut SqliteConnection,
    deck_id: i64,
    params: &DateRange,
) -> Result<Option<Value>, sqlx::Error> {
    let Some(deck) = get_deck(conn, deck_id).await? else {
        return Ok(None);
    };
    let player = get_player(conn, deck.player_id).await?;
    let games = query::games(conn, params, None, Some(deck.id)).await?;
    let seats: Vec<&Seat> = games
        .iter()
        .filter_map(|game| deck_seat(game, deck.id))
        .collect();
    let cutoff = query::detailed_stats_from(conn).await?;
    let detailed = query::detailed(&games, cutoff);
    let opponents: Vec<&Seat> = games
        .iter()
        .flat_map(|game| {
            game.seats
                .iter()
                .filter(|seat| seat.deck_id != Some(deck.id))
        })
        .collect();
    Ok(Some(json!({
        "detailed_stats_from": cutoff,
        "deck": summaries::deck_entity(&deck),
        "player": player.as_ref().map(summaries::player),
        "record": Record::of_seats(seats.iter().copied()).to_json(),
        "average_duration_minutes": records::average(detailed.iter().map(|game| game.duration_minutes)),
        "average_turns": records::average(detailed.iter().map(|game| game.turns)),
        "opponents": grouped_records(opponents.iter().copied(), player_entity, |seat| seat.player_id),
        "recent_games": games
            .iter()
            .take(10)
            .map(|game| Value::Object(summaries::recent_game(game, &results(deck_seat(game, deck.id)))))
            .collect::<Vec<_>>(),
        "win_rate_over_time": records::cumulative_win_rate(
            games.iter().map(|game| (game.played_at, results(deck_seat(game, deck.id)))),
        ),
    })))
}
