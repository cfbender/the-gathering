//! `Game.changeset/2` and `GamePlayer.changeset/2`: casting a game and its seats
//! (`cast_assoc(:seats, required: true)`) with Ecto's rules and messages.

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::changeset::{Changeset, cast_integer};
use crate::db::UtcDateTime;
use crate::error::Errors;

use super::GamesError;
use super::model::{Game, GameFormat, GameResult, GameSource, Seat};
use super::win_condition::WinCondition;

/// A seat after casting (`GamePlayer` changes applied to its data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SeatFields {
    /// The existing row this updates, if any.
    pub existing_id: Option<i64>,
    pub player_id: Option<i64>,
    pub deck_id: Option<i64>,
    pub seat: Option<i64>,
    pub result: Option<String>,
    pub kills: Option<i64>,
    pub eliminated_turn: Option<i64>,
    pub eliminated_by_player_id: Option<i64>,
    pub mvp_card_id: Option<String>,
    pub mvp_card_name: Option<String>,
    pub notes: Option<String>,
}

impl SeatFields {
    fn new() -> Self {
        Self {
            existing_id: None,
            player_id: None,
            deck_id: None,
            seat: None,
            result: None,
            kills: None,
            eliminated_turn: None,
            eliminated_by_player_id: None,
            mvp_card_id: None,
            mvp_card_name: None,
            notes: None,
        }
    }

    fn of(seat: &Seat) -> Self {
        Self {
            existing_id: Some(seat.id),
            player_id: Some(seat.player_id),
            deck_id: seat.deck_id,
            seat: Some(seat.seat),
            result: Some(seat.result.as_str().to_owned()),
            kills: seat.kills,
            eliminated_turn: seat.eliminated_turn,
            eliminated_by_player_id: seat.eliminated_by_player_id,
            mvp_card_id: seat.mvp_card_id.clone(),
            mvp_card_name: seat.mvp_card_name.clone(),
            notes: seat.notes.clone(),
        }
    }
}

/// A seat ready to store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidSeat {
    pub existing_id: Option<i64>,
    pub player_id: i64,
    pub deck_id: Option<i64>,
    pub seat: i64,
    pub result: GameResult,
    pub kills: Option<i64>,
    pub eliminated_turn: Option<i64>,
    pub eliminated_by_player_id: Option<i64>,
    pub mvp_card_id: Option<String>,
    pub mvp_card_name: Option<String>,
    pub notes: Option<String>,
}

/// A game ready to store.
#[derive(Clone, Debug)]
pub(crate) struct ValidGame {
    pub played_at: UtcDateTime,
    pub duration_minutes: Option<i64>,
    pub turns: Option<i64>,
    pub win_condition: Option<WinCondition>,
    pub notes: Option<String>,
    pub source: GameSource,
    pub format: GameFormat,
    pub external_id: Option<String>,
    pub created_by_user_id: Option<i64>,
    /// The seats in params order, or the existing seats when `seats` was not given.
    pub seats: Vec<ValidSeat>,
    /// Whether `seats` was given (so seats are replaced).
    pub seats_given: bool,
}

/// What the caller adds beyond the cast params.
#[derive(Clone, Debug, Default)]
pub(crate) struct Extra<'a> {
    /// `Game.put_created_by/2` (inserts only).
    pub created_by_user_id: Option<i64>,
    /// `Game.put_external_identity/3` (inserts only).
    pub identity: Option<(&'a str, &'a str)>,
}

fn seat_rows(value: &Value) -> Option<Vec<&Value>> {
    match value {
        Value::Array(rows) => Some(rows.iter().collect()),
        // Phoenix form params: `%{"0" => ..., "1" => ...}`, in key order.
        Value::Object(map) => {
            let mut keyed: Vec<(&String, &Value)> = map.iter().collect();
            keyed.sort_by_key(|(key, _)| (key.parse::<i64>().unwrap_or(i64::MAX), (*key).clone()));
            Some(keyed.into_iter().map(|(_, row)| row).collect())
        }
        _ => None,
    }
}

