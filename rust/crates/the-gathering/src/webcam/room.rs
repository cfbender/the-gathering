//! `TheGathering.WebcamTables.Room`: one webcam table's serialized admission and durable game
//! state, as a tokio task.
//!
//! Each room loads its saved session on start and keeps running after its last connection
//! leaves, until `close_if_idle` finds it empty and idle. Presence describes connections, not
//! seats: disconnects never change turns or erase a game. Every mutation is saved before it is
//! broadcast and acknowledged, and broadcasts preserve update order.
//!
//! Connections (channel tasks) are "monitored" through their event sender: when the channel
//! drops its receiver, a watcher tells the room. Channels watch the room through a `watch`
//! channel: [`RoomExit::Closed`] means the owner ended the table; a sender dropped without a
//! value means the room crashed.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot, watch};

use super::cards::{self, Change};
use super::log::{self, LogEntry, RollKind, RollResult};
use super::seat::{Holder, Seat};
use super::timer::{Action, Timer};
use super::turns::{self, Turns};
use super::{Mode, Shared, now};

const MAX_SEATS: usize = 10;
/// Keeps an idle but connected room (a long pause) from expiring.
const REFRESH_INTERVAL: Duration = Duration::from_secs(3600);
/// A reload drops and rejoins within this window; only a longer absence is a leave.
const DEPARTURE_GRACE: Duration = Duration::from_secs(10);
const TEAM_STARTING_LIFE: i64 = 60;

/// The room's durable state (saved in `webcam_table_sessions`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The shared clock.
    pub timer: Timer,
    /// Seat order.
    #[serde(default)]
    pub peer_ids: Vec<String>,
    /// Knocked-out seats by player id (kept after their player leaves).
    #[serde(default)]
    pub eliminated_seats: BTreeMap<i64, Seat>,
    /// Every seat by player id.
    #[serde(default)]
    pub all_seats: BTreeMap<i64, Seat>,
    /// Turn accounting.
    #[serde(default)]
    pub turns: Turns,
    /// Game mode.
    #[serde(default)]
    pub mode: Mode,
    /// Two-Headed Giant shared life by team index.
    #[serde(default)]
    pub team_life: BTreeMap<i64, i64>,
    /// Shuffle seats when the game starts.
    #[serde(default = "default_true")]
    pub auto_randomize: bool,
    /// The monarch.
    #[serde(default)]
    pub monarch: Option<Holder>,
    /// Bumped on every monarch change.
    #[serde(default)]
    pub monarch_revision: i64,
    /// Identified cards.
    #[serde(default)]
    pub cards: Vec<Value>,
    /// The shared log, newest first.
    #[serde(default)]
    pub log: Vec<LogEntry>,
    /// The player who opened the table.
    pub owner_id: i64,
}

fn default_true() -> bool {
    true
}

impl Entry {
    fn new(owner_id: i64) -> Self {
        Self {
            timer: Timer::new(),
            peer_ids: Vec::new(),
            eliminated_seats: BTreeMap::new(),
            all_seats: BTreeMap::new(),
            turns: Turns::default(),
            mode: Mode::Commander,
            team_life: BTreeMap::new(),
            auto_randomize: true,
            monarch: None,
            monarch_revision: 0,
            cards: Vec::new(),
            log: Vec::new(),
            owner_id,
        }
    }

    /// Seats in turn order: by seat order, then join time, then peer id.
    pub fn ordered_seats(&self) -> Vec<Seat> {
        let positions: HashMap<&str, usize> =
            self.peer_ids.iter().enumerate().map(|(index, id)| (id.as_str(), index)).collect();
        let mut seats: Vec<Seat> = self.all_seats.values().cloned().collect();
        seats.sort_by(|a, b| {
            let key = |seat: &Seat| (positions.get(seat.peer_id.as_str()).copied().unwrap_or(999), seat.joined_at);
            key(a).cmp(&key(b)).then_with(|| a.peer_id.cmp(&b.peer_id))
        });
        seats
    }

    fn timer_state(&self) -> TimerState {
        TimerState::new(self.timer, now())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            timer: self.timer_state(),
            peer_ids: self.peer_ids.clone(),
            seats: self.all_seats.values().cloned().collect(),
            owner_id: self.owner_id,
            monarch: MonarchState { holder: self.monarch.clone(), revision: self.monarch_revision },
            cards: self.cards.clone(),
            eliminated_seats: self.eliminated_seats.values().cloned().collect(),
            turns: self.turns.clone(),
            mode: self.mode,
            team_life: self.team_life.clone(),
            auto_randomize: self.auto_randomize,
        }
    }

    fn log(mut self, contents: Vec<LogEntry>) -> Self {
        let at = now();
        for content in contents {
            self.log = log::append(&self.log, content, at);
        }
        self
    }

    fn reconcile_turn(mut self) -> Self {
        if self.timer.started_at.is_some() {
            self.turns = turns::reconcile(&self.turns, &self.ordered_seats(), self.timer.elapsed(now()), self.mode);
        }
        self
    }
}

