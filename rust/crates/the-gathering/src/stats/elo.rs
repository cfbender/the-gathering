//! Multiplayer Elo ratings replayed over a list of games (`TheGathering.Stats.Elo`).
//!
//! Every player starts at 1000. After each game, each seat is compared with every other
//! seat at the table: the winner scores 1 against each loser, two drawing seats score 0.5
//! against each other, and two losers are not compared. The pairwise differences between
//! actual and expected scores are averaged over the seat's opponents and scaled by K = 32,
//! so a game moves at most K points in total and the table's changes sum to zero.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use time::Date;

use crate::db::{IsoDate, UtcDateTime};
use crate::games::{Game, GameResult, Player};

use super::records::{float, round_i64, unique_players};

const START: f64 = 1000.0;
const K: f64 = 32.0;

/// One player's rating.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rating {
    /// Player id.
    pub id: i64,
    /// Player name.
    pub name: String,
    /// Current rating.
    pub rating: i64,
    /// Rating at the start of the window.
    pub start: i64,
    /// Peak inside the window.
    pub peak: i64,
    /// Games rated inside the window.
    pub games: usize,
    /// `(date, rating)` points, oldest first.
    pub history: Vec<(String, i64)>,
}

impl Rating {
    /// `{id, name, rating, start, peak, games, history: [{date, rating}]}`.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "rating": self.rating,
            "start": self.start,
            "peak": self.peak,
            "games": self.games,
            "history": self.history_json(),
        })
    }

    /// The history points.
    pub fn history_json(&self) -> Value {
        Value::Array(self.history.iter().map(|(date, rating)| json!({"date": date, "rating": rating})).collect())
    }
}

#[derive(Clone, Debug)]
struct State {
    player: Player,
    rating: f64,
    start: f64,
    peak: f64,
    games: usize,
    history: Vec<(String, i64)>,
}

fn score(mine: GameResult, theirs: GameResult) -> Option<f64> {
    match (mine, theirs) {
        (GameResult::Win, GameResult::Win) | (GameResult::Draw, GameResult::Draw) => Some(0.5),
        (GameResult::Win, _) => Some(1.0),
        (_, GameResult::Win) => Some(0.0),
        _ => None,
    }
}

fn expected(rating: f64, opponent: f64) -> f64 {
    1.0 / (1.0 + 10_f64.powf((opponent - rating) / 400.0))
}

fn rate_game(game: &Game, states: &mut BTreeMap<i64, State>) {
    let seats = unique_players(game);
    let opponents = float(seats.len().saturating_sub(1)).max(1.0);
    let ratings: BTreeMap<i64, f64> = seats
        .iter()
        .map(|seat| (seat.player_id, states.get(&seat.player_id).map_or(START, |state| state.rating)))
        .collect();
    let date = IsoDate(game.played_at.date()).to_string();
    for seat in &seats {
        let mine = ratings.get(&seat.player_id).copied().unwrap_or(START);
        let change: f64 = seats
            .iter()
            .filter(|opponent| opponent.player_id != seat.player_id)
            .map(|opponent| {
                let theirs = ratings.get(&opponent.player_id).copied().unwrap_or(START);
                score(seat.result, opponent.result).map_or(0.0, |score| score - expected(mine, theirs))
            })
            .sum();
        let rating = mine + K * change / opponents;
        match states.get_mut(&seat.player_id) {
            Some(state) => {
                state.rating = rating;
                state.peak = state.peak.max(rating);
                state.games += 1;
                state.history.push((date.clone(), round_i64(rating)));
            }
            None => {
                states.insert(
                    seat.player_id,
                    State {
                        player: seat.player.clone(),
                        rating,
                        start: START,
                        peak: rating.max(START),
                        games: 1,
                        history: vec![(date.clone(), round_i64(rating))],
                    },
                );
            }
        }
    }
}

/// `Elo.ratings/2` over `games` (newest first), highest rating first.
///
/// `window` is `(first local day, the instant it starts)`: earlier games are replayed so
/// ratings carry in, but only players who played inside the window are returned, with peak,
/// games, and history covering the window (opening with the carried-in rating).
pub fn ratings(games: &[Game], window: Option<(Date, UtcDateTime)>) -> Vec<Rating> {
    let mut states: BTreeMap<i64, State> = BTreeMap::new();
    let oldest_first = games.iter().rev();
    let (before, within): (Vec<&Game>, Vec<&Game>) = match window {
        None => (Vec::new(), oldest_first.collect()),
        Some((_, starts_at)) => {
            let all: Vec<&Game> = oldest_first.collect();
            let split = all.iter().position(|game| game.played_at >= starts_at).unwrap_or(all.len());
            let (before, within) = all.split_at(split);
            (before.to_vec(), within.to_vec())
        }
    };
    for game in before {
        rate_game(game, &mut states);
    }
    if let Some((date, _)) = window {
        let date = IsoDate(date).to_string();
        for state in states.values_mut() {
            state.start = state.rating;
            state.peak = state.rating;
            state.games = 0;
            state.history = vec![(date.clone(), round_i64(state.rating))];
        }
    }
    for game in within {
        rate_game(game, &mut states);
    }
    let mut ratings: Vec<Rating> = states
        .into_values()
        .filter(|state| state.games > 0)
        .map(|state| Rating {
            id: state.player.id,
            name: state.player.name,
            rating: round_i64(state.rating),
            start: round_i64(state.start),
            peak: round_i64(state.peak),
            games: state.games,
            history: state.history,
        })
        .collect();
    ratings.sort_by(|a, b| {
        b.rating.cmp(&a.rating).then(b.games.cmp(&a.games)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    ratings
}