/// `GamePlayer.changeset/2` over `base`.
fn cast_seat(base: &SeatFields, params: &Value) -> (SeatFields, Errors) {
    let mut cs = Changeset::new(params);
    let mut seat = base.clone();
    seat.player_id = cs.integer("player_id").or(base.player_id);
    seat.deck_id = cs.integer("deck_id").or(base.deck_id);
    seat.seat = cs.integer("seat").or(base.seat);
    seat.result = cs.string("result").or(base.result.clone());
    seat.kills = cs.integer("kills").or(base.kills);
    seat.eliminated_turn = cs.integer("eliminated_turn").or(base.eliminated_turn);
    seat.eliminated_by_player_id = cs
        .integer("eliminated_by_player_id")
        .or(base.eliminated_by_player_id);
    seat.mvp_card_id = cs.string("mvp_card_id").or(base.mvp_card_id.clone());
    seat.mvp_card_name = cs.string("mvp_card_name").or(base.mvp_card_name.clone());
    seat.notes = cs.string("notes").or(base.notes.clone());
    cs.required_value("player_id", seat.player_id.as_ref());
    cs.required_value("seat", seat.seat.as_ref());
    cs.required("result", seat.result.as_ref());
    // validate_number checks its options in order and reports only the first failure.
    if seat.seat.is_some_and(|number| number < 1) {
        cs.at_least("seat", seat.seat, 1);
    } else {
        cs.at_most("seat", seat.seat, 10);
    }
    if seat.kills.is_some_and(|kills| kills < 0) {
        cs.at_least("kills", seat.kills, 0);
    } else {
        cs.at_most("kills", seat.kills, 9);
    }
    cs.greater_than("eliminated_turn", seat.eliminated_turn, 0);
    cs.inclusion("result", seat.result.as_deref(), &["win", "loss", "draw"]);
    (seat, cs.errors)
}

/// `Game.validate_seats/1`.
fn validate_seats(errors: &mut Errors, seats: &[SeatFields], format: Option<&str>) {
    let count = seats.len();
    let mut player_ids: Vec<Option<i64>> = Vec::new();
    let mut duplicate = false;
    for seat in seats {
        if player_ids.contains(&seat.player_id) {
            duplicate = true;
        }
        player_ids.push(seat.player_id);
    }
    let mut numbers: Vec<Option<i64>> = seats.iter().map(|seat| seat.seat).collect();
    numbers.sort_by_key(|number| (number.is_none(), *number));
    let consecutive = count > 0
        && numbers
            .iter()
            .zip(1_i64..)
            .all(|(number, expected)| *number == Some(expected));
    let winners = seats
        .iter()
        .filter(|seat| seat.result.as_deref() == Some("win"))
        .count();
    let (required_winners, message) = if format == Some("two_headed_giant") {
        (2, "must have exactly two winners or all draws")
    } else {
        (1, "must have exactly one winner or all draws")
    };
    let winner_and_losses = winners == required_winners
        && seats
            .iter()
            .all(|seat| matches!(seat.result.as_deref(), Some("win" | "loss")));
    let all_draw = !seats.is_empty()
        && seats
            .iter()
            .all(|seat| seat.result.as_deref() == Some("draw"));
    if !(2..=10).contains(&count) {
        errors.add("seats", "must contain between 2 and 10 players");
    }
    if duplicate {
        errors.add("seats", "cannot contain the same player twice");
    }
    if !consecutive {
        errors.add("seats", "must use consecutive seat numbers starting at 1");
    }
    if !(winner_and_losses || all_draw) {
        errors.add("seats", message);
    }
}

async fn exists(conn: &mut SqliteConnection, table: &str, id: i64) -> Result<bool, sqlx::Error> {
    match table {
        "users" => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = ?) AS "e!: bool""#,
                id
            )
            .fetch_one(&mut *conn)
            .await
        }
        _ => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM players WHERE id = ?) AS "e!: bool""#,
                id
            )
            .fetch_one(&mut *conn)
            .await
        }
    }
}