/// The clock as clients see it: the timer plus the server's clock for skew correction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerState {
    /// When the game started.
    pub started_at: Option<i64>,
    /// When it was paused.
    pub paused_at: Option<i64>,
    /// Paused time before `paused_at`.
    pub paused_ms: i64,
    /// The server's clock when this was sent.
    pub server_now: i64,
}

impl TimerState {
    fn new(timer: Timer, server_now: i64) -> Self {
        Self { started_at: timer.started_at, paused_at: timer.paused_at, paused_ms: timer.paused_ms, server_now }
    }

    /// The timer without `server_now`.
    pub fn timer(&self) -> Timer {
        Timer { started_at: self.started_at, paused_at: self.paused_at, paused_ms: self.paused_ms }
    }
}

/// The monarch and its revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonarchState {
    /// Who holds it.
    pub holder: Option<Holder>,
    /// Bumped on every change.
    pub revision: i64,
}

/// The shared table state sent as `table_state`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The clock.
    pub timer: TimerState,
    /// Seat order.
    pub peer_ids: Vec<String>,
    /// Seats, by player id.
    pub seats: Vec<Seat>,
    /// The opener.
    pub owner_id: i64,
    /// The monarch.
    pub monarch: MonarchState,
    /// Identified cards.
    pub cards: Vec<Value>,
    /// Knocked-out seats.
    pub eliminated_seats: Vec<Seat>,
    /// Turns.
    pub turns: Turns,
    /// Game mode.
    pub mode: Mode,
    /// Team life.
    pub team_life: BTreeMap<i64, i64>,
    /// Auto-randomize.
    pub auto_randomize: bool,
}

/// Why a room stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomExit {
    /// Closed as idle.
    Normal,
    /// The owner ended the table.
    Closed,
}

/// What the room tells a seat's connection.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnEvent {
    /// A newer connection took this seat.
    SeatReplaced,
    /// The seat was knocked out or restored.
    SeatEliminated(bool),
    /// A rematch reset the seat.
    SeatReset(Box<Seat>),
}

/// A joined channel, as the room sees it.
#[derive(Clone, Debug)]
pub struct Conn {
    /// Unique per channel.
    pub id: u64,
    /// The channel's room-event inbox; closing it is the connection going away.
    pub tx: mpsc::UnboundedSender<ConnEvent>,
}

/// Who changes a team's life or begins play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    /// A seat holding table controls.
    Owner,
    /// Any other player.
    Player(i64),
}

/// A successful join.
#[derive(Debug)]
pub struct Admitted {
    /// The table.
    pub snapshot: Snapshot,
    /// The admitted participant (a returning player's saved seat; spectator flag set).
    pub participant: Seat,
    /// Watches the room for its end.
    pub exit: watch::Receiver<Option<RoomExit>>,
}

/// Room debugging state (tests).
#[derive(Clone, Debug, Default)]
pub struct RoomDebug {
    /// Players with a live connection.
    pub connections: Vec<i64>,
    /// Players within the departure grace period: name and timer token.
    pub departing: HashMap<i64, (String, u64)>,
}

type Reply<T> = oneshot::Sender<T>;
type Outcome = Result<(), String>;

/// A request to a room.
#[derive(Debug)]
pub enum RoomMsg {
    /// Admit a participant.
    Join(Box<Seat>, Conn, Reply<Result<Admitted, String>>),
    /// The table.
    Snapshot(Reply<Snapshot>),
    /// The log.
    Log(Reply<Vec<LogEntry>>),
    /// A roll by a seat.
    Roll(Holder, RollKind, RollResult, Reply<()>),
    /// Close when empty and idle; answers whether it closed.
    CloseIfIdle(i64, Reply<bool>),
    /// Seated anywhere in this room.
    Seated(i64, Reply<bool>),
    /// End the table.
    Close(Reply<()>),
    /// Reset to a lobby.
    Rematch(Reply<()>),
    /// Whether the connection is the player's current one.
    Current(i64, u64, Reply<bool>),
    /// Record the calling connection's seat.
    RememberSeat(Box<Seat>, u64, Reply<()>),
    /// Reorder mid-game (Commander).
    Order(Vec<String>, Reply<()>),
    /// Arrange the lobby.
    Arrange(Vec<String>, Reply<Outcome>),
    /// Change the mode.
    SetMode(Mode, Reply<Outcome>),
    /// Change a team's shared life.
    TeamLife(Actor, i64, i64, Reply<Outcome>),
    /// Eliminate or restore a seat (by peer id).
    Eliminate(String, bool, Reply<()>),
    /// Pause or resume.
    Timer(Action, Reply<TimerState>),
    /// End the mulligan window.
    BeginPlay(Actor, Reply<Result<TimerState, String>>),
    /// Start the game.
    StartGame(Option<bool>, Reply<Outcome>),
    /// Auto-randomize setting.
    TurnSettings(bool, Reply<()>),
    /// Pass the turn at this revision.
    PassTurn(i64, Reply<Outcome>),
    /// Undo the last pass at this revision.
    UnpassTurn(i64, Reply<Outcome>),
    /// Correct a player's turn count.
    AdjustTurn(i64, i64, Reply<Outcome>),
    /// Give the monarch to `holder` on behalf of `actor`.
    Monarch(Holder, Holder, Reply<()>),
    /// Change the card list on behalf of `actor`.
    Cards(Change, Holder, Reply<Outcome>),
    /// A monitored connection ended.
    Down(u64),
    /// A departure grace period ended.
    Departed(i64, u64),
    /// Debugging state.
    Debug(Reply<RoomDebug>),
}

