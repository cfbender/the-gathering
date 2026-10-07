//! Live rooms where remote players share cameras, life totals,
//! turns, a shared clock, identified cards and a shared table log.
//!
//! Each room runs as its own task ([`room::Room`]), started on first join and registered by
//! room id. Rooms persist every change to [`session`] and keep running with no connections;
//! the pruner closes rooms that stay empty and idle for 30 minutes and deletes expired
//! sessions.
//!
//! Mutations that act for a seat carry the joined connection's id: the room identifies the
//! seat's current connection by it and sends it [`room::ConnEvent`]s.

pub mod cards;
pub mod log;
pub mod room;
pub mod seat;
pub mod session;
pub mod timer;
pub mod turns;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio::task::AbortHandle;

use crate::db::Pool;
use crate::web::channels::pubsub::PubSub;

use self::cards::Change;
use self::log::{LogEntry, RollKind, RollResult};
use self::room::{Actor, Admitted, Conn, Room, RoomDebug, RoomMsg, Snapshot, TimerState};
use self::seat::{Holder, Seat};
use self::timer::Action;

/// How often the pruner runs.
pub const PRUNE_INTERVAL: Duration = Duration::from_secs(60);
/// How long an empty room may sit idle before the pruner closes it.
pub const IDLE_TIMEOUT_MS: i64 = 30 * 60 * 1000;

/// Milliseconds since the epoch (`System.system_time(:millisecond)`).
pub fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis()),
    )
    .unwrap_or(i64::MAX)
}

/// The table's game mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Free-for-all Commander.
    #[default]
    Commander,
    /// Teams of two with shared life.
    TwoHeadedGiant,
    /// Exactly five players.
    FiveStar,
}

impl Mode {
    /// Parses the wire name.
    pub fn parse(mode: &str) -> Option<Self> {
        match mode {
            "commander" => Some(Self::Commander),
            "two_headed_giant" => Some(Self::TwoHeadedGiant),
            "five_star" => Some(Self::FiveStar),
            _ => None,
        }
    }
}

/// The room stopped (or crashed) before answering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("webcam table room is not running")]
pub struct RoomGone;

/// A running room, as listed by [`WebcamTables::rooms`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomInfo {
    /// Room id.
    pub id: String,
    /// When the room started (ms).
    pub opened_at: i64,
}

#[derive(Clone, Debug)]
struct RoomHandle {
    instance: u64,
    tx: mpsc::UnboundedSender<RoomMsg>,
    opened_at: i64,
    abort: AbortHandle,
}

#[derive(Debug)]
pub(crate) struct Shared {
    pool: Pool,
    pubsub: PubSub,
    rooms: Mutex<HashMap<String, RoomHandle>>,
    next_instance: AtomicU64,
}

impl Shared {
    fn rooms(&self) -> MutexGuard<'_, HashMap<String, RoomHandle>> {
        self.rooms
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Forgets a stopped room (unless a newer room took its id).
    fn deregister(&self, id: &str, instance: u64) {
        let mut rooms = self.rooms();
        if rooms
            .get(id)
            .is_some_and(|handle| handle.instance == instance)
        {
            rooms.remove(id);
        }
    }
}

/// The room registry. Cheap to clone.
#[derive(Clone, Debug)]
pub struct WebcamTables(Arc<Shared>);

impl WebcamTables {
    /// No rooms yet.
    pub fn new(pool: Pool, pubsub: PubSub) -> Self {
        Self(Arc::new(Shared {
            pool,
            pubsub,
            rooms: Mutex::new(HashMap::new()),
            next_instance: AtomicU64::new(1),
        }))
    }

    fn live(&self, id: &str) -> Option<RoomHandle> {
        self.0
            .rooms()
            .get(id)
            .filter(|handle| !handle.tx.is_closed())
            .cloned()
    }

