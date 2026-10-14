//! The single writer for `/newgame` queue interactions and Discord edits
//! (`NewGameScheduler`, `NewGameDelivery`). Ticks carry no game state: every sweep reloads
//! due and dirty queues from the database, so work survives restarts.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::db::UtcDateTime;
use crate::state::AppState;

use super::api::{DiscordApi, DiscordError, MessagePayload, SentMessage};
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

    /// Applies a queue action, then delivers its edits. A start time moved to another day
    /// renames the game's thread.
    pub async fn act(
        &self,
        id: i64,
        action: &QueueAction,
        actor: &QueueActor,
    ) -> Result<ScheduledGame, QueueError> {
        let (game, old_name) = {
            let _guard = self.lock.lock().await;
            let old_name = match action {
                QueueAction::Time(_) => scheduled::get(&mut *self.state.pool.acquire().await?, id)
                    .await?
                    .map(|game| self.thread_name(&game)),
                _ => None,
            };
            let game = scheduled::act(&self.state, id, action, actor, (self.clock)()).await?;
            let listed = game.players.contains_key(&actor.discord_id)
                || game.maybe.contains_key(&actor.discord_id);
            if let (Some(thread), QueueAction::Join | QueueAction::Maybe, true) =
                (&game.thread_id, action, listed)
            {
                self.add_to_thread(game.id, thread, &actor.discord_id).await;
            }
            let _ = self.deliver(&game).await;
            (game, old_name)
        };
        if old_name.is_some_and(|old| old != self.thread_name(&game)) {
            self.rename_thread(&game).await;
        }
        Ok(game)
    }

    /// Publishes a queue from its `/newgame` reply. Run inside a game thread, the reply is
    /// the queue and joins that thread. Otherwise, when Discord allows a thread on the
    /// reply, the queue goes in the new thread and the reply points to it; without one the
    /// reply itself shows the queue.
    pub async fn attach(
        &self,
        game: &ScheduledGame,
        reply_id: &str,
        existing_thread: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let game = {
            let _guard = self.lock.lock().await;
            let pool = &self.state.pool;
            let (thread, message_id) = match existing_thread {
                Some(thread) => {
                    self.add_to_thread(game.id, thread, &game.host_discord_id)
                        .await;
                    (Some(thread), reply_id.to_owned())
                }
                None => match self.open_thread(game, reply_id).await {
                    Some(queue_id) => (Some(reply_id), queue_id),
                    None => (None, reply_id.to_owned()),
                },
            };
            if let Some(thread) = thread {
                scheduled::set_thread(pool, game.id, thread).await?;
            }
            scheduled::attach_message(pool, game.id, &message_id).await?;
            let game = scheduled::settle_id(pool, game.id, (self.clock)()).await?;
            let _ = self.deliver(&game).await;
            if thread.is_some() && existing_thread.is_none() {
                let pointer = new_game_message::pointer(&game);
                if let Err(error) = self
                    .api
                    .edit_message(&game.channel_id, reply_id, &pointer)
                    .await
                {
                    tracing::warn!(
                        "Discord newgame {} reply could not be edited: {error}",
                        game.id
                    );
                }
            }
            game
        };
        if existing_thread.is_some() {
            self.rename_thread(&game).await;
        }
        Ok(())
    }

    /// Starts a thread on the `/newgame` reply (Discord gives it the reply's id), posts the
    /// queue's placeholder in it, and adds the host. `None` when Discord refuses, for
    /// example without the thread permissions.
    async fn open_thread(&self, game: &ScheduledGame, reply_id: &str) -> Option<String> {
        let started = self
            .api
            .start_thread(&game.channel_id, reply_id, &self.thread_name(game))
            .await;
        let posted = match started {
            Ok(()) => {
                self.api
                    .create_message(reply_id, &new_game_message::placeholder())
                    .await
            }
            Err(error) => Err(error),
        };
        match posted {
            Ok(message) => {
                self.add_to_thread(game.id, reply_id, &game.host_discord_id)
                    .await;
                Some(message.id)
            }
            Err(error) => {
                tracing::warn!(
                    "Discord newgame {} thread could not be opened: {error}",
                    game.id
                );
                None
            }
        }
    }

    /// Cancels the open games in a thread (`/close`); returns how many.
    pub async fn close(&self, thread_id: &str) -> Result<usize, sqlx::Error> {
        let _guard = self.lock.lock().await;
        let games = scheduled::cancel_thread(&self.state.pool, thread_id, (self.clock)()).await?;
        for game in &games {
            let _ = self.deliver(game).await;
        }
        Ok(games.len())
    }

    fn thread_name(&self, game: &ScheduledGame) -> String {
        new_game_message::thread_name(game, &self.state.config.discord_default_timezone)
    }

    /// Names the game's thread after it. Runs outside the lock: Discord rate-limits channel
    /// renames tightly, so a rename can wait out a long rate limit.
    async fn rename_thread(&self, game: &ScheduledGame) {
        let Some(thread) = &game.thread_id else {
            return;
        };
        if let Err(error) = self
            .api
            .rename_thread(thread, &self.thread_name(game))
            .await
        {
            tracing::warn!(
                "Discord newgame {} thread could not be renamed: {error}",
                game.id
            );
        }
    }

    async fn add_to_thread(&self, id: i64, thread_id: &str, discord_id: &str) {
        if let Err(error) = self.api.add_thread_member(thread_id, discord_id).await {
            tracing::warn!("Discord newgame {id} could not add a member to its thread: {error}");
        }
    }

    /// Posts in the game's thread, or in its channel when it has none or Discord refuses
    /// the thread post (deleted, or locked by a moderator).
    async fn post(
        &self,
        game: &ScheduledGame,
        payload: &MessagePayload,
    ) -> Result<SentMessage, DiscordError> {
        if let Some(thread) = &game.thread_id {
            match self.api.create_message(thread, payload).await {
                Err(DiscordError::Http {
                    status: 400..=428 | 430..=499,
                    ..
                }) => tracing::warn!(
                    "Discord newgame {} thread refused a post; posting in the channel",
                    game.id
                ),
                result => return result,
            }
        }
        self.api.create_message(&game.channel_id, payload).await
    }

    /// Settles due queues and retries dirty messages.
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

    /// Posts the announcement or maybe ping (each recorded
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
            let payload = new_game_message::announcement(public_url, &game);
            let sent = self
                .post(&game, &payload)
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
                .post(&game, &payload)
                .await
                .map_err(|_| DeliveryFailed)?;
            scheduled::set_maybe_ping(pool, game.id, &sent.id)
                .await
                .map_err(|_| DeliveryFailed)?;
            game.maybe_ping_id = Some(sent.id);
        }
        let payload = new_game_message::render(public_url, &game);
        self.api
            .edit_message(game.queue_channel(), message_id, &payload)
            .await
            .map_err(|_| DeliveryFailed)?;
        scheduled::mark_clean(pool, game.id)
            .await
            .map_err(|_| DeliveryFailed)
    }
}
