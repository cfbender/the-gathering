//! The webcam table channel: one socket's seat at one table, and its per-connection rate limits.
//!
//! A channel task starts with the socket's `join` event and handles the socket's events in order
//! until it stops. While joined, the socket is in the table topic's Socket.IO room, which carries
//! the room's broadcasts and presence rosters.
//!
//! Every event spends a token from the connection's bucket before it is handled, so floods
//! are refused before they validate, broadcast or write SQLite. Signals have their own,
//! larger bucket because connecting to a table sends dozens of candidates at once. Joins are
//! limited per user through the shared rate limiter, so rejoining cannot reset the budget.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rand::Rng;
use serde_json::{Map, Value, json};
use the_gathering_sfu::SfuEvent;
use tokio::sync::{broadcast, mpsc, watch};

use super::presence::Left;
use super::rooms::{self, LOBBY_TOPIC};
use super::{ClientEvent, ClientMsg, Reply, SocketCtx};
use crate::accounts::User;
use crate::rate_limit::{Decision, TokenBucket};
use crate::regex::{Regex, compile};
use crate::webcam::cards::Change;
use crate::webcam::log::{RollKind, RollResult};
use crate::webcam::room::{Actor, Conn, ConnEvent, RoomExit};
use crate::webcam::seat::{CombatEffect, CustomCounter, Seat};
use crate::webcam::timer::Action;
use crate::webcam::{self, Mode, RoomGone};

const LIFE_RANGE: std::ops::RangeInclusive<i64> = -999..=999;
/// SDP offers with many candidates run 10–20 KB; anything far larger is abuse.
const MAX_SDP_BYTES: usize = 65_536;
/// Direct seat-to-seat messages carry card crops (JPEG data URLs) for the scanner.
const MAX_PEER_MESSAGE_BYTES: usize = 262_144;
/// No webcam publishes more rows than 8K; anything above is a bogus status.
const MAX_CAMERA_HEIGHT: i64 = 4_320;
const SIGNAL_EVENTS: [&str; 5] = [
    "sfu_offer",
    "sfu_answer",
    "sfu_candidate",
    "sfu_layer",
    "peer_message",
];
const OWNER_EVENTS: [&str; 9] = [
    "start_game",
    "seat_order",
    "arrange_seats",
    "set_mode",
    "turn_settings",
    "adjust_turn",
    "timer",
    "end_game",
    "rematch",
];

static UUID: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"));
static ROOM_ID: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
});
static PLAYER_KEY: LazyLock<Regex> = LazyLock::new(|| compile(r"^[1-9][0-9]{0,15}$"));

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

/// Clients generate peer ids with `crypto.randomUUID()`; only that canonical form passes.
pub fn uuid(value: &str) -> bool {
    UUID.is_match(value)
}

/// Room ids are UUIDs in either case.
fn valid_room_id(room_id: &str) -> bool {
    ROOM_ID.is_match(room_id)
}

/// How a channel stops.
#[derive(Debug)]
enum Stop {
    /// Normally, and the client does not rejoin.
    Close(&'static str),
    /// Abnormally: the client gets `rejoin`.
    Error(String),
    /// The socket went away.
    Silent,
}

impl From<RoomGone> for Stop {
    fn from(_: RoomGone) -> Self {
        Self::Error("room_down".into())
    }
}

impl From<sqlx::Error> for Stop {
    fn from(error: sqlx::Error) -> Self {
        Self::Error(format!("database: {error}"))
    }
}

type Handled = Result<Option<Reply>, Stop>;

#[allow(clippy::unnecessary_wraps)] // Shaped like every handler's result.
fn reply(reply: impl Into<Reply>) -> Handled {
    Ok(Some(reply.into()))
}

#[allow(clippy::unnecessary_wraps)] // Shaped like every handler's result.
fn error(reason: &str) -> Handled {
    Ok(Some(Reply::error(reason)))
}

fn object(payload: &Value) -> Option<&Map<String, Value>> {
    payload.as_object()
}

/// The payload as a map with exactly `size` keys.
fn exactly(payload: &Value, size: usize) -> Option<&Map<String, Value>> {
    object(payload).filter(|map| map.len() == size)
}

fn empty(payload: &Value) -> bool {
    exactly(payload, 0).is_some()
}

fn int(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

fn short_string(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|text| (1..=max).contains(&text.len()))
}

/// Status changes a seat publishes about itself. `camera_height` distinguishes "unchanged"
/// from "no camera" (`null`).
#[derive(Debug, Default)]
#[allow(clippy::option_option)]
struct StatusChanges {
    life: Option<i64>,
    camera_off: Option<bool>,
    camera_height: Option<Option<i64>>,
    shares_corrections: Option<bool>,
    poison: Option<i64>,
    rad: Option<i64>,
    commander_casts: Option<BTreeMap<String, i64>>,
    commander_damage: Option<BTreeMap<String, BTreeMap<String, i64>>>,
    eliminated: Option<bool>,
    custom_counters: Option<Vec<CustomCounter>>,
    combat_effects: Option<Vec<CombatEffect>>,
}

impl StatusChanges {
    /// Validates every key; any unknown key or invalid value rejects the whole update.
    fn parse(payload: &Map<String, Value>) -> Option<Self> {
        let mut changes = Self::default();
        for (key, value) in payload {
            match key.as_str() {
                "life" => {
                    changes.life = Some(int(Some(value)).filter(|life| LIFE_RANGE.contains(life))?);
                }
                "camera_off" => changes.camera_off = Some(value.as_bool()?),
                "camera_height" => {
                    changes.camera_height = Some(match value {
                        Value::Null => None,
                        other => Some(
                            int(Some(other))
                                .filter(|rows| (1..=MAX_CAMERA_HEIGHT).contains(rows))?,
                        ),
                    });
                }
                "shares_corrections" => changes.shares_corrections = Some(value.as_bool()?),
                "poison" => {
                    changes.poison =
                        Some(int(Some(value)).filter(|count| (0..=999).contains(count))?);
                }
                "rad" => {
                    changes.rad = Some(int(Some(value)).filter(|count| (0..=999).contains(count))?);
                }
                "commander_casts" => changes.commander_casts = Some(counts(value)?),
                "commander_damage" => changes.commander_damage = Some(damage(value)?),
                "eliminated" => changes.eliminated = Some(value.as_bool()?),
                "custom_counters" => changes.custom_counters = Some(custom_counters(value)?),
                "combat_effects" => changes.combat_effects = Some(combat_effects(value)?),
                _ => return None,
            }
        }
        Some(changes)
    }