    fn start_room(&self, id: &str) -> RoomHandle {
        let mut rooms = self.0.rooms();
        if let Some(handle) = rooms.get(id).filter(|handle| !handle.tx.is_closed()) {
            return handle.clone();
        }
        let instance = self.0.next_instance.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        let room = Room::new(id.to_owned(), instance, Arc::clone(&self.0), tx.downgrade());
        let abort = tokio::spawn(room.run(rx)).abort_handle();
        let handle = RoomHandle {
            instance,
            tx,
            opened_at: now(),
            abort,
        };
        rooms.insert(id.to_owned(), handle.clone());
        handle
    }

    async fn ask<T>(
        handle: &RoomHandle,
        message: impl FnOnce(oneshot::Sender<T>) -> RoomMsg,
    ) -> Result<T, RoomGone> {
        let (reply, answer) = oneshot::channel();
        handle.tx.send(message(reply)).map_err(|_| RoomGone)?;
        answer.await.map_err(|_| RoomGone)
    }

    async fn call<T>(
        &self,
        room: &str,
        message: impl FnOnce(oneshot::Sender<T>) -> RoomMsg,
    ) -> Result<T, RoomGone> {
        let handle = self.live(room).ok_or(RoomGone)?;
        Self::ask(&handle, message).await
    }

    /// Admits `participant` with `conn` as its connection, starting the room if needed.
    ///
    /// Returns the table snapshot, the admitted participant (a returning player gets their
    /// saved seat back; late arrivals become spectators) and a watch on the room's end.
    pub async fn join(
        &self,
        room: &str,
        participant: Seat,
        conn: Conn,
    ) -> Result<Result<Admitted, String>, RoomGone> {
        let mut attempts = 3;
        loop {
            let handle = self.start_room(room);
            let participant = Box::new(participant.clone());
            let conn = conn.clone();
            match Self::ask(&handle, |reply| RoomMsg::Join(participant, conn, reply)).await {
                // The room closed as idle just before this join reached it; start anew.
                Err(RoomGone) if attempts > 1 => attempts -= 1,
                other => return other,
            }
        }
    }

    /// The table.
    pub async fn snapshot(&self, room: &str) -> Result<Snapshot, RoomGone> {
        self.call(room, RoomMsg::Snapshot).await
    }

    /// The shared table log, newest first.
    pub async fn log(&self, room: &str) -> Result<Vec<LogEntry>, RoomGone> {
        self.call(room, RoomMsg::Log).await
    }

    /// Records and broadcasts a seat's dice or coin roll.
    pub async fn roll(
        &self,
        room: &str,
        actor: Holder,
        kind: RollKind,
        result: RollResult,
    ) -> Result<(), RoomGone> {
        self.call(room, |reply| RoomMsg::Roll(actor, kind, result, reply))
            .await
    }

    /// Running rooms, connected or not.
    pub fn rooms(&self) -> Vec<RoomInfo> {
        self.0
            .rooms()
            .iter()
            .filter(|(_, handle)| !handle.tx.is_closed())
            .map(|(id, handle)| RoomInfo {
                id: id.clone(),
                opened_at: handle.opened_at,
            })
            .collect()
    }

    fn handles(&self) -> Vec<(String, RoomHandle)> {
        self.0
            .rooms()
            .iter()
            .map(|(id, handle)| (id.clone(), handle.clone()))
            .collect()
    }

    /// Closes every room with no connections and no activity for `idle_ms`, deleting its
    /// saved session. Returns the closed room ids.
    pub async fn close_idle_rooms(&self, idle_ms: i64) -> Vec<String> {
        let mut closed = Vec::new();
        for (id, handle) in self.handles() {
            if Self::ask(&handle, |reply| RoomMsg::CloseIfIdle(idle_ms, reply)).await == Ok(true) {
                closed.push(id);
            }
        }
        closed
    }

