//! `TheGathering.WebcamTables.Turns`: pure turn accounting.
//!
//! Times are measured against the shared game's elapsed milliseconds, so pauses freeze a turn
//! without a second set of pause bookkeeping. Counts increment when a turn starts, including
//! the first turn of the game. `history` holds the most recent passes, newest first, so a
//! mistaken pass can be undone exactly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Mode;

const HISTORY_LIMIT: usize = 20;

/// What turn accounting needs to know about a seat.
pub trait TurnSeat {
    /// The seat's player.
    fn player_id(&self) -> i64;
    /// Knocked out of the game.
    fn eliminated(&self) -> bool;
    /// Left the game for good (never set by the room; kept for parity with the Elixir code).
    fn departed(&self) -> bool {
        false
    }
}

/// A plain seat for turn accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    /// The player (a team's first seat in Two-Headed Giant).
    pub player_id: i64,
    /// Out of the game.
    pub eliminated: bool,
    /// Departed.
    pub departed: bool,
}

impl TurnSeat for Unit {
    fn player_id(&self) -> i64 {
        self.player_id
    }

    fn eliminated(&self) -> bool {
        self.eliminated
    }

    fn departed(&self) -> bool {
        self.departed
    }
}

/// One recorded pass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pass {
    /// Who passed.
    pub player_id: i64,
    /// When their turn started.
    pub started_elapsed_ms: i64,
    /// Who received the turn.
    pub next_player_id: Option<i64>,
}

/// The shared turn state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turns {
    /// Whose turn it is.
    #[serde(default)]
    pub active_player_id: Option<i64>,
    /// Turns started per player.
    #[serde(default)]
    pub counts: BTreeMap<i64, i64>,
    /// Banked turn time per player.
    #[serde(default)]
    pub elapsed_ms: BTreeMap<i64, i64>,
    /// When the current turn started (game elapsed time).
    #[serde(default)]
    pub started_elapsed_ms: i64,
    /// Bumped by every pass, so stale passes are refused.
    #[serde(default)]
    pub revision: i64,
    /// Recent passes, newest first.
    #[serde(default)]
    pub history: Vec<Pass>,
}

fn eligible(seat: &impl TurnSeat) -> bool {
    !seat.eliminated() && !seat.departed()
}

/// The two-seat team holding `player_id` (empty when none). A team's first seat is its stable
/// accounting key; keep the full order when grouping.
pub fn team<S: TurnSeat>(seats: &[S], player_id: i64) -> &[S] {
    seats.chunks(2).find(|team| team.iter().any(|seat| seat.player_id() == player_id)).unwrap_or(&[])
}

/// The id turns are accounted under: the team's first seat in Two-Headed Giant.
pub fn turn_id<S: TurnSeat>(seats: &[S], player_id: i64, mode: Mode) -> i64 {
    if mode == Mode::TwoHeadedGiant {
        team(seats, player_id).first().map_or(player_id, TurnSeat::player_id)
    } else {
        player_id
    }
}

fn units<S: TurnSeat>(seats: &[S], mode: Mode) -> Vec<Unit> {
    if mode == Mode::TwoHeadedGiant {
        seats
            .chunks(2)
            .filter_map(|team| {
                team.first().map(|first| Unit {
                    player_id: first.player_id(),
                    eliminated: !team.iter().any(eligible),
                    departed: false,
                })
            })
            .collect()
    } else {
        seats
            .iter()
            .map(|seat| Unit { player_id: seat.player_id(), eliminated: seat.eliminated(), departed: seat.departed() })
            .collect()
    }
}

/// The next eligible player after `active_id`, wrapping around.
pub fn next_player<S: TurnSeat>(seats: &[S], active_id: Option<i64>, mode: Mode) -> Option<i64> {
    let active_id = active_id.map(|id| turn_id(seats, id, mode));
    let units = units(seats, mode);
    let offset = units.iter().position(|unit| Some(unit.player_id) == active_id).map_or(0, |index| index + 1);
    units.iter().skip(offset).chain(units.iter().take(offset)).find(|unit| eligible(*unit)).map(|unit| unit.player_id)
}

/// Passes the turn at `elapsed` game milliseconds.
pub fn pass<S: TurnSeat>(turns: &Turns, seats: &[S], elapsed: i64, mode: Mode) -> Turns {
    let next = next_player(seats, turns.active_player_id, mode);
    let mut elapsed_ms = turns.elapsed_ms.clone();
    if let Some(active) = turns.active_player_id {
        *elapsed_ms.entry(active).or_insert(0) += elapsed - turns.started_elapsed_ms;
    }
    let mut counts = turns.counts.clone();
    if let Some(next) = next {
        *counts.entry(next).or_insert(0) += 1;
    }
    let history = match turns.active_player_id {
        // A turn that starts from no active player has no one to hand it back to.
        None => turns.history.clone(),
        Some(active) => {
            let entry = Pass { player_id: active, started_elapsed_ms: turns.started_elapsed_ms, next_player_id: next };
            std::iter::once(entry).chain(turns.history.iter().cloned()).take(HISTORY_LIMIT).collect()
        }
    };
    Turns {
        active_player_id: next,
        counts,
        elapsed_ms,
        started_elapsed_ms: elapsed,
        revision: turns.revision + 1,
        history,
    }
}

/// Reverses the most recent pass: the previous player's turn resumes from when it originally
/// started, the time banked by that pass is removed, and the receiving player's count goes
/// back down. Refuses when the last pass does not lead to the current player or the previous
/// player is now out.
pub fn unpass<S: TurnSeat>(turns: &Turns, seats: &[S], mode: Mode) -> Option<Turns> {
    let (last, rest) = turns.history.split_first()?;
    if last.next_player_id != turns.active_player_id {
        return None;
    }
    if !units(seats, mode).iter().any(|unit| unit.player_id == last.player_id && eligible(unit)) {
        return None;
    }
    let banked = turns.started_elapsed_ms - last.started_elapsed_ms;
    let mut counts = turns.counts.clone();
    if let Some(next) = last.next_player_id {
        let count = counts.entry(next).or_insert(1);
        *count = (*count - 1).max(0);
    }
    let mut elapsed_ms = turns.elapsed_ms.clone();
    let banked_time = elapsed_ms.entry(last.player_id).or_insert(banked);
    *banked_time = (*banked_time - banked).max(0);
    Some(Turns {
        active_player_id: Some(last.player_id),
        counts,
        elapsed_ms,
        started_elapsed_ms: last.started_elapsed_ms,
        revision: turns.revision + 1,
        history: rest.to_vec(),
    })
}

/// Moves the turn on when the active player can no longer take it, or starts it when a seat
/// becomes eligible again.
pub fn reconcile<S: TurnSeat>(turns: &Turns, seats: &[S], elapsed: i64, mode: Mode) -> Turns {
    let active = units(seats, mode).iter().any(|unit| Some(unit.player_id) == turns.active_player_id && eligible(unit));
    if active || (turns.active_player_id.is_none() && next_player(seats, None, mode).is_none()) {
        turns.clone()
    } else {
        pass(turns, seats, elapsed, mode)
    }
}

/// Corrects a player's turn count, clamped to 0..=999.
pub fn adjust(turns: &Turns, player_id: i64, delta: i64) -> Turns {
    let mut counts = turns.counts.clone();
    let count = (counts.get(&player_id).copied().unwrap_or(0) + delta).clamp(0, 999);
    counts.insert(player_id, count);
    Turns { counts, ..turns.clone() }
}
