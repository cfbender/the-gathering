//! Satisfaction ("salt") statistics: how much players enjoyed their games, rated from 1
//! (awful) to 5 (rad) on their own seat. Unrated seats are left out of every figure.

use super::query::DateRange;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::games::{Game, Seat};

use super::commanders::{self, deck_seats};
use super::query;
use super::records::{self, Object, group_by, player_entity};

/// Ratings tallied from seats.
#[derive(Clone, Debug, PartialEq)]
pub struct Ratings {
    /// Rated seats.
    pub ratings: usize,
    /// One decimal, `None` without ratings.
    pub average: Option<f64>,
    /// `counts[i]` seats rated `i + 1`.
    pub counts: [usize; 5],
}

impl Ratings {
    /// Tallies the present ratings.
    pub fn of(ratings: impl IntoIterator<Item = Option<i64>>) -> Self {
        let ratings: Vec<i64> = ratings.into_iter().flatten().collect();
        let mut counts = [0; 5];
        for (slot, value) in counts.iter_mut().zip(1_i64..) {
            *slot = ratings.iter().filter(|rating| **rating == value).count();
        }
        Self {
            ratings: ratings.len(),
            average: records::average(ratings.iter().copied().map(Some)),
            counts,
        }
    }

    /// `{ratings, average, counts}`.
    pub fn object(&self) -> Object {
        let mut object = Object::new();
        object.insert("ratings".into(), json!(self.ratings));
        object.insert("average".into(), json!(self.average));
        object.insert("counts".into(), json!(self.counts));
        object
    }
}

/// The ratings of everyone else at `seat`'s table.
pub fn opponent_ratings<'a>(
    game: &'a Game,
    seat: &'a Seat,
) -> impl Iterator<Item = Option<i64>> + 'a {
    game.seats
        .iter()
        .filter(move |other| other.id != seat.id)
        .map(|other| other.satisfaction)
}

/// A player's or deck's ratings: `{...summary, opponents: summary}`, where `opponents` is
/// how the rest of the table rated the games `seats` sat in.
pub fn seat_summary<'a>(seats: impl IntoIterator<Item = (&'a Game, &'a Seat)>) -> Value {
    let seats: Vec<(&Game, &Seat)> = seats.into_iter().collect();
    let mut object = Ratings::of(seats.iter().map(|(_, seat)| seat.satisfaction)).object();
    object.insert(
        "opponents".into(),
        Value::Object(
            Ratings::of(
                seats
                    .iter()
                    .flat_map(|(game, seat)| opponent_ratings(game, seat)),
            )
            .object(),
        ),
    );
    Value::Object(object)
}

fn average(object: &Object, key: &str) -> Option<f64> {
    object.get(key).and_then(Value::as_f64)
}

/// Lowest `key` first, rows without one last, then most ratings.
pub fn sort_saltiest(rows: &mut [Object], key: &str, count: &str) {
    rows.sort_by(|a, b| {
        let (left, right) = (average(a, key), average(b, key));
        match (left, right) {
            (Some(left), Some(right)) => left.total_cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| {
            let count_of = |row: &Object| row.get(count).and_then(Value::as_u64);
            count_of(b).cmp(&count_of(a))
        })
    });
}

/// The playgroup's satisfaction overview for a date range.
pub async fn get(
    conn: &mut SqliteConnection,
    playgroup_id: i64,
    params: &DateRange,
) -> Result<Value, sqlx::Error> {
    let games = query::games(conn, playgroup_id, params, None, None).await?;
    let seats: Vec<(&Game, &Seat)> = games
        .iter()
        .flat_map(|game| game.seats.iter().map(move |seat| (game, seat)))
        .collect();
    let mut players: Vec<Object> = group_by(seats.iter().copied(), |(_, seat)| seat.player_id)
        .into_values()
        .filter_map(|rows| {
            let (_, first) = rows.first()?;
            let mut object = player_entity(first);
            object.insert("games".into(), json!(rows.len()));
            let own = Ratings::of(rows.iter().map(|(_, seat)| seat.satisfaction));
            let opponents = Ratings::of(
                rows.iter()
                    .flat_map(|(game, seat)| opponent_ratings(game, seat)),
            );
            if own.ratings == 0 && opponents.ratings == 0 {
                return None;
            }
            object.insert("ratings".into(), json!(own.ratings));
            object.insert("average".into(), json!(own.average));
            object.insert("opponent_ratings".into(), json!(opponents.ratings));
            object.insert("opponent_average".into(), json!(opponents.average));
            Some(object)
        })
        .collect();
    sort_saltiest(&mut players, "average", "ratings");
    let rated_games = games
        .iter()
        .filter(|game| game.seats.iter().any(|seat| seat.satisfaction.is_some()))
        .count();
    let by_month: Vec<Value> = group_by(seats.iter().copied(), |(game, _)| {
        let date = game.played_at.date();
        format!("{:04}-{:02}", date.year(), u8::from(date.month()))
    })
    .into_iter()
    .filter_map(|(month, rows)| {
        let ratings = Ratings::of(rows.iter().map(|(_, seat)| seat.satisfaction));
        if ratings.ratings == 0 {
            return None;
        }
        let mut object = ratings.object();
        object.insert("month".into(), json!(month));
        Some(Value::Object(object))
    })
    .collect();
    let commanders = commanders::satisfaction(conn, &deck_seats(&games)).await?;
    // Lowest average among games at least two players rated; ties go to more ratings, then
    // the newest game.
    let mut saltiest: Option<(&Game, Ratings)> = None;
    for game in &games {
        let ratings = Ratings::of(game.seats.iter().map(|seat| seat.satisfaction));
        let (Some(average), true) = (ratings.average, ratings.ratings >= 2) else {
            continue;
        };
        let better = saltiest.as_ref().is_none_or(|(_, best)| {
            match average.total_cmp(&best.average.unwrap_or(f64::MAX)) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => ratings.ratings > best.ratings,
                std::cmp::Ordering::Greater => false,
            }
        });
        if better {
            saltiest = Some((game, ratings));
        }
    }

    Ok(json!({
        "games_count": games.len(),
        "rated_games": rated_games,
        "summary": Ratings::of(seats.iter().map(|(_, seat)| seat.satisfaction)).object(),
        "by_month": by_month,
        "saltiest_game": saltiest.map(|(game, ratings)| json!({
            "id": game.id,
            "played_at": game.played_at,
            "average": ratings.average,
            "ratings": ratings.ratings,
        })),
        "players": players,
        "commanders": commanders,
    }))
}