    /// Whether `player_id` holds a seat at any running table, in the lobby or a started game.
    /// Seats stay taken until the room closes, even after their player disconnects. Do not
    /// call this inside a write transaction: rooms write their sessions while answering.
    pub async fn seated(&self, player_id: i64) -> bool {
        for (_, handle) in self.handles() {
            if Self::ask(&handle, |reply| RoomMsg::Seated(player_id, reply)).await == Ok(true) {
                return true;
            }
        }
        false
    }

    /// Ends the table for everyone. The saved session is deleted and the room stops.
    pub async fn close(&self, room: &str) -> Result<(), RoomGone> {
        self.call(room, RoomMsg::Close).await
    }

    /// Resets the table to a fresh lobby for a rematch.
    pub async fn rematch(&self, room: &str) -> Result<(), RoomGone> {
        self.call(room, RoomMsg::Rematch).await
    }

    /// Whether connection `conn_id` is `player_id`'s current connection.
    pub async fn current(
        &self,
        room: &str,
        player_id: i64,
        conn_id: u64,
    ) -> Result<bool, RoomGone> {
        self.call(room, |reply| RoomMsg::Current(player_id, conn_id, reply))
            .await
    }

    /// Records the calling connection's seat (life, counters, deck, reveal).
    pub async fn remember_seat(
        &self,
        room: &str,
        participant: Seat,
        conn_id: u64,
    ) -> Result<(), RoomGone> {
        self.call(room, |reply| {
            RoomMsg::RememberSeat(Box::new(participant), conn_id, reply)
        })
        .await
    }

    /// Reorders seats (Commander); starts the clock if needed.
    pub async fn order(&self, room: &str, peer_ids: Vec<String>) -> Result<(), RoomGone> {
        self.call(room, |reply| RoomMsg::Order(peer_ids, reply))
            .await
    }

