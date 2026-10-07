//! Win/loss/draw arithmetic shared by every statistics view.
//!
//! Rows are seats; games come newest first. Groups are visited in key order before the
//! stable sort, which is how Elixir's small maps iterate, so ties keep the same order.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::db::UtcDateTime;
use crate::games::color_identity;
use crate::games::{Game, GameResult, Seat};

/// A JSON object under construction.
pub type Object = Map<String, Value>;

/// `usize` as `f64` without a lossy cast.
pub fn float(value: usize) -> f64 {
    u32::try_from(value).map_or(f64::from(u32::MAX), f64::from)
}

/// `i64` as `f64` without a lossy cast (values here are small).
pub fn float_i64(value: i64) -> f64 {
    i32::try_from(value).map_or_else(
        |_| {
            if value < 0 {
                f64::from(i32::MIN)
            } else {
                f64::from(i32::MAX)
            }
        },
        f64::from,
    )
}

/// `Float.round/2`: half away from zero at `digits` decimals.
pub fn round(value: f64, digits: i32) -> f64 {
    let scale = 10_f64.powi(digits);
    (value * scale).round() / scale
}

/// `Kernel.round/1` to an integer.
pub fn round_i64(value: f64) -> i64 {
    format!("{:.0}", value.round()).parse().unwrap_or_default()
}

/// `percentage/2`: one decimal, `0.0` for no games.
pub fn percentage(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        round(float(part) * 100.0 / float(total), 1)
    }
}

/// `average/2`: one decimal over the present values, `None` without any.
pub fn average(values: impl IntoIterator<Item = Option<i64>>) -> Option<f64> {
    let values: Vec<i64> = values.into_iter().flatten().collect();
    if values.is_empty() {
        return None;
    }
    let sum: i64 = values.iter().sum();
    Some(round(float_i64(sum) / float(values.len()), 1))
}

/// A win/loss/draw record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    /// Games.
    pub games: usize,
    /// Wins.
    pub wins: usize,
    /// Losses.
    pub losses: usize,
    /// Draws.
    pub draws: usize,
    /// Win percentage, one decimal.
    pub win_rate: f64,
}

impl Record {
    /// `record/1`.
    pub fn of(results: impl IntoIterator<Item = GameResult>) -> Self {
        let (mut games, mut wins, mut losses, mut draws) = (0, 0, 0, 0);
        for result in results {
            games += 1;
            match result {
                GameResult::Win => wins += 1,
                GameResult::Loss => losses += 1,
                GameResult::Draw => draws += 1,
            }
        }
        Self {
            games,
            wins,
            losses,
            draws,
            win_rate: percentage(wins, games),
        }
    }

    /// Of seats.
    pub fn of_seats<'a>(seats: impl IntoIterator<Item = &'a Seat>) -> Self {
        Self::of(seats.into_iter().map(|seat| seat.result))
    }

    /// `{games, wins, losses, draws, win_rate}`.
    pub fn to_json(&self) -> Value {
        Value::Object(self.object())
    }

    /// The fields as an object.
    pub fn object(&self) -> Object {
        let mut object = Object::new();
        self.merge_into(&mut object);
        object
    }

    /// Adds the fields to `object` (`Map.merge(entity, record)`).
    pub fn merge_into(&self, object: &mut Object) {
        object.insert("games".into(), json!(self.games));
        object.insert("wins".into(), json!(self.wins));
        object.insert("losses".into(), json!(self.losses));
        object.insert("draws".into(), json!(self.draws));
        object.insert("win_rate".into(), json!(self.win_rate));
    }
}

