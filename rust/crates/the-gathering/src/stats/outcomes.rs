//! Kill totals and recorded win conditions, independent of the detailed-stats cutoff
//! (`TheGathering.Stats.Outcomes`).

use serde_json::{Value, json};

use crate::games::{Game, Seat, WinCondition};

use super::records::{float, float_i64, group_by, round};

/// `kills/1`: totals over seats with recorded kills, per player most kills first.
pub fn kills<'a>(seats: impl IntoIterator<Item = &'a Seat>) -> Value {
    let seats: Vec<&Seat> = seats.into_iter().collect();
    let recorded: Vec<&Seat> = seats.iter().copied().filter(|seat| seat.kills.is_some()).collect();
    let mut players: Vec<(i64, String, i64, usize)> = group_by(recorded.iter().copied(), |seat| seat.player_id)
        .into_iter()
        .filter_map(|(id, rows)| {
            let first = rows.first()?;
            let total: i64 = rows.iter().filter_map(|seat| seat.kills).sum();
            Some((id, first.player.name.clone(), total, rows.len()))
        })
        .collect();
    players.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())).then(a.0.cmp(&b.0)));
    let total: i64 = recorded.iter().filter_map(|seat| seat.kills).sum();
    json!({
        "total": total,
        "recorded_seats": recorded.len(),
        "total_seats": seats.len(),
        "players": players
            .into_iter()
            .map(|(id, name, kills, games)| json!({
                "id": id,
                "name": name,
                "kills": kills,
                "recorded_games": games,
                "average": round(float_i64(kills) / float(games), 2),
            }))
            .collect::<Vec<_>>(),
    })
}

/// `win_conditions/1`: games with a recorded (not unknown) condition, most common first.
pub fn win_conditions<'a>(games: impl IntoIterator<Item = &'a Game>) -> Value {
    let games: Vec<&Game> = games.into_iter().collect();
    let recorded: Vec<WinCondition> = games
        .iter()
        .filter_map(|game| game.win_condition)
        .filter(|condition| *condition != WinCondition::Unknown)
        .collect();
    let mut conditions: Vec<(&'static str, usize)> = group_by(recorded.iter().copied(), |condition| condition.as_str())
        .into_iter()
        .map(|(condition, rows)| (condition, rows.len()))
        .collect();
    conditions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    json!({
        "recorded_games": recorded.len(),
        "total_games": games.len(),
        "conditions": conditions
            .into_iter()
            .map(|(condition, count)| json!({"condition": condition, "games": count}))
            .collect::<Vec<_>>(),
    })
}