    fn apply(self, seat: &mut Seat) {
        if let Some(life) = self.life {
            seat.life = life;
        }
        if let Some(camera_off) = self.camera_off {
            seat.camera_off = camera_off;
        }
        if let Some(height) = self.camera_height {
            seat.camera_height = height;
        }
        if let Some(shares) = self.shares_corrections {
            seat.shares_corrections = shares;
        }
        if let Some(poison) = self.poison {
            seat.poison = poison;
        }
        if let Some(rad) = self.rad {
            seat.rad = rad;
        }
        if let Some(casts) = self.commander_casts {
            seat.commander_casts = casts;
        }
        if let Some(damage) = self.commander_damage {
            seat.commander_damage = damage;
        }
        if let Some(eliminated) = self.eliminated {
            seat.eliminated = eliminated;
        }
        if let Some(counters) = self.custom_counters {
            seat.custom_counters = counters;
        }
        if let Some(effects) = self.combat_effects {
            seat.combat_effects = effects;
        }
    }
}

fn counts(value: &Value) -> Option<BTreeMap<String, i64>> {
    let map = value.as_object().filter(|map| map.len() <= 100)?;
    map.iter()
        .map(|(name, count)| {
            let count = int(Some(count)).filter(|count| (0..=999).contains(count))?;
            (1..=300)
                .contains(&name.len())
                .then(|| (name.clone(), count))
        })
        .collect()
}

fn damage(value: &Value) -> Option<BTreeMap<String, BTreeMap<String, i64>>> {
    let map = value.as_object().filter(|map| map.len() <= 100)?;
    map.iter()
        .map(|(player_id, value)| {
            PLAYER_KEY
                .is_match(player_id)
                .then(|| counts(value).map(|c| (player_id.clone(), c)))?
        })
        .collect()
}

/// Free-form counters a seat shares ("Lands: 7"). The client keeps private ones to itself.
fn custom_counters(value: &Value) -> Option<Vec<CustomCounter>> {
    let list = value.as_array().filter(|list| list.len() <= 20)?;
    list.iter()
        .map(|counter| {
            let map = exactly(counter, 3)?;
            let (id, label, value) = (map.get("id")?, map.get("label")?, int(map.get("value"))?);
            (short_string(id, 40) && short_string(label, 40) && (0..=100).contains(&value)).then(
                || CustomCounter {
                    id: id.as_str().unwrap_or_default().to_owned(),
                    label: label.as_str().unwrap_or_default().to_owned(),
                    value,
                },
            )
        })
        .collect()
}

fn string_list(value: Option<&Value>, max_items: usize) -> Option<Vec<String>> {
    let list = value?.as_array().filter(|list| list.len() <= max_items)?;
    list.iter()
        .map(|item| short_string(item, 40).then(|| item.as_str().unwrap_or_default().to_owned()))
        .collect()
}

/// Anthems and combat buffs a seat shares; every client derives the same totals from them.
fn combat_effects(value: &Value) -> Option<Vec<CombatEffect>> {
    let list = value.as_array().filter(|list| list.len() <= 30)?;
    let buff = |value: Option<&Value>| int(value).filter(|amount| (-99..=99).contains(amount));
    list.iter()
        .map(|effect| {
            let map = exactly(effect, 6)?;
            let id = map
                .get("id")
                .filter(|id| short_string(id, 40))?
                .as_str()?
                .to_owned();
            let name = map
                .get("name")?
                .as_str()
                .filter(|name| name.len() <= 80)?
                .to_owned();
            Some(CombatEffect {
                id,
                name,
                power: buff(map.get("power"))?,
                toughness: buff(map.get("toughness"))?,
                conditions: string_list(map.get("conditions"), 8)?,
                keywords: string_list(map.get("keywords"), 10)?,
            })
        })
        .collect()
}

fn to_value(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

struct Channel {
    socket: SocketCtx,
    topic: String,
    room_id: String,
    participant: Seat,
    owner: bool,
    conn_id: u64,
    events: TokenBucket,
    signals: TokenBucket,
    joined_at: Instant,
}

/// What a successful join hands the channel loop.
struct Inboxes {
    conn: mpsc::UnboundedReceiver<ConnEvent>,
    leaves: broadcast::Receiver<Left>,
    exit: watch::Receiver<Option<RoomExit>>,
}

/// The `join` event's payload.
#[derive(serde::Deserialize)]
struct JoinParams {
    room_id: String,
    #[serde(flatten)]
    seat: Value,
}

/// Runs one joined channel: the join, `after_join`, then events until it stops.
pub async fn run(
    socket: SocketCtx,
    join: ClientEvent,
    mut client: mpsc::UnboundedReceiver<ClientMsg>,
) {
    let (mut channel, mut inboxes, response) = match Channel::join(&socket, &join.payload).await {
        Ok(joined) => joined,
        Err(reply) => {
            reply.send(join.ack);
            return;
        }
    };
    Reply::Ok(response).send(join.ack);

    let (sfu_tx, mut sfu_rx) = mpsc::unbounded_channel();
    let mut stop = channel.after_join(sfu_tx).await.err();
    while stop.is_none() {
        // Biased: room, presence and SFU events that arrived before a client event are handled
        // first, in mailbox order.
        let outcome = tokio::select! {
            biased;
            Some(event) = inboxes.conn.recv() => channel.handle_conn_event(event),
            left = inboxes.leaves.recv() => match left {
                Ok(left) if left.topic != channel.topic => Ok(()),
                Ok(left) => channel.handle_left(Some(&left.key)).await,
                // Missed leaves: check the reveal target directly.
                Err(broadcast::error::RecvError::Lagged(_)) => channel.handle_left(None).await,
                Err(broadcast::error::RecvError::Closed) => Err(Stop::Silent),
            },
            Some(event) = sfu_rx.recv() => channel.handle_sfu(event),
            changed = inboxes.exit.changed() => Err(match changed.ok().and(*inboxes.exit.borrow()) {
                // The owner ended the table. The client leaves instead of rejoining (which
                // would open a fresh room under the id).
                Some(RoomExit::Closed) => {
                    channel.push("table_closed", &json!({}));
                    Stop::Close("the table ended")
                }
                // The room crashed: the client rejoins a fresh room restored from the saved
                // session.
                _ => Stop::Error("room_down".into()),
            }),
            message = client.recv() => match message {
                None => Err(Stop::Silent),
                Some(ClientMsg::Shutdown) => Err(Stop::Close("replaced by a new join on the same socket")),
                Some(ClientMsg::Event(event)) if event.event == "leave" => {
                    Reply::ok().send(event.ack);
                    Err(Stop::Close("left"))
                }
                Some(ClientMsg::Event(event)) => match channel.handle_in(&event.event, &event.payload).await {
                    Ok(Some(answer)) => {
                        answer.send(event.ack);
                        Ok(())
                    }
                    Ok(None) => Ok(()),
                    Err(stop) => Err(stop),
                },
            },
        };
        stop = outcome.err();
    }
    channel.terminate(stop.unwrap_or(Stop::Silent)).await;
}

impl Channel {
    fn push(&self, event: &str, payload: &impl serde::Serialize) {
        let _ = self.socket.socket.emit(event, payload);
    }

    fn tables(&self) -> &webcam::WebcamTables {
        &self.socket.state.webcam_tables
    }

    async fn join(socket: &SocketCtx, payload: &Value) -> Result<(Self, Inboxes, Value), Reply> {
        let state = &socket.state;
        let user = &socket.user;
        let limits = &state.config.rate_limits;
        if let Decision::Deny(_) = state.rate_limiter.hit(
            &format!("webcam_table_joins:{}", user.id),
            limits.webcam_table_joins,
        ) {
            return Err(Reply::error("rate limited"));
        }
        let params = serde_json::from_value::<JoinParams>(payload.clone())
            .ok()
            .filter(|params| valid_room_id(&params.room_id));
        let Some(JoinParams { room_id, seat }) = params else {
            return Err(Reply::error("invalid room"));
        };
        let participant = participant(state, &seat, user)
            .await
            .map_err(Reply::error)?;

        let conn_id = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
        let (conn_tx, conn_rx) = mpsc::unbounded_channel();
        let admitted = match state
            .webcam_tables
            .join(
                &room_id,
                participant,
                Conn {
                    id: conn_id,
                    tx: conn_tx,
                },
            )
            .await
        {
            Ok(Ok(admitted)) => admitted,
            Ok(Err(reason)) => return Err(Reply::Error(reason)),
            Err(RoomGone) => return Err(Reply::error("join crashed")),
        };
        let participant = admitted.participant;
        // Admins run every table they sit at, alongside the player who opened it. Spectators
        // never hold table controls.
        let owner = !participant.spectator
            && (user.is_admin() || admitted.snapshot.owner_id == participant.player_id);

        // Subscribed before the reply, so no broadcast after the snapshot is missed.
        let topic = webcam::topic(&room_id);
        let leaves = state.presence.leaves();
        socket.socket.join(topic.clone());
        let response =
            json!({ "participant": participant, "table_state": admitted.snapshot, "owner": owner });
        let channel = Self {
            socket: socket.clone(),
            topic,
            room_id,
            participant,
            owner,
            conn_id,
            events: TokenBucket::new(limits.webcam_table_events),
            signals: TokenBucket::new(limits.webcam_table_signals),
            joined_at: Instant::now(),
        };
        Ok((
            channel,
            Inboxes {
                conn: conn_rx,
                leaves,
                exit: admitted.exit,
            },
            response,
        ))
    }

    async fn after_join(&mut self, sfu: mpsc::UnboundedSender<SfuEvent>) -> Result<(), Stop> {
        let state = self.socket.state.clone();
        tracing::info!(
            "Webcam table seat {} (socket {}) joined table {}",
            self.participant.peer_id,
            self.socket.socket.id,
            self.room_id
        );
        // Sends every seat, this one included, the roster.
        state.presence.track(
            &self.topic,
            self.conn_id,
            &self.participant.peer_id,
            to_value(&self.participant),
        );
        if !self.participant.spectator {
            rooms::track_seat(&state, self.conn_id, &self.room_id, &self.participant);
        }
        let snapshot = self.tables().snapshot(&self.room_id).await?;
        self.push("table_state", &snapshot);
        self.push("monarch_state", &snapshot.monarch);
        // Sent once per join rather than in every table_state; new entries follow as log_entry.
        let log = self.tables().log(&self.room_id).await?;
        self.push("table_log", &json!({ "entries": log }));
        // The seat's media connection lives in the SFU; the browser offers once the join
        // reply arrives.
        state
            .sfu
            .join(
                &self.room_id,
                &self.participant.peer_id,
                self.participant.spectator,
                sfu,
            )
            .await
            .map_err(|error| Stop::Error(format!("sfu_unavailable: {error}")))
    }

    async fn terminate(self, stop: Stop) {
        let state = &self.socket.state;
        let lifetime = self.joined_at.elapsed().as_millis();
        let reason = match &stop {
            Stop::Close(reason) => reason,
            Stop::Error(reason) => reason.as_str(),
            Stop::Silent => "socket closed",
        };
        tracing::info!(
            "Webcam table seat {} (socket {}) left after {lifetime}ms: {reason}",
            self.participant.peer_id,
            self.socket.socket.id
        );
        self.socket.socket.leave(self.topic.clone());
        state.presence.untrack(&self.topic, self.conn_id);
        state.presence.untrack(LOBBY_TOPIC, self.conn_id);
        state
            .sfu
            .leave(&self.room_id, &self.participant.peer_id)
            .await;
        if let Stop::Error(reason) = stop {
            self.push("rejoin", &json!({ "reason": reason }));
        }
    }

    fn update_presence(&self) {
        self.socket.state.presence.update(
            &self.topic,
            self.conn_id,
            &self.participant.peer_id,
            to_value(&self.participant),
        );
    }

    fn present(&self, peer_id: &str) -> bool {
        self.socket.state.presence.has_key(&self.topic, peer_id)
    }

    async fn remember_seat(&self, eliminated: Option<bool>) -> Result<bool, RoomGone> {
        self.tables()
            .remember_seat(
                &self.room_id,
                self.participant.clone(),
                self.conn_id,
                eliminated,
            )
            .await
    }

    fn handle_sfu(&self, event: SfuEvent) -> Result<(), Stop> {
        match event {
            SfuEvent::Offer(payload) => self.push("sfu_offer", &payload),
            SfuEvent::Candidate(payload) => self.push("sfu_candidate", &payload),
            SfuEvent::PeerMessage(payload) => self.push("peer_message", &payload),
            // The media connection failed or crashed: the client rejoins under a new peer id and
            // negotiates a fresh connection.
            SfuEvent::Down(reason) => return Err(Stop::Error(format!("sfu_down: {reason}"))),
        }
        Ok(())
    }

    fn handle_conn_event(&mut self, event: ConnEvent) -> Result<(), Stop> {
        match event {
            ConnEvent::SeatReplaced => {
                self.push("seat_replaced", &json!({}));
                return Err(Stop::Close("the seat was taken by another connection"));
            }
            ConnEvent::SeatEliminated(eliminated) => {
                self.participant.eliminated = eliminated;
                self.update_presence();
            }
            // A rematch reset this seat. Adopting the room's copy keeps later status updates
            // from restoring the old game's life and counters.
            ConnEvent::SeatReset(seat) => {
                self.participant = *seat;
                self.update_presence();
                self.push("seat_reset", &json!({ "participant": self.participant }));
            }
        }
        Ok(())
    }

    /// A reveal ends when its target leaves (`left` is `None` when leaves were missed).
    async fn handle_left(&mut self, left: Option<&str>) -> Result<(), Stop> {
        if let Some(target) = self.participant.reveal_to.clone()
            && left.is_none_or(|key| key == target)
            && !self.present(&target)
        {
            self.put_reveal(None).await?;
        }
        Ok(())
    }

    /// Presence tells every seat who may see the board; the SFU enforces it on the media.
    async fn put_reveal(&mut self, target: Option<String>) -> Result<bool, RoomGone> {
        self.participant.reveal_to = target;
        if !self.remember_seat(None).await? {
            return Ok(false);
        }
        self.update_presence();
        let _ = self
            .socket
            .state
            .sfu
            .reveal(
                &self.room_id,
                &self.participant.peer_id,
                self.participant.reveal_to.as_deref(),
            )
            .await;
        Ok(true)
    }

    async fn handle_in(&mut self, event: &str, payload: &Value) -> Handled {
        let signal = SIGNAL_EVENTS.contains(&event);
        let bucket = if signal {
            &mut self.signals
        } else {
            &mut self.events
        };
        if !bucket.take() {
            return error("rate limited");
        }
        if self.participant.spectator && !signal && event != "timer_sync" {
            return error("spectators cannot change the game");
        }
        if !self.owner && OWNER_EVENTS.contains(&event) {
            return error("only the room owner can change table controls");
        }
        match event {
            "cards" => self.cards(payload).await,
            "sfu_offer" => self.sfu_offer(payload).await,
            "sfu_answer" => self.sfu_answer(payload).await,
            "sfu_candidate" => self.sfu_candidate(payload).await,
            "sfu_layer" => self.sfu_layer(payload).await,
            "peer_message" => self.peer_message(payload).await,
            "choose_deck" => self.choose_deck(payload).await,
            "reveal" => self.reveal(payload).await,
            "update_status" => self.update_status(payload).await,
            "take_monarch" => self.take_monarch(payload).await,
            "set_eliminated" => self.set_eliminated(payload).await,
            "seat_order" | "arrange_seats" => self.seat_order(event, payload).await,
            "set_mode" => self.set_mode(payload).await,
            "adjust_team_life" => self.adjust_team_life(payload).await,
            "end_game" if empty(payload) => {
                // Closes the table for every seat; each connection then gets table_closed.
                self.tables().close(&self.room_id).await?;
                reply(Reply::ok())
            }
            "end_game" => error("invalid end game"),
            "rematch" if empty(payload) => {
                self.tables().rematch(&self.room_id).await?;
                reply(Reply::ok())
            }
            "rematch" => error("invalid rematch"),
            "start_game" => self.start_game(payload).await,
            "turn_settings" => {
                match exactly(payload, 1).and_then(|map| map.get("auto_randomize")?.as_bool()) {
                    Some(enabled) => {
                        self.tables().turn_settings(&self.room_id, enabled).await?;
                        reply(Reply::ok())
                    }
                    None => error("invalid turn settings"),
                }
            }
            "pass_turn" => match revision(payload) {
                Some(revision) => reply(self.tables().pass_turn(&self.room_id, revision).await?),
                None => error("invalid pass turn"),
            },
            "unpass_turn" => match revision(payload) {
                Some(revision) => reply(self.tables().unpass_turn(&self.room_id, revision).await?),
                None => error("invalid un-pass turn"),
            },
            "adjust_turn" => self.adjust_turn(payload).await,
            "timer" => match exactly(payload, 1)
                .and_then(|map| map.get("action")?.as_str())
                .and_then(Action::parse_client)
            {
                Some(action) => {
                    let timer = self.tables().timer(&self.room_id, action).await?;
                    reply(Reply::Ok(to_value(&timer)))
                }
                None => error("invalid timer action"),
            },
            "begin_play" if empty(payload) => {
                let actor = self.actor();
                match self.tables().begin_play(&self.room_id, actor).await? {
                    Ok(timer) => reply(Reply::Ok(to_value(&timer))),
                    Err(reason) => error(&reason),
                }
            }
            "begin_play" => error("invalid start"),
            "timer_sync" if empty(payload) => {
                let snapshot = self.tables().snapshot(&self.room_id).await?;
                reply(Reply::Ok(to_value(&snapshot.timer)))
            }
            "timer_sync" => error("invalid timer sync"),
            "roll" => self.roll(payload).await,
            _ => error("unknown event"),
        }
    }

    fn actor(&self) -> Actor {
        if self.owner {
            Actor::Owner
        } else {
            Actor::Player(self.participant.player_id)
        }
    }

    async fn update_cards(&self, payload: &Value) -> Handled {
        let Some(change) = Change::parse(payload) else {
            return error("invalid cards");
        };
        reply(
            self.tables()
                .cards(&self.room_id, change, self.participant.holder())
                .await?,
        )
    }

    async fn cards(&self, payload: &Value) -> Handled {
        let kind = payload.get("type").and_then(Value::as_str);
        if kind == Some("cards_cleared")
            && let Some(owner) = payload.get("ownerPeerId")
        {
            if owner.as_str() == Some(self.participant.peer_id.as_str()) {
                return self.update_cards(payload).await;
            }
            return error("only the board owner can clear its cards");
        }
        // Attribution is always the sender's seat; a client-supplied name is ignored.
        if kind == Some("card_identified")
            && let Some(Value::Object(entry)) = payload.get("entry")
        {
            let mut entry = entry.clone();
            entry.insert("byPlayerName".into(), json!(self.participant.player_name));
            let mut payload = payload.clone();
            if let Some(object) = payload.as_object_mut() {
                object.insert("entry".into(), Value::Object(entry));
            }
            return self.update_cards(&payload).await;
        }
        // Any seated player may remove any entry to correct a misidentification.
        self.update_cards(payload).await
    }

    fn sdp(payload: &Value) -> Option<&str> {
        exactly(payload, 1)?
            .get("sdp")?
            .as_str()
            .filter(|sdp| sdp.len() <= MAX_SDP_BYTES)
    }

    /// The browser's one offer carries its camera; every later offer comes from the server.
    async fn sfu_offer(&self, payload: &Value) -> Handled {
        let Some(sdp) = Self::sdp(payload) else {
            return error("invalid offer");
        };
        match self
            .socket
            .state
            .sfu
            .offer(&self.room_id, &self.participant.peer_id, sdp)
            .await
        {
            Ok(answer) => reply(Reply::Ok(json!({ "sdp": answer }))),
            Err(_) => error("offer rejected"),
        }
    }

    async fn sfu_answer(&self, payload: &Value) -> Handled {
        let Some(sdp) = Self::sdp(payload) else {
            return error("invalid answer");
        };
        match self
            .socket
            .state
            .sfu
            .answer(&self.room_id, &self.participant.peer_id, sdp)
            .await
        {
            Ok(()) => reply(Reply::ok()),
            Err(_) => error("answer rejected"),
        }
    }

    async fn sfu_candidate(&self, payload: &Value) -> Handled {
        let Some(candidate) = payload
            .get("candidate")
            .filter(|candidate| candidate.get("candidate").is_some())
        else {
            return error("invalid candidate");
        };
        match self
            .socket
            .state
            .sfu
            .candidate(&self.room_id, &self.participant.peer_id, candidate)
            .await
        {
            Ok(()) => Ok(None),
            Err(_) => error("invalid candidate"),
        }
    }

    /// Which simulcast layer of another seat's board this browser wants.
    async fn sfu_layer(&self, payload: &Value) -> Handled {
        let Some(map) = exactly(payload, 2) else {
            return error("invalid layer");
        };
        let (Some(owner), Some(layer)) = (
            map.get("peer_id").and_then(Value::as_str),
            map.get("layer").and_then(Value::as_str),
        ) else {
            return error("invalid layer");
        };
        if !uuid(owner) || !the_gathering_sfu::valid_layer(layer) {
            return error("invalid layer");
        }
        match self
            .socket
            .state
            .sfu
            .layer(&self.room_id, &self.participant.peer_id, owner, layer)
            .await
        {
            Ok(()) => Ok(None),
            Err(_) => error("unknown board"),
        }
    }

    /// A message for one other seat (card crops for the scanner), relayed as-is.
    async fn peer_message(&self, payload: &Value) -> Handled {
        let Some(map) = exactly(payload, 2) else {
            return error("invalid message");
        };
        let (Some(to), Some(message)) = (
            map.get("to").and_then(Value::as_str),
            map.get("message").filter(|m| m.is_object()),
        ) else {
            return error("invalid message");
        };
        if !uuid(to) || to == self.participant.peer_id {
            return error("invalid recipient");
        }
        if serde_json::to_vec(message)
            .map_or(true, |encoded| encoded.len() > MAX_PEER_MESSAGE_BYTES)
        {
            return error("message too large");
        }
        match self
            .socket
            .state
            .sfu
            .relay(
                &self.room_id,
                &self.participant.peer_id,
                to,
                message.clone(),
            )
            .await
        {
            Ok(()) => Ok(None),
            Err(_) => error("recipient has left"),
        }
    }

    async fn choose_deck(&mut self, payload: &Value) -> Handled {
        let Some(deck_id) = int(payload.get("deck_id")) else {
            return error("invalid deck");
        };
        let state = self.socket.state.clone();
        match webcam::get_deck(&state.pool, deck_id).await? {
            Some(deck) if deck.player_id == self.participant.player_id => {
                self.participant.deck_id = Some(deck.id);
                self.participant.deck_name = Some(deck.name);
                if !self.remember_seat(None).await? {
                    return error("seat has changed; try again");
                }
                self.update_presence();
                // Peers may have cached the deck list before this deck was created or edited.
                let _ = state
                    .io
                    .to(self.topic.clone())
                    .emit("deck_selected", &json!({ "deck_id": deck.id }))
                    .await;
                reply(Reply::ok())
            }
            _ => error("deck does not belong to player"),
        }
    }

    async fn reveal(&mut self, payload: &Value) -> Handled {
        let target = match exactly(payload, 1).and_then(|map| map.get("target")) {
            Some(Value::Null) => None,
            Some(Value::String(target)) => Some(target.clone()),
            _ => return error("invalid reveal"),
        };
        let allowed = match &target {
            None => true,
            Some(target) => *target != self.participant.peer_id && self.present(target),
        };
        if !allowed {
            return error("reveal target must be another seated player");
        }
        if !self.put_reveal(target).await? {
            return error("seat has changed; try again");
        }
        reply(Reply::ok())
    }

    /// Ephemeral state a player publishes about their own seat, carried by presence.
    async fn update_status(&mut self, payload: &Value) -> Handled {
        let Some(mut changes) = object(payload).and_then(StatusChanges::parse) else {
            return error("invalid status");
        };
        // Dropping to zero life knocks a player out in every format. Restoring is
        // deliberately manual, so gaining life back does not silently un-eliminate.
        if changes.life.is_some_and(|life| life <= 0)
            && !self.participant.eliminated
            && changes.eliminated.is_none()
        {
            changes.eliminated = Some(true);
        }
        let eliminated = changes.eliminated;
        changes.apply(&mut self.participant);
        if !self.remember_seat(eliminated).await? {
            return error("seat has changed; try again");
        }
        self.update_presence();
        reply(Reply::ok())
    }

    async fn take_monarch(&self, payload: &Value) -> Handled {
        let me = self.participant.holder();
        if empty(payload) {
            self.tables()
                .take_monarch(&self.room_id, me.clone(), me)
                .await?;
            return reply(Reply::ok());
        }
        // Any player may hand the monarch to another present, seated player.
        let Some(peer_id) = exactly(payload, 1).and_then(|map| map.get("peer_id")?.as_str()) else {
            return error("invalid monarch claim");
        };
        let snapshot = self.tables().snapshot(&self.room_id).await?;
        match snapshot.seats.iter().find(|seat| seat.peer_id == peer_id) {
            Some(seat) if self.present(peer_id) => {
                self.tables()
                    .take_monarch(&self.room_id, seat.holder(), me)
                    .await?;
                reply(Reply::ok())
            }
            _ => error("the monarch must go to a seated player"),
        }
    }

    /// The owner may eliminate or restore any present seat; other players only their own.
    async fn set_eliminated(&self, payload: &Value) -> Handled {
        let Some(map) = exactly(payload, 2) else {
            return error("invalid elimination");
        };
        let (Some(peer_id), Some(eliminated)) = (
            map.get("peer_id").and_then(Value::as_str),
            map.get("eliminated").and_then(Value::as_bool),
        ) else {
            return error("invalid elimination");
        };
        let allowed = (self.owner || peer_id == self.participant.peer_id)
            && self
                .tables()
                .snapshot(&self.room_id)
                .await?
                .seats
                .iter()
                .any(|seat| seat.peer_id == peer_id)
            && self.present(peer_id);
        if !allowed {
            return error("player must be present to change elimination");
        }
        self.tables()
            .eliminate(&self.room_id, peer_id, eliminated)
            .await?;
        reply(Reply::ok())
    }

    /// Seat order is shared so every browser records the same turn order. The proposed order
    /// must name exactly the seated peers.
    async fn seat_order(&self, event: &str, payload: &Value) -> Handled {
        let Some(peer_ids) = payload.get("peer_ids").and_then(Value::as_array) else {
            return error("invalid seat order");
        };
        let snapshot = self.tables().snapshot(&self.room_id).await?;
        let mut present: Vec<&str> = snapshot
            .seats
            .iter()
            .map(|seat| seat.peer_id.as_str())
            .collect();
        present.sort_unstable();
        let proposed: Option<Vec<String>> = peer_ids
            .iter()
            .map(|id| id.as_str().map(str::to_owned))
            .collect();
        let Some(proposed) = proposed else {
            return error("seat order must list every seated player");
        };
        let mut sorted: Vec<&str> = proposed.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        if sorted != present {
            return error("seat order must list every seated player");
        }
        if event == "arrange_seats" || snapshot.mode != Mode::Commander {
            reply(self.tables().arrange(&self.room_id, proposed).await?)
        } else {
            self.tables().order(&self.room_id, proposed).await?;
            reply(Reply::ok())
        }
    }

    async fn set_mode(&self, payload: &Value) -> Handled {
        match exactly(payload, 1)
            .and_then(|map| map.get("mode")?.as_str())
            .and_then(Mode::parse)
        {
            Some(mode) => reply(self.tables().set_mode(&self.room_id, mode).await?),
            None => error("invalid game mode"),
        }
    }

    async fn adjust_team_life(&self, payload: &Value) -> Handled {
        let parsed = exactly(payload, 2).and_then(|map| {
            let team = int(map.get("team_index")).filter(|team| *team >= 0)?;
            let delta = int(map.get("delta")).filter(|delta| (-1998..=1998).contains(delta))?;
            Some((team, delta))
        });
        match parsed {
            Some((team, delta)) => reply(
                self.tables()
                    .adjust_team_life(&self.room_id, self.actor(), team, delta)
                    .await?,
            ),
            None => error("invalid team life adjustment"),
        }
    }

    /// An explicit `randomize` overrides the room's auto-randomize setting for this start.
    async fn start_game(&self, payload: &Value) -> Handled {
        let randomize = if empty(payload) {
            None
        } else {
            match exactly(payload, 1).and_then(|map| map.get("randomize")?.as_bool()) {
                Some(randomize) => Some(randomize),
                None => return error("invalid start"),
            }
        };
        reply(self.tables().start_game(&self.room_id, randomize).await?)
    }

    async fn adjust_turn(&self, payload: &Value) -> Handled {
        let parsed = exactly(payload, 2).and_then(|map| {
            let player_id = int(map.get("player_id"))?;
            let delta = int(map.get("delta")).filter(|delta| [-1, 1].contains(delta))?;
            Some((player_id, delta))
        });
        match parsed {
            Some((player_id, delta)) => reply(
                self.tables()
                    .adjust_turn(&self.room_id, player_id, delta)
                    .await?,
            ),
            None => error("invalid turn adjustment"),
        }
    }

    async fn roll(&self, payload: &Value) -> Handled {
        let roll = exactly(payload, 2)
            .filter(|map| map.get("kind").and_then(Value::as_str) == Some("dice"))
            .and_then(|map| int(map.get("sides")))
            .filter(|sides| (2..=1000).contains(sides))
            .map(|sides| {
                (
                    RollKind::Dice(sides),
                    RollResult::Number(rand::rng().random_range(1..=sides)),
                )
            })
            .or_else(|| {
                exactly(payload, 1)
                    .filter(|map| map.get("kind").and_then(Value::as_str) == Some("coin"))
                    .map(|_| {
                        let face = if rand::rng().random_bool(0.5) {
                            "Heads"
                        } else {
                            "Tails"
                        };
                        (RollKind::Coin, RollResult::Face(face.into()))
                    })
            });
        let Some((kind, result)) = roll else {
            return error("invalid roll (dice must have 2–1000 sides)");
        };
        self.tables()
            .roll(&self.room_id, self.participant.holder(), kind, result)
            .await?;
        reply(Reply::ok())
    }
}

fn revision(payload: &Value) -> Option<i64> {
    exactly(payload, 1)
        .and_then(|map| int(map.get("revision")))
        .filter(|revision| *revision >= 0)
}

/// Builds the joining participant from the join params, checking the player belongs to the
/// signed-in account.
async fn participant(
    state: &crate::state::AppState,
    params: &Value,
    user: &User,
) -> Result<Seat, String> {
    let (Some(peer_id), Some(player_id)) = (params.get("peer_id"), int(params.get("player_id")))
    else {
        return Err("account is not linked to a player".into());
    };
    let Some(peer_id) = peer_id.as_str().filter(|peer_id| uuid(peer_id)) else {
        return Err("invalid peer id".into());
    };
    let player = webcam::get_player(&state.pool, player_id)
        .await
        .map_err(|_| "join crashed".to_owned())?;
    let Some(player) = player.filter(|player| player.user_id == Some(user.id)) else {
        return Err("account is not linked to this player".into());
    };
    // Default seat order is join order, so every browser sees the same seats.
    let mut seat = Seat::new(peer_id.to_owned(), player.id, player.name, webcam::now());
    if let Some(deck_id) = int(params.get("deck_id"))
        && let Ok(Some(deck)) = webcam::get_deck(&state.pool, deck_id).await
        && deck.player_id == player.id
    {
        seat.deck_id = Some(deck.id);
        seat.deck_name = Some(deck.name);
    }
    Ok(seat)
}