fn number(object: &Object, key: &str) -> f64 {
    object.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn name_key(object: &Object) -> String {
    object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase()
}

/// Sorts rows most played first, then by win rate, then by case-folded name.
pub fn sort_records(rows: &mut [Object]) {
    rows.sort_by(|a, b| {
        number(b, "games")
            .total_cmp(&number(a, "games"))
            .then_with(|| number(b, "win_rate").total_cmp(&number(a, "win_rate")))
            .then_with(|| name_key(a).cmp(&name_key(b)))
    });
}

/// Groups `rows` by key, preserving first-occurrence order inside each group.
pub fn group_by<T, K: Ord>(
    rows: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> K,
) -> BTreeMap<K, Vec<T>> {
    let mut groups: BTreeMap<K, Vec<T>> = BTreeMap::new();
    for row in rows {
        groups.entry(key(&row)).or_default().push(row);
    }
    groups
}

/// `grouped_records/3`: one record per group (entity of its first row), most played first.
pub fn grouped_records<'a, K: Ord>(
    seats: impl IntoIterator<Item = &'a Seat>,
    entity: impl Fn(&Seat) -> Object,
    key: impl Fn(&Seat) -> K,
) -> Vec<Value> {
    let mut rows: Vec<Object> = group_by(seats, |seat| key(seat))
        .into_values()
        .filter_map(|group| {
            let first = group.first()?;
            let mut object = entity(first);
            Record::of_seats(group.iter().copied()).merge_into(&mut object);
            Some(object)
        })
        .collect();
    sort_records(&mut rows);
    rows.into_iter().map(Value::Object).collect()
}

/// `{id, name}` of a player.
pub fn player_entity(seat: &Seat) -> Object {
    let mut object = Object::new();
    object.insert("id".into(), json!(seat.player.id));
    object.insert("name".into(), json!(seat.player.name));
    object
}

/// `{id: seat, name: "Seat n"}`.
pub fn seat_entity(seat: &Seat) -> Object {
    let mut object = Object::new();
    object.insert("id".into(), json!(seat.seat));
    object.insert("name".into(), json!(format!("Seat {}", seat.seat)));
    object
}

/// `color_records/1`: one record per deck identity (canonical letters and guild name).
pub fn color_records<'a>(seats: impl IntoIterator<Item = &'a Seat>) -> Vec<Value> {
    grouped_records(
        seats.into_iter().filter(|seat| seat.deck.is_some()),
        |seat| {
            let identity = seat
                .deck
                .as_ref()
                .map(|deck| deck.color_identity.as_str())
                .unwrap_or_default();
            let mut object = Object::new();
            object.insert("id".into(), json!(color_identity::canonical(identity)));
            object.insert("name".into(), json!(color_identity::name(identity)));
            object
        },
        |seat| {
            color_identity::canonical(
                seat.deck
                    .as_ref()
                    .map(|deck| deck.color_identity.as_str())
                    .unwrap_or_default(),
            )
        },
    )
}

/// `color_exposure/1`: per WUBRG color, the seats whose deck ran it, with `share` of the
/// deck-bearing seats.
pub fn color_exposure<'a>(seats: impl IntoIterator<Item = &'a Seat>) -> Vec<Value> {
    let with_decks: Vec<&Seat> = seats
        .into_iter()
        .filter(|seat| seat.deck.is_some())
        .collect();
    [
        ("W", "White"),
        ("U", "Blue"),
        ("B", "Black"),
        ("R", "Red"),
        ("G", "Green"),
    ]
    .into_iter()
    .map(|(color, name)| {
        let rows: Vec<&Seat> = with_decks
            .iter()
            .copied()
            .filter(|seat| {
                seat.deck.as_ref().is_some_and(|deck| {
                    color_identity::canonical(&deck.color_identity).contains(color)
                })
            })
            .collect();
        let mut object = Object::new();
        object.insert("id".into(), json!(color));
        object.insert("name".into(), json!(name));
        object.insert(
            "share".into(),
            json!(percentage(rows.len(), with_decks.len())),
        );
        Record::of_seats(rows.iter().copied()).merge_into(&mut object);
        Value::Object(object)
    })
    .collect()
}

/// Seats with distinct players, first seat per player.
pub fn unique_players(game: &Game) -> Vec<&Seat> {
    let mut seen = Vec::new();
    game.seats
        .iter()
        .filter(|seat| {
            if seen.contains(&seat.player_id) {
                false
            } else {
                seen.push(seat.player_id);
                true
            }
        })
        .collect()
}