enum Broadcast {
    TableState,
    Event(&'static str, Value),
}

enum Flow {
    Continue,
    Stop(RoomExit),
}

struct Connection {
    conn: Conn,
    monitor: u64,
}

pub(super) struct Room {
    id: String,
    instance: u64,
    shared: Arc<Shared>,
    entry: Option<Entry>,
    connections: HashMap<i64, Connection>,
    monitors: HashMap<u64, (i64, String)>,
    departing: HashMap<i64, (String, u64)>,
    active_at: i64,
    next_ref: u64,
    weak_self: mpsc::WeakUnboundedSender<RoomMsg>,
    exit: watch::Sender<Option<RoomExit>>,
}

fn to_value(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn eliminated_seats_event(entry: &Entry) -> Broadcast {
    Broadcast::Event(
        "eliminated_seats",
        json!({ "participants": entry.eliminated_seats.values().collect::<Vec<_>>() }),
    )
}

fn put_eliminated(seats: &mut BTreeMap<i64, Seat>, seat: &Seat) {
    if seat.eliminated {
        seats.insert(seat.player_id, seat.clone());
    } else {
        seats.remove(&seat.player_id);
    }
}

fn roster_error(mode: Mode, count: usize) -> Option<&'static str> {
    match mode {
        Mode::TwoHeadedGiant if count < 4 || !count.is_multiple_of(2) => {
            Some("Two-Headed Giant requires an even number of players (at least 4)")
        }
        Mode::FiveStar if count != 5 => Some("Five Star requires exactly 5 players"),
        _ => None,
    }
}

fn shuffle(peers: Vec<String>, mode: Mode, randomize: bool) -> Vec<String> {
    if !randomize {
        return peers;
    }
    let mut rng = rand::rng();
    if mode == Mode::TwoHeadedGiant {
        let mut teams: Vec<Vec<String>> = peers.chunks(2).map(<[String]>::to_vec).collect();
        teams.shuffle(&mut rng);
        teams.into_iter().flatten().collect()
    } else {
        let mut peers = peers;
        peers.shuffle(&mut rng);
        peers
    }
}

/// Entries that are new or changed by a merge, oldest first.
fn new_log_entries(old: Option<&[LogEntry]>, new: &[LogEntry]) -> Vec<LogEntry> {
    let old: HashMap<i64, &LogEntry> = old.unwrap_or(&[]).iter().map(|entry| (entry.id, entry)).collect();
    new.iter().filter(|entry| old.get(&entry.id) != Some(entry)).rev().cloned().collect()
}

impl Room {
    pub(super) fn new(
        id: String,
        instance: u64,
        shared: Arc<Shared>,
        weak_self: mpsc::WeakUnboundedSender<RoomMsg>,
    ) -> Self {
        Self {
            id,
            instance,
            shared,
            entry: None,
            connections: HashMap::new(),
            monitors: HashMap::new(),
            departing: HashMap::new(),
            active_at: now(),
            next_ref: 0,
            weak_self,
            exit: watch::channel(None).0,
        }
    }

    /// Loads the session and serves requests until the room stops.
    pub(super) async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<RoomMsg>) {
        match super::session::load(&self.shared.pool, &self.id).await {
            Ok(entry) => self.entry = entry,
            Err(error) => {
                tracing::error!("webcam table {} could not load its session: {error}", self.id);
                self.shared.deregister(&self.id, self.instance);
                return;
            }
        }
        let mut refresh = tokio::time::interval_at(tokio::time::Instant::now() + REFRESH_INTERVAL, REFRESH_INTERVAL);
        loop {
            let flow = tokio::select! {
                message = inbox.recv() => match message {
                    Some(message) => self.handle(message).await,
                    None => Ok(Flow::Stop(RoomExit::Normal)),
                },
                _ = refresh.tick() => self.refresh().await.map(|()| Flow::Continue),
            };
            match flow {
                Ok(Flow::Continue) => {}
                Ok(Flow::Stop(reason)) => {
                    self.shared.deregister(&self.id, self.instance);
                    let _ = self.exit.send(Some(reason));
                    return;
                }
                Err(error) => {
                    // A failed write crashes the room, as `:ok = Session.save(...)` did; its
                    // channels rejoin a fresh room restored from the last saved session.
                    tracing::error!("webcam table {} crashed: {error}", self.id);
                    self.shared.deregister(&self.id, self.instance);
                    return;
                }
            }
        }
    }

