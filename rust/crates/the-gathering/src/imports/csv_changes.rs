//! The material differences a CSV correction would make to a game
//! (`TheGathering.Imports.CSVChanges`).

use serde::Serialize;
use serde_json::{Value, json};

use crate::db::UtcDateTime;
use crate::games::{Deck, Game, WinCondition};

/// One changed value; `player` is `None` for game-level fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    /// The field (`played_at`, `participant`, `deck`, ...).
    pub field: &'static str,
    /// Whose seat changed.
    pub player: Option<String>,
    /// The current value.
    pub before: Value,
    /// The new value.
    pub after: Value,
}

/// A seat as compared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatView {
    /// Player id.
    pub player_id: i64,
    /// Player name.
    pub player_name: String,
    /// The deck.
    pub deck: Option<Deck>,
    /// Seat number.
    pub seat: i64,
    /// Result.
    pub result: String,
    /// Kills.
    pub kills: Option<i64>,
    /// MVP card name.
    pub mvp_card_name: Option<String>,
    /// Who eliminated them.
    pub eliminated_by_player_id: Option<i64>,
}

/// A game as compared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameView {
    /// When.
    pub played_at: UtcDateTime,
    /// Minutes.
    pub duration_minutes: Option<i64>,
    /// Turns.
    pub turns: Option<i64>,
    /// Win condition key.
    pub win_condition: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Seats.
    pub seats: Vec<SeatView>,
}

impl GameView {
    /// A stored game with its seats.
    pub fn of(game: &Game) -> Self {
        Self {
            played_at: game.played_at,
            duration_minutes: game.duration_minutes,
            turns: game.turns,
            win_condition: game
                .win_condition
                .map(|condition| condition.as_str().to_owned()),
            notes: game.notes.clone(),
            seats: game
                .seats
                .iter()
                .map(|seat| SeatView {
                    player_id: seat.player_id,
                    player_name: seat.player.name.clone(),
                    deck: seat.deck.clone(),
                    seat: seat.seat,
                    result: seat.result.as_str().to_owned(),
                    kills: seat.kills,
                    mvp_card_name: seat.mvp_card_name.clone(),
                    eliminated_by_player_id: seat.eliminated_by_player_id,
                })
                .collect(),
        }
    }
}

fn deck_label(deck: &Deck) -> String {
    match &deck.partner_name {
        Some(partner) => format!("{} ({} / {partner})", deck.name, deck.commander_name),
        None => format!("{} ({})", deck.name, deck.commander_name),
    }
}

fn seat_values(seat: Option<&SeatView>) -> [(&'static str, Value); 7] {
    match seat {
        None => [
            ("participant", Value::Null),
            ("deck", Value::Null),
            ("seat", Value::Null),
            ("result", Value::Null),
            ("kills", Value::Null),
            ("mvp_card_name", Value::Null),
            ("eliminated_by_player_id", Value::Null),
        ],
        Some(seat) => [
            ("participant", json!(seat.player_name)),
            ("deck", json!(seat.deck.as_ref().map(deck_label))),
            ("seat", json!(seat.seat)),
            ("result", json!(seat.result)),
            ("kills", json!(seat.kills)),
            ("mvp_card_name", json!(seat.mvp_card_name)),
            (
                "eliminated_by_player_id",
                json!(seat.eliminated_by_player_id),
            ),
        ],
    }
}

fn game_values(game: &GameView) -> [(&'static str, Value); 5] {
    [
        ("played_at", json!(game.played_at.to_ecto_string())),
        ("duration_minutes", json!(game.duration_minutes)),
        ("turns", json!(game.turns)),
        (
            "win_condition",
            json!(
                game.win_condition
                    .as_deref()
                    .map(|key| WinCondition::label_of(WinCondition::parse(key)))
            ),
        ),
        ("notes", json!(game.notes)),
    ]
}

fn changes<const N: usize>(
    before: [(&'static str, Value); N],
    after: [(&'static str, Value); N],
    player: Option<&str>,
) -> Vec<Change> {
    before
        .into_iter()
        .zip(after)
        .filter(|((_, old), (_, new))| old != new)
        .map(|((field, old), (_, new))| Change {
            field,
            player: player.map(str::to_owned),
            before: old,
            after: new,
        })
        .collect()
}

/// `CSVChanges.diff/2`: game fields, then each participant's seat (current seats first).
pub fn diff(before: &GameView, after: &GameView) -> Vec<Change> {
    let mut diffs = changes(game_values(before), game_values(after), None);
    let mut players: Vec<i64> = Vec::new();
    for seat in before.seats.iter().chain(&after.seats) {
        if !players.contains(&seat.player_id) {
            players.push(seat.player_id);
        }
    }
    for id in players {
        let old = before.seats.iter().find(|seat| seat.player_id == id);
        let new = after.seats.iter().find(|seat| seat.player_id == id);
        let name = old.or(new).map(|seat| seat.player_name.clone());
        diffs.extend(changes(seat_values(old), seat_values(new), name.as_deref()));
    }
    diffs
}