/// `matchups/1`: every ordered pair of players who shared a table, with the first
/// player's record in those games.
pub fn matchups<'a>(games: impl IntoIterator<Item = &'a Game>) -> Vec<Value> {
    let pairs = games.into_iter().flat_map(|game| {
        let seats = unique_players(game);
        let mut pairs = Vec::new();
        for seat in &seats {
            for opponent in &seats {
                if seat.player_id != opponent.player_id {
                    pairs.push((*seat, *opponent));
                }
            }
        }
        pairs
    });
    let mut rows: Vec<(Object, i64)> = group_by(pairs, |(seat, opponent)| {
        (seat.player_id, opponent.player_id)
    })
    .into_values()
    .filter_map(|pairs| {
        let (seat, opponent) = pairs.first()?;
        let mut object = Object::new();
        object.insert("id".into(), json!(seat.player_id));
        object.insert("name".into(), json!(seat.player.name));
        object.insert("opponent_id".into(), json!(opponent.player_id));
        Record::of_seats(pairs.iter().map(|(seat, _)| *seat)).merge_into(&mut object);
        Some((object, opponent.player_id))
    })
    .collect();
    rows.sort_by(|(a, a_opponent), (b, b_opponent)| {
        number(b, "games")
            .total_cmp(&number(a, "games"))
            .then_with(|| name_key(a).cmp(&name_key(b)))
            .then_with(|| a_opponent.cmp(b_opponent))
    });
    rows.into_iter()
        .map(|(object, _)| Value::Object(object))
        .collect()
}

/// `histogram/2`: consecutive `bin_size`-wide bins from the lowest value's bin to the
/// highest's (`to` exclusive), skipping missing values.
pub fn histogram(values: impl IntoIterator<Item = Option<i64>>, bin_size: i64) -> Vec<Value> {
    let values: Vec<i64> = values.into_iter().flatten().collect();
    let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) else {
        return Vec::new();
    };
    if bin_size <= 0 {
        return Vec::new();
    }
    let counts = group_by(values.iter().copied(), |value| value / bin_size);
    (min / bin_size..=max / bin_size)
        .map(|bin| {
            json!({
                "from": bin * bin_size,
                "to": (bin + 1) * bin_size,
                "games": counts.get(&bin).map_or(0, Vec::len),
            })
        })
        .collect()
}

/// `cumulative_win_rate/2`: games (newest first) as `(played_at, tracked results)`; the
/// running win rate after each game with any tracked seat, oldest first.
pub fn cumulative_win_rate(
    games: impl DoubleEndedIterator<Item = (UtcDateTime, Vec<GameResult>)>,
) -> Vec<Value> {
    let (mut wins, mut total) = (0, 0);
    let mut points = Vec::new();
    for (played_at, results) in games.rev() {
        if results.is_empty() {
            continue;
        }
        wins += results
            .iter()
            .filter(|result| **result == GameResult::Win)
            .count();
        total += results.len();
        points.push(json!({
            "date": crate::db::IsoDate(played_at.date()).to_string(),
            "win_rate": percentage(wins, total),
        }));
    }
    points
}

/// `tracked_result/1`: a seat's own result, or for several tracked seats in one game
/// `win` if any won, `draw` if any drew, else `loss`.
pub fn tracked_result(results: &[GameResult]) -> Option<GameResult> {
    match results {
        [] => None,
        [only] => Some(*only),
        many if many.contains(&GameResult::Win) => Some(GameResult::Win),
        many if many.contains(&GameResult::Draw) => Some(GameResult::Draw),
        _ => Some(GameResult::Loss),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_like_elixir() {
        assert!((percentage(2, 3) - 66.7).abs() < f64::EPSILON);
        assert!((percentage(1, 8) - 12.5).abs() < f64::EPSILON);
        assert_eq!(round_i64(1016.4), 1016);
        assert_eq!(round_i64(-8.5), -9);
        assert_eq!(average([Some(75), None, Some(40)]), Some(57.5));
    }
}
