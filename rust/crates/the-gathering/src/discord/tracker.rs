//! Stages observed SpellBot reports so a winner can complete them later.
//!
//! A mutex serializes calls, so concurrent reports and winners for one game do not race.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::db::{self, Pool, UtcDateTime};

use super::pending::{self, ResolveError, StageError};
use super::report::GameReport;
use super::sink::{Sink, SinkError};

/// Why an observation was not staged.
#[derive(Debug, thiserror::Error)]
pub enum ObserveError {
    /// The sink rejected the report.
    #[error("{0}")]
    Sink(SinkError),
    /// Staging failed.
    #[error("{0}")]
    Stage(StageError),
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The tracker.
pub struct Tracker {
    pool: Pool,
    sink: Arc<dyn Sink>,
    lock: Mutex<()>,
}

impl std::fmt::Debug for Tracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tracker").finish_non_exhaustive()
    }
}

/// `12345`, `sb12345` → `spellbot:SB12345`.
pub fn normalize_game_id(game_id: &str) -> String {
    let game_id = game_id.trim().to_uppercase();
    if game_id.starts_with("SB") {
        format!("spellbot:{game_id}")
    } else {
        format!("spellbot:SB{game_id}")
    }
}

impl Tracker {
    /// A tracker dispatching to `sink`.
    pub fn new(pool: Pool, sink: Arc<dyn Sink>) -> Self {
        Self {
            pool,
            sink,
            lock: Mutex::new(()),
        }
    }

    /// Dispatches, stages, then prunes old staged games.
    pub async fn observe(&self, report: &GameReport) -> Result<(), ObserveError> {
        let _guard = self.lock.lock().await;
        let mut tx = db::begin(&self.pool).await?;
        if let Err(error) = self.sink.handle_report(&mut tx, report).await {
            tx.rollback().await?;
            return Err(ObserveError::Sink(error));
        }
        tx.commit().await?;
        let mut tx = db::begin(&self.pool).await?;
        pending::stage(&mut tx, report)
            .await
            .map_err(ObserveError::Stage)?;
        pending::prune(&mut tx, UtcDateTime::now()).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Records the staged game with `discord_id` as the winner.
    pub async fn record_winner(
        &self,
        game_id: &str,
        discord_id: &str,
    ) -> Result<GameReport, ResolveError> {
        let _guard = self.lock.lock().await;
        let external_id = normalize_game_id(game_id);
        let found = pending::by_external_id(&mut *self.pool.acquire().await?, &external_id).await?;
        let pending = found.ok_or(ResolveError::UnknownGame)?;
        pending::resolve(&self.pool, &pending, discord_id, self.sink.as_ref()).await
    }

    /// The most recently started winnerless game in
    /// `channel_id`.
    pub async fn record_latest_winner(
        &self,
        channel_id: &str,
        discord_id: &str,
    ) -> Result<GameReport, ResolveError> {
        let _guard = self.lock.lock().await;
        let found =
            pending::latest_in_channel(&mut *self.pool.acquire().await?, channel_id).await?;
        let pending = found.ok_or(ResolveError::NoGameInChannel)?;
        pending::resolve(&self.pool, &pending, discord_id, self.sink.as_ref()).await
    }
}
