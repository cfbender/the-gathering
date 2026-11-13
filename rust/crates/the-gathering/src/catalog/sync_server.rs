//! One catalog sync at a time, on demand or on a schedule.

use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use super::sync::{self, Source};
use crate::db::{Pool, UtcDateTime};
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

    /// Starts a Scryfall sync in the background unless one is running.
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

const SOON: Duration = Duration::from_secs(1);

/// How long to wait before the first scheduled sync at boot: `SOON` when the catalog
/// [needs a sync](super::Catalog::needs_sync), has no recorded successful sync, or the last
/// successful sync's `last_finished_at + interval` is due; otherwise the remaining time
/// until `last_finished_at + interval` (clamped to `[SOON, interval]`). Failed runs do not
/// advance the last successful finish time, so a restart after a failure retries when the
/// last good sync's interval is due.
async fn initial_delay(pool: &Pool, interval: Duration, now: UtcDateTime) -> Duration {
    let needed = super::Catalog { pool: pool.clone() }
        .needs_sync()
        .await
        .unwrap_or(true);
    if needed {
        return SOON;
    }
    let Some(last_finished) = sync::last_succeeded_at(pool).await.ok().flatten() else {
        return SOON;
    };
    let elapsed_secs = now.unix().saturating_sub(last_finished.unix());
    let elapsed = u64::try_from(elapsed_secs).map_or(Duration::ZERO, Duration::from_secs);
    let remaining = interval.saturating_sub(elapsed);
    remaining.clamp(SOON, interval)
}

/// Starts the scheduled sync when `catalog_sync_enabled`: the first run when the last
/// successful sync's `last_finished_at + catalog_sync_interval` comes due (or after one
/// second when already past, never synced, or the catalog
/// [needs a sync](super::Catalog::needs_sync)), and then `catalog_sync_interval` after each
/// sync (scheduled or triggered) finishes. A scheduled run that comes due while another is
/// running is skipped.
pub fn start(state: &AppState) {
    if !state.config.catalog_sync_enabled {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let interval = state.config.catalog_sync_interval;
        let idle = Duration::from_hours(365 * 24);
        let mut deadline =
            Instant::now() + initial_delay(&state.pool, interval, UtcDateTime::now()).await;
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