    async fn refresh(&self) -> Result<(), sqlx::Error> {
        match &self.entry {
            Some(entry) => super::session::save(&self.shared.pool, &self.id, entry).await,
            None => Ok(()),
        }
    }

    /// Saves before broadcasting, so everything clients see is recoverable. New and merged log
    /// entries follow the broadcasts as `log_entry` events.
    async fn commit(&mut self, entry: Entry, broadcasts: Vec<Broadcast>) -> Result<(), sqlx::Error> {
        super::session::save(&self.shared.pool, &self.id, &entry).await?;
        let log_entries = new_log_entries(self.entry.as_ref().map(|old| old.log.as_slice()), &entry.log);
        let topic = format!("webcam_table:{}", self.id);
        for broadcast in broadcasts {
            match broadcast {
                Broadcast::TableState => self.shared.pubsub.broadcast(&topic, "table_state", to_value(&entry.snapshot())),
                Broadcast::Event(event, payload) => self.shared.pubsub.broadcast(&topic, event, payload),
            }
        }
        for log_entry in log_entries {
            self.shared.pubsub.broadcast(&topic, "log_entry", to_value(&log_entry));
        }
        self.entry = Some(entry);
        self.active_at = now();
        Ok(())
    }

    fn connection(&self, player_id: i64) -> Option<&Conn> {
        self.connections.get(&player_id).map(|connection| &connection.conn)
    }

    async fn handle(&mut self, message: RoomMsg) -> Result<Flow, sqlx::Error> {
        let entry = match message {
            RoomMsg::Join(participant, conn, reply) => {
                let result = self.join(*participant, conn).await?;
                let _ = reply.send(result);
                return Ok(Flow::Continue);
            }
            RoomMsg::CloseIfIdle(idle_ms, reply) => {
                if self.connections.is_empty() && now() - self.active_at >= idle_ms {
                    super::session::delete(&self.shared.pool, &self.id).await?;
                    self.shared.deregister(&self.id, self.instance);
                    let _ = reply.send(true);
                    return Ok(Flow::Stop(RoomExit::Normal));
                }
                let _ = reply.send(false);
                return Ok(Flow::Continue);
            }
            RoomMsg::Seated(player_id, reply) => {
                let seated = self.entry.as_ref().is_some_and(|entry| entry.all_seats.contains_key(&player_id));
                let _ = reply.send(seated);
                return Ok(Flow::Continue);
            }
            RoomMsg::Current(player_id, conn_id, reply) => {
                let _ = reply.send(self.connection(player_id).is_some_and(|conn| conn.id == conn_id));
                return Ok(Flow::Continue);
            }
            RoomMsg::Down(monitor) => {
                self.down(monitor);
                return Ok(Flow::Continue);
            }
            RoomMsg::Debug(reply) => {
                let _ = reply.send(RoomDebug {
                    connections: self.connections.keys().copied().collect(),
                    departing: self.departing.clone(),
                });
                return Ok(Flow::Continue);
            }
            other => match self.entry.clone() {
                Some(entry) => (other, entry),
                // Only joins name a new room's owner; requests to a room nobody joined are
                // dropped, so callers see the room as gone.
                None => return Ok(Flow::Continue),
            },
        };
        self.handle_entry(entry.0, entry.1).await
    }