    /// Arranges lobby seats before the game starts.
    pub async fn arrange(
        &self,
        room: &str,
        peer_ids: Vec<String>,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::Arrange(peer_ids, reply))
            .await
    }

    /// Changes the game mode before the start.
    pub async fn set_mode(&self, room: &str, mode: Mode) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::SetMode(mode, reply)).await
    }

    /// Changes a started Two-Headed Giant team's shared life.
    pub async fn adjust_team_life(
        &self,
        room: &str,
        actor: Actor,
        team_index: i64,
        delta: i64,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| {
            RoomMsg::TeamLife(actor, team_index, delta, reply)
        })
        .await
    }

    /// Eliminates or restores the seat with `peer_id` (its whole team in Two-Headed Giant).
    pub async fn eliminate(
        &self,
        room: &str,
        peer_id: &str,
        eliminated: bool,
    ) -> Result<(), RoomGone> {
        let peer_id = peer_id.to_owned();
        self.call(room, |reply| RoomMsg::Eliminate(peer_id, eliminated, reply))
            .await
    }

    /// Pauses or resumes the clock.
    pub async fn timer(&self, room: &str, action: Action) -> Result<TimerState, RoomGone> {
        self.call(room, |reply| RoomMsg::Timer(action, reply)).await
    }

    /// Ends the mulligan window by starting the game clock.
    pub async fn begin_play(
        &self,
        room: &str,
        actor: Actor,
    ) -> Result<Result<TimerState, String>, RoomGone> {
        self.call(room, |reply| RoomMsg::BeginPlay(actor, reply))
            .await
    }

    /// Starts the game; `randomize` overrides the room's auto-randomize setting.
    pub async fn start_game(
        &self,
        room: &str,
        randomize: Option<bool>,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::StartGame(randomize, reply))
            .await
    }

    /// Sets auto-randomize.
    pub async fn turn_settings(&self, room: &str, auto_randomize: bool) -> Result<(), RoomGone> {
        self.call(room, |reply| RoomMsg::TurnSettings(auto_randomize, reply))
            .await
    }

    /// Passes the turn if `revision` is current.
    pub async fn pass_turn(
        &self,
        room: &str,
        revision: i64,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::PassTurn(revision, reply))
            .await
    }

    /// Undoes the last pass if `revision` is current.
    pub async fn unpass_turn(
        &self,
        room: &str,
        revision: i64,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::UnpassTurn(revision, reply))
            .await
    }

    /// Corrects a player's turn count.
    pub async fn adjust_turn(
        &self,
        room: &str,
        player_id: i64,
        delta: i64,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::AdjustTurn(player_id, delta, reply))
            .await
    }

    /// Makes `holder` the monarch on behalf of `actor`, who may be the holder themselves.
    pub async fn take_monarch(
        &self,
        room: &str,
        holder: Holder,
        actor: Holder,
    ) -> Result<(), RoomGone> {
        self.call(room, |reply| RoomMsg::Monarch(holder, actor, reply))
            .await
    }

    /// Applies a card list change on behalf of `actor`.
    pub async fn cards(
        &self,
        room: &str,
        change: Change,
        actor: Holder,
    ) -> Result<Result<(), String>, RoomGone> {
        self.call(room, |reply| RoomMsg::Cards(change, actor, reply))
            .await
    }

    /// Deletes expired sessions.
    pub async fn prune_sessions(&self) -> Result<u64, sqlx::Error> {
        session::prune(&self.0.pool).await
    }

    /// Runs the pruner forever: every minute, closes rooms idle for 30 minutes and deletes
    /// expired sessions.
    pub fn spawn_pruner(&self) -> tokio::task::JoinHandle<()> {
        let tables = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval_at(
                tokio::time::Instant::now() + PRUNE_INTERVAL,
                PRUNE_INTERVAL,
            );
            loop {
                interval.tick().await;
                tables.close_idle_rooms(IDLE_TIMEOUT_MS).await;
                if let Err(error) = tables.prune_sessions().await {
                    tracing::warn!("could not prune webcam table sessions: {error}");
                }
            }
        })
    }

    /// The running room's instance number (a new number after a restart). For tests.
    #[doc(hidden)]
    pub fn instance(&self, room: &str) -> Option<u64> {
        self.live(room).map(|handle| handle.instance)
    }

    /// Kills the room's task without cleanup, like `Process.exit(pid, :kill)`. For tests.
    #[doc(hidden)]
    pub fn kill(&self, room: &str) {
        if let Some(handle) = self.0.rooms().remove(room) {
            handle.abort.abort();
        }
    }

    /// The room's connections and departing players. For tests.
    #[doc(hidden)]
    pub async fn debug(&self, room: &str) -> Result<RoomDebug, RoomGone> {
        self.call(room, RoomMsg::Debug).await
    }

    /// Ends a player's departure grace period now, as its timer would. For tests.
    #[doc(hidden)]
    pub fn depart(&self, room: &str, player_id: i64, token: u64) {
        if let Some(handle) = self.live(room) {
            let _ = handle.tx.send(RoomMsg::Departed(player_id, token));
        }
    }
}

/// A player as the table needs it (`Games.get_player/1`).
#[derive(Clone, Debug)]
pub struct Player {
    /// Id.
    pub id: i64,
    /// Name.
    pub name: String,
    /// Linked account.
    pub user_id: Option<i64>,
}

/// A deck as the table needs it (`Games.get_deck/1`).
#[derive(Clone, Debug)]
pub struct Deck {
    /// Id.
    pub id: i64,
    /// Owner.
    pub player_id: i64,
    /// Name.
    pub name: String,
}

/// Looks up a player. Swap for the games module's API when it lands.
pub async fn get_player(pool: &Pool, id: i64) -> Result<Option<Player>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id AS "id!", name, user_id FROM players WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| Player {
        id: row.id,
        name: row.name,
        user_id: row.user_id,
    }))
}

/// Looks up a deck. Swap for the games module's API when it lands.
pub async fn get_deck(pool: &Pool, id: i64) -> Result<Option<Deck>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id AS "id!", player_id, name FROM decks WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| Deck {
        id: row.id,
        player_id: row.player_id,
        name: row.name,
    }))
}