/// Casts and validates `attrs` onto `current` (or a new game), then applies the checks
/// `RecordGame` adds: the creator exists and every deck belongs to its seat's player.
///
/// Seat params whose `id` matches none of the game's seats insert new rows; Ecto would also
/// cast that foreign `id` onto the new row, which could collide with another game's seat,
/// so it is ignored. Seats naming a player that does not exist get Ecto's
/// `assoc_constraint` error (`player: does not exist`) instead of the raise SQLite's unnamed
/// foreign-key errors caused in Elixir.
pub(crate) async fn changeset(
    conn: &mut SqliteConnection,
    current: Option<&Game>,
    attrs: &Value,
    extra: Extra<'_>,
) -> Result<ValidGame, GamesError> {
    let mut cs = Changeset::new(attrs);
    let played_at = cs
        .datetime("played_at")
        .or(current.map(|game| game.played_at));
    let duration_minutes = cs
        .integer("duration_minutes")
        .or(current.and_then(|game| game.duration_minutes));
    let turns = cs.integer("turns").or(current.and_then(|game| game.turns));
    let win_condition = cs.string("win_condition").or(current
        .and_then(|game| game.win_condition)
        .map(|condition| condition.as_str().to_owned()));
    let format = cs.string("format").or(Some(
        current
            .map_or(GameFormat::Commander, |game| game.format)
            .as_str()
            .to_owned(),
    ));
    let notes = cs
        .string("notes")
        .or(current.and_then(|game| game.notes.clone()));
    let (mut source, mut external_id) = match current {
        Some(game) => (
            Some(game.source.as_str().to_owned()),
            game.external_id.clone(),
        ),
        None => (Some(GameSource::Manual.as_str().to_owned()), None),
    };
    if current.is_none()
        && let Some((given_source, given_external_id)) = extra.identity
    {
        source = Some(given_source.to_owned());
        external_id = Some(given_external_id.to_owned());
    }

    cs.required_value("played_at", played_at.as_ref());
    cs.required("source", source.as_ref());
    cs.required("format", format.as_ref());
    cs.inclusion(
        "format",
        format.as_deref(),
        &["commander", "two_headed_giant", "five_star"],
    );
    cs.inclusion(
        "source",
        source.as_deref(),
        &["manual", "csv", "mythic_track", "discord"],
    );
    let keys: Vec<&str> = WinCondition::all().map(WinCondition::as_str).collect();
    cs.inclusion("win_condition", win_condition.as_deref(), &keys);
    cs.greater_than("duration_minutes", duration_minutes, 0);
    cs.greater_than("turns", turns, 0);

    // cast_assoc(:seats, required: true)
    let existing: Vec<SeatFields> = current
        .map(|game| game.seats.iter().map(SeatFields::of).collect())
        .unwrap_or_default();
    let (seats, seats_given) = match cs.raw("seats") {
        None => (existing.clone(), false),
        Some(Value::Null) => (Vec::new(), true),
        Some(value) => {
            if let Some(rows) = seat_rows(value) {
                let mut seats = Vec::with_capacity(rows.len());
                let mut row_errors = Vec::with_capacity(rows.len());
                for row in rows {
                    if !row.is_object() {
                        cs.add_error("seats", "is invalid");
                        continue;
                    }
                    let id = row.get("id").and_then(cast_integer);
                    let base = id
                        .and_then(|id| existing.iter().find(|seat| seat.existing_id == Some(id)))
                        .cloned()
                        .unwrap_or_else(SeatFields::new);
                    let (seat, errors) = cast_seat(&base, row);
                    seats.push(seat);
                    row_errors.push(errors);
                }
                cs.errors.set_nested("seats", row_errors);
                (seats, true)
            } else {
                cs.add_error("seats", "is invalid");
                (existing.clone(), false)
            }
        }
    };
    if seats.is_empty() && !cs.errors.has("seats") {
        cs.add_error("seats", "can't be blank");
    }
    validate_seats(&mut cs.errors, &seats, format.as_deref());

    // RecordGame: validate_user_exists(:created_by_user_id) and validate_deck_ownership/1.
    let created_by_user_id = if current.is_none() {
        extra.created_by_user_id
    } else {
        current.and_then(|game| game.created_by_user_id)
    };
    if current.is_none()
        && let Some(user_id) = created_by_user_id
        && !exists(conn, "users", user_id).await?
    {
        cs.add_error("created_by_user_id", "does not exist");
    }
    let mut mismatched = false;
    for seat in &seats {
        if let (Some(deck_id), player_id) = (seat.deck_id, seat.player_id) {
            let owned = match player_id {
                Some(player_id) => sqlx::query_scalar!(
                    r#"SELECT EXISTS(SELECT 1 FROM decks WHERE id = ? AND player_id = ?) AS "e!: bool""#,
                    deck_id,
                    player_id
                )
                .fetch_one(&mut *conn)
                .await?,
                None => false,
            };
            mismatched |= !owned;
        }
    }
    if mismatched {
        cs.add_error(
            "seats",
            "contains a deck that does not belong to its player",
        );
    }

    // Constraint errors only surface once everything else is valid, as with Repo.insert.
    if cs.is_valid() {
        let mut rows = Vec::with_capacity(seats.len());
        for seat in &seats {
            let mut errors = Errors::new();
            if let Some(player_id) = seat.player_id
                && !exists(conn, "players", player_id).await?
            {
                errors.add("player", "does not exist");
            }
            if let Some(player_id) = seat.eliminated_by_player_id
                && !exists(conn, "players", player_id).await?
            {
                errors.add("eliminated_by_player", "does not exist");
            }
            rows.push(errors);
        }
        cs.errors.set_nested("seats", rows);
    }
    cs.finish()?;

    let valid_seats = seats
        .into_iter()
        .filter_map(|seat| {
            Some(ValidSeat {
                existing_id: seat.existing_id,
                player_id: seat.player_id?,
                deck_id: seat.deck_id,
                seat: seat.seat?,
                result: GameResult::parse(seat.result.as_deref()?)?,
                kills: seat.kills,
                eliminated_turn: seat.eliminated_turn,
                eliminated_by_player_id: seat.eliminated_by_player_id,
                mvp_card_id: seat.mvp_card_id,
                mvp_card_name: seat.mvp_card_name,
                notes: seat.notes,
            })
        })
        .collect();
    let invalid = || Errors::single("base", "is invalid");
    Ok(ValidGame {
        played_at: played_at.ok_or_else(invalid)?,
        duration_minutes,
        turns,
        win_condition: win_condition.as_deref().and_then(WinCondition::parse),
        notes,
        source: source
            .as_deref()
            .and_then(GameSource::parse)
            .ok_or_else(invalid)?,
        format: format
            .as_deref()
            .and_then(GameFormat::parse)
            .ok_or_else(invalid)?,
        external_id,
        created_by_user_id,
        seats: valid_seats,
        seats_given,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(player: i64, number: i64, result: &str) -> SeatFields {
        SeatFields {
            player_id: Some(player),
            seat: Some(number),
            result: Some(result.into()),
            ..SeatFields::new()
        }
    }

    #[test]
    fn seat_rules_match_game_ex() {
        let mut errors = Errors::new();
        validate_seats(&mut errors, &[], Some("commander"));
        assert_eq!(
            errors.messages("seats"),
            [
                "must have exactly one winner or all draws",
                "must use consecutive seat numbers starting at 1",
                "must contain between 2 and 10 players"
            ]
        );
        let mut errors = Errors::new();
        validate_seats(
            &mut errors,
            &[seat(1, 1, "win"), seat(2, 2, "win")],
            Some("two_headed_giant"),
        );
        assert!(errors.is_empty());
        let mut errors = Errors::new();
        validate_seats(&mut errors, &[seat(1, 1, "win"), seat(1, 3, "loss")], None);
        assert_eq!(
            errors.messages("seats"),
            [
                "must use consecutive seat numbers starting at 1",
                "cannot contain the same player twice"
            ]
        );
        let (_, errors) = cast_seat(
            &SeatFields::new(),
            &serde_json::json!({"player_id": "1", "seat": 0, "result": "x", "kills": 1.5}),
        );
        assert_eq!(
            errors.messages("seat"),
            ["must be greater than or equal to 1"]
        );
        assert_eq!(errors.messages("result"), ["is invalid"]);
        assert_eq!(errors.messages("kills"), ["is invalid"]);
    }
}
