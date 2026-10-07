//! The single writer for `/newgame` queue interactions and Discord edits
//! (`NewGameScheduler`, `NewGameDelivery`). Ticks carry no game state: every sweep reloads
//! due and dirty queues from the database, so work survives restarts.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::db::UtcDateTime;
use crate::state::AppState;

use super::api::DiscordApi;
use super::new_game_message;
use super::scheduled::{self, QueueAction, QueueActor, QueueError, ScheduledGame, Status};

/// The current time, injectable for tests.
pub type Clock = Arc<dyn Fn() -> UtcDateTime + Send + Sync>;

/// Default sweep interval.
pub const INTERVAL: Duration = Duration::from_secs(5);

/// A delivery that will be retried on the next sweep.
#[derive(Debug, thiserror::Error)]
#[error("delivery failed")]
pub struct DeliveryFailed;

/// The scheduler.
pub struct NewGameScheduler {
    state: AppState,
    api: Arc<dyn DiscordApi>,
    clock: Clock,
    lock: Mutex<()>,
}

impl std::fmt::Debug for NewGameScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewGameScheduler").finish_non_exhaustive()
    }
}

impl NewGameScheduler {
    /// A scheduler using the wall clock.
    pub fn new(state: AppState, api: Arc<dyn DiscordApi>) -> Self {
        Self::with_clock(state, api, Arc::new(UtcDateTime::now))
    }

    /// A scheduler using `clock`.
    pub fn with_clock(state: AppState, api: Arc<dyn DiscordApi>, clock: Clock) -> Self {
        Self {
            state,
            api,
            clock,
            lock: Mutex::new(()),
        }
    }

    /// Sweeps at once (reloading pending work on every boot), then every `interval`.
    pub fn spawn(self: &Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
        let scheduler = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                scheduler.sweep().await;
                tokio::time::sleep(interval).await;
            }
        })
    }

    /// `NewGameScheduler.act/4`: applies a queue action, then delivers its edits.
    pub async fn act(
        &self,
        id: i64,
        action: &QueueAction,
        actor: &QueueActor,
    ) -> Result<ScheduledGame, QueueError> {
        let _guard = self.lock.lock().await;
        let game = scheduled::act(&self.state, id, action, actor, (self.clock)()).await?;
        let _ = self.deliver(&game).await;
        Ok(game)
    }

    /// `NewGameScheduler.attach/3`: publishes a queue on its placeholder message.
    pub async fn attach(&self, id: i64, message_id: &str) -> Result<(), sqlx::Error> {
        let _guard = self.lock.lock().await;
        scheduled::attach_message(&self.state.pool, id, message_id).await?;
        let game = scheduled::settle_id(&self.state.pool, id, (self.clock)()).await?;
        let _ = self.deliver(&game).await;
        Ok(())
    }

    /// `NewGameScheduler.sweep/1`: settles due queues and retries dirty messages.
    pub async fn sweep(&self) {
        let _guard = self.lock.lock().await;
        if let Err(error) = self.run().await {
            tracing::error!("Discord newgame sweep failed: {error}");
        }
    }

    async fn run(&self) -> Result<(), sqlx::Error> {
        let now = (self.clock)();
        let mut after = 0;
        loop {
            let ids = scheduled::pending_ids(&self.state.pool, now, after).await?;
            for id in &ids {
                let game = scheduled::settle_id(&self.state.pool, *id, now).await?;
                let _ = self.deliver(&game).await;
            }
            match ids.last() {
                Some(last) if ids.len() == 100 => after = *last,
                _ => return Ok(()),
            }
        }
    }

    /// `NewGameDelivery.deliver/2`: posts the announcement or maybe ping (each recorded
    /// before the embed edit, so an edit failure never re-pings), then edits the queue.
    async fn deliver(&self, game: &ScheduledGame) -> Result<(), DeliveryFailed> {
        let Some(message_id) = &game.message_id else {
            return Ok(());
        };
        if !game.message_dirty {
            return Ok(());
        }
        let result = self.deliver_dirty(game, message_id).await;
        if result.is_err() {
            // Never log API bodies or interaction tokens. Stays dirty for the next sweep.
            tracing::warn!(
                "Discord newgame {} notification failed; will retry",
                game.id
            );
        }
        result
    }

    async fn deliver_dirty(
        &self,
        game: &ScheduledGame,
        message_id: &str,
    ) -> Result<(), DeliveryFailed> {
        let pool = &self.state.pool;
        let public_url = self.state.config.public_url();
        let mut game = game.clone();
        if game.status == Status::Started && game.announcement_id.is_none() {
            let payload = new_game_message::announcement(&public_url, &game);
            let sent = self
                .api
                .create_message(&game.channel_id, &payload)
                .await
                .map_err(|_| DeliveryFailed)?;
            scheduled::set_announcement(pool, game.id, &sent.id)
                .await
                .map_err(|_| DeliveryFailed)?;
            game.announcement_id = Some(sent.id);
        }
        if game.status == Status::Open
            && game.maybe_ping_id.is_none()
            && game.maybe_pinged_at.is_some()
            && !game.maybe.is_empty()
        {
            let payload = new_game_message::maybe_ping(&game);
            let sent = self
                .api
                .create_message(&game.channel_id, &payload)
                .await
                .map_err(|_| DeliveryFailed)?;
            scheduled::set_maybe_ping(pool, game.id, &sent.id)
                .await
                .map_err(|_| DeliveryFailed)?;
            game.maybe_ping_id = Some(sent.id);
        }
        let payload = new_game_message::render(&public_url, &game);
        self.api
            .edit_message(&game.channel_id, message_id, &payload)
            .await
            .map_err(|_| DeliveryFailed)?;
        scheduled::mark_clean(pool, game.id)
            .await
            .map_err(|_| DeliveryFailed)
    }
}
