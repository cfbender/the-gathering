//! `TheGathering.WebcamTables.Timer`: the pure shared game clock.
//!
//! The server stamps every transition, so clients cannot forge times. Repeated actions are
//! idempotent: starting again never resets and resuming a running clock changes nothing.
//! Starting stamps the game as begun but holds the clock at zero, so players can mulligan;
//! the first resume begins play.

use serde::{Deserialize, Serialize};

/// A clock transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Stamp the game as started (held at zero).
    Start,
    /// Pause a running clock.
    Pause,
    /// Resume a paused (or held) clock.
    Resume,
}

impl Action {
    /// The client's names for pause and resume (`start` is server-only).
    pub fn parse_client(action: &str) -> Option<Self> {
        match action {
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            _ => None,
        }
    }
}

/// The clock's state, in milliseconds since the epoch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    /// When the game started.
    #[serde(default)]
    pub started_at: Option<i64>,
    /// When the clock was paused, if it is.
    #[serde(default)]
    pub paused_at: Option<i64>,
    /// Total paused time before `paused_at`.
    #[serde(default)]
    pub paused_ms: i64,
}

impl Timer {
    /// A clock that has not started.
    pub fn new() -> Self {
        Self::default()
    }

    /// Milliseconds of play so far, excluding pauses.
    pub fn elapsed(&self, now: i64) -> i64 {
        match self.started_at {
            None => 0,
            Some(started) => (self.paused_at.unwrap_or(now) - started - self.paused_ms).max(0),
        }
    }

    /// True between the start of the game and the first time its clock runs.
    pub fn awaiting_start(&self) -> bool {
        self.started_at.is_some() && self.started_at == self.paused_at
    }

    /// Applies `action` at `now`.
    #[must_use]
    pub fn update(self, action: Action, now: i64) -> Self {
        match (self.started_at, self.paused_at, action) {
            (None, _, Action::Start) => Self {
                started_at: Some(now),
                paused_at: Some(now),
                ..self
            },
            (Some(_), None, Action::Pause) => Self {
                paused_at: Some(now),
                ..self
            },
            (Some(_), Some(paused), Action::Resume) => Self {
                paused_at: None,
                paused_ms: self.paused_ms + now - paused,
                ..self
            },
            _ => self,
        }
    }
}