    async fn handle_entry(&mut self, message: RoomMsg, mut entry: Entry) -> Result<Flow, sqlx::Error> {
        match message {
            RoomMsg::Snapshot(reply) => {
                let _ = reply.send(entry.snapshot());
            }
            RoomMsg::Log(reply) => {
                let _ = reply.send(entry.log);
            }
            RoomMsg::Roll(actor, kind, result, reply) => {
                let mut roll = json!({
                    "kind": match kind { RollKind::Dice(_) => "dice", RollKind::Coin => "coin" },
                    "result": result,
                    "id": uuid::Uuid::new_v4().to_string(),
                    "actor": actor.peer_id,
                    "player_name": actor.player_name,
                    "at": now(),
                });
                if let (RollKind::Dice(sides), Some(object)) = (kind, roll.as_object_mut()) {
                    object.insert("sides".into(), json!(sides));
                }
                let entry = entry.log(vec![log::roll(kind, &result, &actor.peer_id, &actor.player_name)]);
                self.commit(entry, vec![Broadcast::Event("roll", roll)]).await?;
                let _ = reply.send(());
            }
            RoomMsg::Close(reply) => {
                super::session::delete(&self.shared.pool, &self.id).await?;
                self.shared.deregister(&self.id, self.instance);
                let _ = reply.send(());
                return Ok(Flow::Stop(RoomExit::Closed));
            }
            RoomMsg::Rematch(reply) => {
                self.rematch(entry).await?;
                let _ = reply.send(());
            }
            RoomMsg::Monarch(holder, actor, reply) => {
                if entry.monarch.as_ref() != Some(&holder) {
                    entry.monarch = Some(holder.clone());
                    entry.monarch_revision += 1;
                    let event = json!({ "holder": holder, "revision": entry.monarch_revision });
                    let entry = entry.log(vec![log::monarch(&holder, &actor)]);
                    self.commit(entry, vec![Broadcast::Event("monarch", event)]).await?;
                }
                let _ = reply.send(());
            }
            RoomMsg::Cards(change, actor, reply) => {
                let seats: Vec<Seat> = entry.all_seats.values().cloned().collect();
                match cards::update(&entry.cards, &change, &seats) {
                    Some(cards) => {
                        let kind = match change {
                            Change::Identified(_) => "card_identified",
                            Change::Removed(_) => "card_removed",
                            Change::Cleared(_) => "cards_cleared",
                        };
                        let event = json!({ "entries": cards, "type": kind, "by": actor });
                        entry.cards = cards;
                        self.commit(entry, vec![Broadcast::Event("identified_cards", event)]).await?;
                        let _ = reply.send(Ok(()));
                    }
                    None => {
                        let _ = reply.send(Err("invalid cards".into()));
                    }
                }
            }
            RoomMsg::RememberSeat(participant, conn_id, reply) => {
                self.remember_seat(entry, *participant, conn_id).await?;
                let _ = reply.send(());
            }
            RoomMsg::Order(peers, reply) => {
                self.reorder(entry, peers, false).await?;
                let _ = reply.send(());
            }
            RoomMsg::Arrange(peers, reply) => {
                if entry.timer.started_at.is_none() {
                    entry.peer_ids = peers;
                    self.commit(entry, vec![Broadcast::TableState]).await?;
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("seat order is fixed after start".into()));
                }
            }
            RoomMsg::SetMode(mode, reply) => {
                if entry.timer.started_at.is_none() {
                    entry.mode = mode;
                    entry.team_life = BTreeMap::new();
                    self.commit(entry, vec![Broadcast::TableState]).await?;
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("game mode is fixed after start".into()));
                }
            }
            RoomMsg::TeamLife(actor, team_index, delta, reply) => {
                let outcome = self.team_life(entry, actor, team_index, delta).await?;
                let _ = reply.send(outcome);
            }
            RoomMsg::Eliminate(peer_id, eliminated, reply) => {
                if let Some(seat) = entry.all_seats.values().find(|seat| seat.peer_id == peer_id).cloned() {
                    let targets = if entry.mode == Mode::TwoHeadedGiant {
                        turns::team(&entry.ordered_seats(), seat.player_id).to_vec()
                    } else {
                        vec![seat]
                    };
                    let entry = self.eliminate_seats(entry, targets, eliminated);
                    let events = vec![eliminated_seats_event(&entry), Broadcast::TableState];
                    self.commit(entry, events).await?;
                }
                let _ = reply.send(());
            }
            RoomMsg::Timer(action, reply) => {
                entry.timer = entry.timer.update(action, now());
                let timer = entry.timer_state();
                self.commit(entry, vec![Broadcast::Event("timer_state", to_value(&timer))]).await?;
                let _ = reply.send(timer);
            }
            RoomMsg::BeginPlay(actor, reply) => {
                let outcome = if !entry.timer.awaiting_start() {
                    Ok(entry.timer_state())
                } else if let Actor::Player(player_id) = actor
                    && Some(turns::turn_id(&entry.ordered_seats(), player_id, entry.mode)) != entry.turns.active_player_id
                {
                    Err("only the first player can start the game".to_owned())
                } else {
                    entry.timer = entry.timer.update(Action::Resume, now());
                    let timer = entry.timer_state();
                    self.commit(entry, vec![Broadcast::Event("timer_state", to_value(&timer))]).await?;
                    Ok(timer)
                };
                let _ = reply.send(outcome);
            }
            RoomMsg::StartGame(randomize, reply) => {
                let outcome = self.start_game(entry, randomize).await?;
                let _ = reply.send(outcome);
            }
            RoomMsg::TurnSettings(enabled, reply) => {
                entry.auto_randomize = enabled;
                self.commit(entry, vec![Broadcast::TableState]).await?;
                let _ = reply.send(());
            }
            RoomMsg::PassTurn(revision, reply) => {
                if entry.turns.revision == revision && entry.turns.active_player_id.is_some() {
                    // Passing the first turn before pressing Start still begins the clock.
                    let awaiting = entry.timer.awaiting_start();
                    if awaiting {
                        entry.timer = entry.timer.update(Action::Resume, now());
                    }
                    entry.turns =
                        turns::pass(&entry.turns, &entry.ordered_seats(), entry.timer.elapsed(now()), entry.mode);
                    let events = if awaiting {
                        vec![Broadcast::Event("timer_state", to_value(&entry.timer_state())), Broadcast::TableState]
                    } else {
                        vec![Broadcast::TableState]
                    };
                    self.commit(entry, events).await?;
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("turn has changed or the game has not started".into()));
                }
            }
            RoomMsg::UnpassTurn(revision, reply) => {
                let undone = (entry.turns.revision == revision)
                    .then(|| turns::unpass(&entry.turns, &entry.ordered_seats(), entry.mode))
                    .flatten();
                if let Some(turns) = undone {
                    entry.turns = turns;
                    self.commit(entry, vec![Broadcast::TableState]).await?;
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("there is no pass to undo".into()));
                }
            }
            RoomMsg::AdjustTurn(player_id, delta, reply) => {
                if entry.all_seats.contains_key(&player_id) {
                    let player_id = turns::turn_id(&entry.ordered_seats(), player_id, entry.mode);
                    entry.turns = turns::adjust(&entry.turns, player_id, delta);
                    self.commit(entry, vec![Broadcast::TableState]).await?;
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("player is not in this game".into()));
                }
            }
            RoomMsg::Departed(player_id, token) => {
                // Stale tokens belong to an earlier disconnect the player already returned from.
                if self.departing.get(&player_id).is_some_and(|(_, current)| *current == token)
                    && let Some((name, _)) = self.departing.remove(&player_id)
                {
                    self.commit(entry.log(vec![log::left(&name)]), Vec::new()).await?;
                }
            }
            RoomMsg::Join(..)
            | RoomMsg::CloseIfIdle(..)
            | RoomMsg::Seated(..)
            | RoomMsg::Current(..)
            | RoomMsg::Down(_)
            | RoomMsg::Debug(_) => {}
        }
        Ok(Flow::Continue)
    }

    async fn join(&mut self, participant: Seat, conn: Conn) -> Result<Result<Admitted, String>, sqlx::Error> {
        let entry = self.entry.clone().unwrap_or_else(|| Entry::new(participant.player_id));
        let previous = entry.all_seats.get(&participant.player_id).cloned();
        let duplicate = entry
            .all_seats
            .iter()
            .any(|(id, seat)| *id != participant.player_id && seat.peer_id == participant.peer_id);
        if duplicate {
            return Ok(Err("peer id is already in use".into()));
        }
        if entry.timer.started_at.is_none() && previous.is_none() && entry.all_seats.len() >= MAX_SEATS {
            return Ok(Err("room is full".into()));
        }
        self.admit(entry, previous, participant, conn).await.map(Ok)
    }

    /// Seats the participant (a returning player gets their saved seat back; late arrivals
    /// spectate) with `conn` as its connection.
    async fn admit(
        &mut self,
        entry: Entry,
        previous: Option<Seat>,
        participant: Seat,
        conn: Conn,
    ) -> Result<Admitted, sqlx::Error> {
        let spectator = previous.is_none() && entry.timer.started_at.is_some();
        let mut participant = match &previous {
            Some(previous) => Seat { peer_id: participant.peer_id, ..previous.clone() },
            None => participant,
        };
        participant.spectator = spectator;
        // A reload replaces a live tab or returns within the grace period: not news.
        let returning = self.connections.contains_key(&participant.player_id)
            || self.departing.contains_key(&participant.player_id);
        self.departing.remove(&participant.player_id);
        self.replace_connection(conn, &participant);
        let entry = if spectator { entry } else { restore_seat(entry, previous.as_ref(), &participant) };
        let entry = if returning { entry } else { entry.log(vec![log::joined(&participant.player_name)]) };
        let snapshot = entry.snapshot();
        self.commit(entry, vec![Broadcast::TableState]).await?;
        Ok(Admitted { snapshot, participant, exit: self.exit.subscribe() })
    }

    /// One live connection per player: a reload replaces the older tab.
    fn replace_connection(&mut self, conn: Conn, participant: &Seat) {
        if let Some(old) = self.connections.remove(&participant.player_id) {
            self.monitors.remove(&old.monitor);
            let _ = old.conn.tx.send(ConnEvent::SeatReplaced);
        }
        self.next_ref += 1;
        let monitor = self.next_ref;
        let watched = conn.tx.clone();
        let room = self.weak_self.clone();
        tokio::spawn(async move {
            watched.closed().await;
            if let Some(room) = room.upgrade() {
                let _ = room.send(RoomMsg::Down(monitor));
            }
        });
        self.monitors.insert(monitor, (participant.player_id, participant.player_name.clone()));
        self.connections.insert(participant.player_id, Connection { conn, monitor });
    }

    fn down(&mut self, monitor: u64) {
        let Some((player_id, name)) = self.monitors.remove(&monitor) else {
            return;
        };
        self.next_ref += 1;
        let token = self.next_ref;
        let room = self.weak_self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(DEPARTURE_GRACE).await;
            if let Some(room) = room.upgrade() {
                let _ = room.send(RoomMsg::Departed(player_id, token));
            }
        });
        self.connections.remove(&player_id);
        self.departing.insert(player_id, (name, token));
        self.active_at = now();
    }

    /// A rematch keeps the room, owner, mode, randomize setting and the seats of players still
    /// here (in their last order, with their decks, cameras and reveals) and puts everything
    /// else back to a fresh lobby. Connected seats are sent their reset seat to adopt.
    async fn rematch(&mut self, entry: Entry) -> Result<(), sqlx::Error> {
        let present: Vec<i64> = self.connections.keys().chain(self.departing.keys()).copied().collect();
        let seats: BTreeMap<i64, Seat> = entry
            .all_seats
            .iter()
            .filter(|(id, _)| present.contains(id))
            .map(|(id, seat)| (*id, seat.clone().reset()))
            .collect();
        let kept: Vec<&str> = seats.values().map(|seat| seat.peer_id.as_str()).collect();
        let peer_ids = entry.peer_ids.iter().filter(|id| kept.contains(&id.as_str())).cloned().collect();
        let log = log::append(&[], log::rematch(), now());
        let entry = Entry {
            timer: Timer::new(),
            turns: Turns::default(),
            team_life: BTreeMap::new(),
            monarch: None,
            // A higher revision, so clients drop the old crown rather than ignore a stale event.
            monarch_revision: entry.monarch_revision + 1,
            cards: Vec::new(),
            eliminated_seats: BTreeMap::new(),
            all_seats: seats.clone(),
            peer_ids,
            log: log.clone(),
            ..entry
        };
        self.commit(entry, vec![Broadcast::TableState, Broadcast::Event("table_log", json!({ "entries": log }))])
            .await?;
        for (id, seat) in seats {
            if let Some(conn) = self.connection(id) {
                let _ = conn.tx.send(ConnEvent::SeatReset(Box::new(seat)));
            }
        }
        Ok(())
    }

    /// Only the seat's current connection may update it, so a replaced tab's late updates
    /// cannot overwrite the reloaded seat.
    async fn remember_seat(&mut self, mut entry: Entry, mut participant: Seat, conn_id: u64) -> Result<(), sqlx::Error> {
        if self.connection(participant.player_id).is_none_or(|conn| conn.id != conn_id) {
            return Ok(());
        }
        let Some(previous) = entry.all_seats.get(&participant.player_id).cloned() else {
            return Ok(());
        };
        participant.eliminated = previous.eliminated;
        let mut eliminated_seats = entry.eliminated_seats.clone();
        put_eliminated(&mut eliminated_seats, &participant);
        let changed = eliminated_seats != entry.eliminated_seats;
        entry.eliminated_seats = eliminated_seats;
        entry.all_seats.insert(participant.player_id, participant.clone());
        let entry = entry.reconcile_turn();
        let seats: Vec<Seat> = entry.all_seats.values().cloned().collect();
        let entry = entry.log(log::seat_changes(&previous, &participant, &seats));
        let events = if changed {
            vec![eliminated_seats_event(&entry), Broadcast::TableState]
        } else {
            vec![Broadcast::TableState]
        };
        self.commit(entry, events).await
    }

    async fn team_life(&mut self, mut entry: Entry, actor: Actor, team_index: i64, delta: i64) -> Result<Outcome, sqlx::Error> {
        let ordered = entry.ordered_seats();
        let team: Vec<Seat> = usize::try_from(team_index)
            .ok()
            .and_then(|index| ordered.chunks(2).nth(index))
            .map(<[Seat]>::to_vec)
            .unwrap_or_default();
        let allowed = match actor {
            Actor::Owner => true,
            Actor::Player(player_id) => team.iter().any(|seat| seat.player_id == player_id),
        };
        let Some(current) = entry.team_life.get(&team_index).copied().filter(|_| entry.mode == Mode::TwoHeadedGiant && allowed)
        else {
            return Ok(Err("only teammates or the owner can change a started team's life".into()));
        };
        let life = (current + delta).clamp(-999, 999);
        entry.team_life.insert(team_index, life);
        // Zero shared life knocks the whole team out; restoring stays manual.
        if life <= 0 && team.iter().any(|seat| !seat.eliminated) {
            let entry = self.eliminate_seats(entry, team, true);
            let events = vec![eliminated_seats_event(&entry), Broadcast::TableState];
            self.commit(entry, events).await?;
        } else {
            self.commit(entry, vec![Broadcast::TableState]).await?;
        }
        Ok(Ok(()))
    }

    async fn start_game(&mut self, mut entry: Entry, randomize: Option<bool>) -> Result<Outcome, sqlx::Error> {
        let randomize = randomize.unwrap_or(entry.auto_randomize);
        let peers: Vec<String> = entry
            .ordered_seats()
            .into_iter()
            .filter(|seat| !turns::TurnSeat::departed(seat))
            .map(|seat| seat.peer_id)
            .collect();
        if entry.timer.started_at.is_some() {
            return Ok(Ok(()));
        }
        if let Some(reason) = roster_error(entry.mode, peers.len()) {
            return Ok(Err(reason.into()));
        }
        if entry.mode == Mode::TwoHeadedGiant {
            let teams = i64::try_from(peers.len() / 2).unwrap_or(0);
            entry.team_life = (0..teams).map(|team| (team, TEAM_STARTING_LIFE)).collect();
        } else {
            entry.team_life = BTreeMap::new();
        }
        let peers = shuffle(peers, entry.mode, randomize);
        self.reorder(entry, peers, randomize).await?;
        Ok(Ok(()))
    }

    /// Sets the seat order and starts the clock (idempotently). Shared by the initial start
    /// and mid-game Commander reorders.
    async fn reorder(&mut self, entry: Entry, peers: Vec<String>, shuffled: bool) -> Result<(), sqlx::Error> {
        // Keep departed eliminated seats in their recorded positions when live seats reshuffle.
        let departed: Vec<String> = entry
            .eliminated_seats
            .values()
            .map(|seat| seat.peer_id.clone())
            .filter(|id| !peers.contains(id))
            .collect();
        let mut rest = peers.into_iter();
        let mut ordered: Vec<String> = entry
            .peer_ids
            .iter()
            .filter_map(|id| if departed.contains(id) { Some(id.clone()) } else { rest.next() })
            .collect();
        ordered.extend(rest);
        ordered.extend(departed.into_iter().filter(|id| !entry.peer_ids.contains(id)));

        // Cards identified in the lobby do not carry into the game.
        let started = entry.timer.started_at.is_some();
        let entry = Entry {
            cards: if started { entry.cards.clone() } else { Vec::new() },
            timer: entry.timer.update(Action::Start, now()),
            peer_ids: ordered.clone(),
            ..entry
        };
        let entry = entry.reconcile_turn().log(vec![log::seat_order(shuffled, started)]);
        let timer = entry.timer_state();
        self.commit(
            entry,
            vec![
                Broadcast::Event("seat_order", json!({ "peer_ids": ordered, "shuffled": shuffled })),
                Broadcast::Event("timer_state", to_value(&timer)),
                Broadcast::TableState,
            ],
        )
        .await
    }

    /// Marks seats in or out, tells their live connections, and moves the turn on if the
    /// active seat just left. Callers commit the returned entry.
    fn eliminate_seats(&self, mut entry: Entry, seats: Vec<Seat>, eliminated: bool) -> Entry {
        for seat in seats {
            if seat.eliminated != eliminated {
                entry = entry.log(vec![log::elimination(&seat, eliminated)]);
            }
            let seat = Seat { eliminated, ..seat };
            if let Some(conn) = self.connection(seat.player_id) {
                let _ = conn.tx.send(ConnEvent::SeatEliminated(eliminated));
            }
            put_eliminated(&mut entry.eliminated_seats, &seat);
            entry.all_seats.insert(seat.player_id, seat);
        }
        entry.reconcile_turn()
    }
}

/// A returning player keeps their seat, position, crown and cards under the new peer id.
fn restore_seat(mut entry: Entry, previous: Option<&Seat>, participant: &Seat) -> Entry {
    let replace = |id: &str| -> String {
        if previous.is_some_and(|previous| previous.peer_id == id) { participant.peer_id.clone() } else { id.to_owned() }
    };
    entry.peer_ids = entry.peer_ids.iter().map(|id| replace(id)).collect();
    entry.monarch = entry.monarch.map(|holder| Holder { peer_id: replace(&holder.peer_id), ..holder });
    for card in &mut entry.cards {
        if let Some(object) = card.as_object_mut() {
            let owner = object.get("ownerPeerId").and_then(Value::as_str).map(replace);
            object.insert("ownerPeerId".into(), owner.map_or(Value::Null, Value::String));
        }
    }
    put_eliminated(&mut entry.eliminated_seats, participant);
    entry.all_seats.insert(participant.player_id, participant.clone());
    entry
}
