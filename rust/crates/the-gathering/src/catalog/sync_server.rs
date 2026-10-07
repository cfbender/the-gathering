//! One catalog sync at a time, on demand or on a schedule
//! (`TheGathering.Catalog.SyncServer`).

use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use super::sync::{self, Source};
use crate::state::AppState;

/// What `trigger` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// A sync started.
    Started,
    /// One was already running.
    AlreadyRunning,
}

impl Trigger {
    /// `started` or `already_running`, as the API renders it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::AlreadyRunning => "already_running",
        }
    }
}

/// Tracks the running sync and tells the scheduler when one finishes.
#[derive(Debug)]
pub struct SyncServer {
    running: Mutex<bool>,
    finished: watch::Sender<u64>,
}

impl Default for SyncServer {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncServer {
    /// Idle.
    pub fn new() -> Self {
        Self {
            running: Mutex::new(false),
            finished: watch::channel(0).0,
        }
    }

    /// Whether a sync is running.
    pub fn running(&self) -> bool {
        self.running.lock().is_ok_and(|running| *running)
    }

    /// Resolves after the next sync finishes.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.finished.subscribe()
    }

    /// `trigger/0`: starts a Scryfall sync in the background unless one is running.
    pub fn trigger(&self, state: &AppState) -> Trigger {
        self.start(state, Source::Scryfall)
    }

    /// Starts a sync of `source` in the background unless one is running.
    pub fn start(&self, state: &AppState, source: Source) -> Trigger {
        {
            let Ok(mut running) = self.running.lock() else {
                return Trigger::AlreadyRunning;
            };
            if *running {
                return Trigger::AlreadyRunning;
            }
            *running = true;
        }
        let state = state.clone();
        tokio::spawn(async move {
            let outcome = tokio::spawn({
                let state = state.clone();
                async move { sync::run(&state.pool, &state.scryfall, source).await }
            })
            .await;
            if let Err(error) = outcome {
                tracing::error!("catalog sync task crashed: {error}");
            }
            if let Ok(mut running) = state.catalog_sync.running.lock() {
                *running = false;
            }
            state
                .catalog_sync
                .finished
                .send_modify(|count| *count = count.wrapping_add(1));
        });
        Trigger::Started
    }
}

/// Starts the scheduled sync when `catalog_sync_enabled`: the first run after one second
/// when the catalog is empty, else after `catalog_sync_interval`, and then
/// `catalog_sync_interval` after each sync (scheduled or triggered) finishes. A scheduled
/// run that comes due while another is running is skipped.
pub fn start(state: &AppState) {
    if !state.config.catalog_sync_enabled {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let interval = state.config.catalog_sync_interval;
        let empty = super::Catalog {
            pool: state.pool.clone(),
        }
        .count_cards()
        .await
        .unwrap_or(0)
            == 0;
        let idle = Duration::from_hours(365 * 24);
        let mut deadline = Instant::now()
            + if empty {
                Duration::from_secs(1)
            } else {
                interval
            };
        let mut finished = state.catalog_sync.subscribe();
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => {
                    deadline = Instant::now() + idle;
                    state.catalog_sync.trigger(&state);
                }
                changed = finished.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    deadline = Instant::now() + interval;
                }
            }
        }
    });
}
