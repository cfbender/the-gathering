//! Webcam table channel test harness: the real router served on 127.0.0.1:0, a minimal
//! Socket.IO (v5 over Engine.IO v4, WebSocket transport) client, and fixtures.
//!
//! Each client has its own message queue, so tests assert on the client that should receive
//! a message.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use the_gathering::accounts::User;
use the_gathering::config::Config;
use the_gathering::web::channels;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::support::TestApp;

/// Alice's peer id in every table test.
pub const PEER_A: &str = "00000000-0000-4000-8000-00000000000a";
pub const PEER_B: &str = "00000000-0000-4000-8000-00000000000b";
pub const PEER_C: &str = "00000000-0000-4000-8000-00000000000c";
pub const PEER_D: &str = "00000000-0000-4000-8000-00000000000d";
pub const PEER_E: &str = "00000000-0000-4000-8000-00000000000e";
pub const PEER_F: &str = "00000000-0000-4000-8000-00000000000f";

const TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_USER: AtomicU64 = AtomicU64::new(1);

/// `peer(index)`: a canonical UUID ending in `index`.
pub fn peer(index: u32) -> String {
    format!("00000000-0000-4000-8000-{index:012}")
}

/// A random room id.
pub fn room_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// One decoded server message: an event, or the acknowledgement (`event == "ack"`) of the
/// client's event `ack`.
#[derive(Clone, Debug)]
pub struct Msg {
    pub event: String,
    pub ack: Option<String>,
    pub payload: Value,
}

/// Decodes a Socket.IO packet on the default namespace (`2` event, `3` ack, `1` disconnect).
fn decode(packet: &str) -> Option<Msg> {
    let kind = packet.chars().next()?;
    let rest = &packet[1..];
    let split = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (id, data) = rest.split_at(split);
    let id = (!id.is_empty()).then(|| id.to_owned());
    match kind {
        '1' => Some(Msg {
            event: "socket_closed".into(),
            ack: None,
            payload: Value::Null,
        }),
        '2' => {
            let mut args: Vec<Value> = serde_json::from_str(data).expect("event packet");
            let event = args.remove(0).as_str().expect("event name").to_owned();
            let payload = args.into_iter().next().unwrap_or(Value::Null);
            Some(Msg {
                event,
                ack: id,
                payload,
            })
        }
        '3' => {
            let args: Vec<Value> = serde_json::from_str(data).expect("ack packet");
            let payload = args.into_iter().next().unwrap_or(Value::Null);
            Some(Msg {
                event: "ack".into(),
                ack: id,
                payload,
            })
        }
        _ => None,
    }
}

/// A table socket with (at most) one joined table.
pub struct Client {
    out: mpsc::UnboundedSender<Message>,
    rx: mpsc::UnboundedReceiver<Msg>,
    buffer: VecDeque<Msg>,
    next_ack: u64,
    /// The joined room id.
    pub room: String,
    /// The join reply's participant.
    pub participant: Value,
    /// The join reply's `owner`.
    pub owner: bool,
    /// Set once the server closed the socket.
    pub socket_closed: bool,
}

impl Client {
    /// Connects to `/socket.io/` with `token`; the `connect_error` message when refused.
    pub async fn connect(addr: SocketAddr, token: &str) -> Result<Self, String> {
        Self::connect_with(addr, json!({ "token": token })).await
    }

    /// Connects with an arbitrary `auth` payload.
    pub async fn connect_with(addr: SocketAddr, auth: Value) -> Result<Self, String> {
        let url = format!("ws://{addr}/socket.io/?EIO=4&transport=websocket");
        let (stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("websocket connect");
        let (mut sink, mut stream) = stream.split();
        let (out, mut outbound) = mpsc::unbounded_channel::<Message>();
        tokio::spawn(async move {
            while let Some(message) = outbound.recv().await {
                let close = matches!(message, Message::Close(_));
                if sink.send(message).await.is_err() || close {
                    break;
                }
            }
        });
        // The Engine.IO handshake, then the namespace connect.
        let open = stream.next().await.expect("open").expect("open frame");
        assert!(open.to_text().expect("text").starts_with('0'), "{open:?}");
        out.send(Message::text(format!("40{auth}")))
            .expect("connect");
        let reply = loop {
            let frame = stream.next().await.expect("connect reply").expect("frame");
            match frame.to_text().expect("text") {
                "2" => out.send(Message::text("3")).expect("pong"),
                reply => break reply.to_owned(),
            }
        };
        if let Some(error) = reply.strip_prefix("44") {
            let error: Value = serde_json::from_str(error).expect("connect_error");
            return Err(error["message"].as_str().unwrap_or_default().to_owned());
        }
        assert!(reply.starts_with("40"), "{reply}");

        let (tx, rx) = mpsc::unbounded_channel();
        let pong = out.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                match text.as_str() {
                    // Engine.IO ping.
                    "2" => {
                        let _ = pong.send(Message::text("3"));
                    }
                    "1" => break,
                    packet => {
                        let Some(message) = packet.strip_prefix('4').and_then(decode) else {
                            continue;
                        };
                        if tx.send(message).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = tx.send(Msg {
                event: "socket_closed".into(),
                ack: None,
                payload: Value::Null,
            });
        });
        Ok(Self {
            out,
            rx,
            buffer: VecDeque::new(),
            next_ack: 0,
            room: String::new(),
            participant: Value::Null,
            owner: false,
            socket_closed: false,
        })
    }

    /// Sends a raw Socket.IO packet (without the Engine.IO `4` message prefix).
    pub fn send(&self, packet: &str) {
        self.out
            .send(Message::text(format!("4{packet}")))
            .expect("send packet");
    }

    async fn receive(&mut self, timeout: Duration) -> Option<Msg> {
        match tokio::time::timeout(timeout, self.rx.recv()).await {
            Ok(Some(message)) => {
                if message.event == "socket_closed" {
                    self.socket_closed = true;
                }
                Some(message)
            }
            _ => None,
        }
    }

    /// Joins `room`; returns the reply status and response.
    pub async fn join(&mut self, room: &str, mut payload: Value) -> (String, Value) {
        room.clone_into(&mut self.room);
        payload["room_id"] = json!(room);
        let (status, response) = self.call("join", payload).await;
        if status == "ok" {
            self.participant = response["participant"].clone();
            self.owner = response["owner"] == json!(true);
        }
        (status, response)
    }

    /// Emits an event asking for an acknowledgement; returns its ack id.
    pub fn push(&mut self, event: &str, payload: Value) -> String {
        self.next_ack += 1;
        let id = self.next_ack.to_string();
        self.send(&format!(
            "2{id}{}",
            Value::Array(vec![json!(event), payload])
        ));
        id
    }

    /// Waits for the acknowledgement `id`: `("ok", response)` or `("error", {"reason": ...})`.
    pub async fn reply(&mut self, id: &str) -> (String, Value) {
        let wanted = id.to_owned();
        let Some(message) = self
            .take_where(move |message| {
                message.event == "ack" && message.ack.as_deref() == Some(&wanted)
            })
            .await
        else {
            panic!("no reply to {id}; buffered: {:?}", self.events());
        };
        match (message.payload.get("ok"), message.payload.get("error")) {
            (Some(response), None) => ("ok".to_owned(), response.clone()),
            (None, Some(reason)) => ("error".to_owned(), json!({ "reason": reason })),
            _ => panic!("malformed reply: {}", message.payload),
        }
    }

    /// Pushes and waits for the reply.
    pub async fn call(&mut self, event: &str, payload: Value) -> (String, Value) {
        let id = self.push(event, payload);
        self.reply(&id).await
    }

    /// Pushes and asserts an ok reply; returns its response.
    pub async fn ok(&mut self, event: &str, payload: Value) -> Value {
        let (status, response) = self.call(event, payload.clone()).await;
        assert_eq!(status, "ok", "{event} {payload} replied {response}");
        response
    }

    /// Pushes and asserts an error reply; returns its response.
    pub async fn err(&mut self, event: &str, payload: Value) -> Value {
        let (status, response) = self.call(event, payload.clone()).await;
        assert_eq!(status, "error", "{event} {payload} replied ok {response}");
        response
    }

    /// Pushes and asserts an error reply with `reason`.
    pub async fn refused(&mut self, event: &str, payload: Value, reason: &str) {
        let response = self.err(event, payload.clone()).await;
        assert_eq!(response, json!({ "reason": reason }), "{event} {payload}");
    }

    /// Removes and returns the first message matching `predicate`, waiting for it.
    pub async fn take_where(&mut self, predicate: impl Fn(&Msg) -> bool) -> Option<Msg> {
        if let Some(index) = self.buffer.iter().position(&predicate) {
            return self.buffer.remove(index);
        }
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let message = self.receive(remaining).await?;
            if predicate(&message) {
                return Some(message);
            }
            self.buffer.push_back(message);
        }
    }

    /// The next `event` (buffered or arriving).
    pub async fn expect(&mut self, event: &str) -> Value {
        let wanted = event.to_owned();
        match self
            .take_where(move |message| message.event == wanted)
            .await
        {
            Some(message) => message.payload,
            None => panic!("no {event}; buffered: {:?}", self.events()),
        }
    }

    /// The next `event` whose payload satisfies `predicate`.
    pub async fn expect_where(&mut self, event: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        let wanted = event.to_owned();
        match self
            .take_where(move |message| message.event == wanted && predicate(&message.payload))
            .await
        {
            Some(message) => message.payload,
            None => panic!("no matching {event}; buffered: {:?}", self.events()),
        }
    }

    /// No `event` matching `predicate` within a short wait.
    pub async fn refute_where(&mut self, event: &str, predicate: impl Fn(&Value) -> bool) {
        self.settle(Duration::from_millis(100)).await;
        let found = self
            .buffer
            .iter()
            .find(|message| message.event == event && predicate(&message.payload));
        assert!(
            found.is_none(),
            "unexpected {event}: {:?}",
            found.map(|message| &message.payload)
        );
    }

    /// No `event` with any payload.
    pub async fn refute(&mut self, event: &str) {
        self.refute_where(event, |_| true).await;
    }

    /// Buffers whatever arrives within `wait`.
    pub async fn settle(&mut self, wait: Duration) {
        while let Some(message) = self.receive(wait).await {
            self.buffer.push_back(message);
        }
    }

    /// Forgets buffered messages.
    pub fn drain(&mut self) {
        self.buffer.clear();
    }

    /// Buffered event names.
    pub fn events(&self) -> Vec<String> {
        self.buffer
            .iter()
            .map(|message| message.event.clone())
            .collect()
    }

    /// Leaves the table.
    pub async fn leave(&mut self) {
        let (status, _) = self.call("leave", json!({})).await;
        assert_eq!(status, "ok");
    }

    /// Closes the WebSocket.
    pub fn close(self) {
        let _ = self.out.send(Message::Close(None));
    }

    /// The joined participant's peer id.
    pub fn peer_id(&self) -> String {
        self.participant["peer_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// The joined participant's player id.
    pub fn player_id(&self) -> i64 {
        self.participant["player_id"].as_i64().unwrap_or_default()
    }
}

/// The presence roster entry for `peer`, if listed.
pub fn listed<'a>(roster: &'a Value, peer: &str) -> Option<&'a Value> {
    roster
        .as_array()
        .expect("roster")
        .iter()
        .find(|meta| meta["peer_id"] == peer)
}

/// The app plus a live server.
pub struct Server {
    pub app: TestApp,
    pub addr: SocketAddr,
}

impl Server {
    /// Serves a fresh app on 127.0.0.1:0.
    pub async fn start() -> Self {
        Self::with_config(|_| {}).await
    }

    /// Serves a fresh app after adjusting its configuration.
    pub async fn with_config(adjust: impl FnOnce(&mut Config)) -> Self {
        let app = TestApp::with_config(adjust).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("address");
        let router = app.router.clone();
        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .expect("serve");
        });
        Self { app, addr }
    }

    pub fn state(&self) -> &the_gathering::state::AppState {
        &self.app.state
    }

    /// A member account.
    pub async fn user(&self) -> User {
        let n = NEXT_USER.fetch_add(1, Ordering::Relaxed);
        self.app.member(&format!("user{n}")).await
    }

    /// An administrator account.
    pub async fn admin(&self) -> User {
        let n = NEXT_USER.fetch_add(1, Ordering::Relaxed);
        self.app.admin(&format!("admin{n}")).await
    }

    /// Inserts a player.
    pub async fn player(&self, name: &str, user_id: Option<i64>) -> i64 {
        sqlx::query(
            "INSERT INTO players (name, user_id, inserted_at, updated_at) VALUES (?, ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(name)
        .bind(user_id)
        .execute(self.app.pool())
        .await
        .expect("player")
        .last_insert_rowid()
    }

    /// Inserts a deck.
    pub async fn deck(&self, player_id: i64, name: &str, commander: &str) -> i64 {
        sqlx::query(
            "INSERT INTO decks (player_id, name, commander_name, inserted_at, updated_at) VALUES (?, ?, ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(player_id)
        .bind(name)
        .bind(commander)
        .execute(self.app.pool())
        .await
        .expect("deck")
        .last_insert_rowid()
    }

    /// A member with a linked player.
    pub async fn linked_player(&self, name: &str) -> (User, i64) {
        let user = self.user().await;
        let player = self.player(name, Some(user.id)).await;
        (user, player)
    }

    /// A fresh session for `user`, encrypted as a socket token.
    pub async fn token(&self, user: &User) -> String {
        let session = self
            .state()
            .accounts
            .generate_user_session_token(user)
            .await
            .expect("session token");
        channels::socket_token(self.state(), &session)
    }

    /// A connected socket for `user`.
    pub async fn connect(&self, user: &User) -> Client {
        Client::connect(self.addr, &self.token(user).await)
            .await
            .expect("socket")
    }

    /// Joins `room` as `player_id` (owned by `user`); returns the reply and the client.
    pub async fn try_join(
        &self,
        user: &User,
        player_id: i64,
        room: &str,
        peer: &str,
    ) -> (String, Value, Client) {
        let mut client = self.connect(user).await;
        let (status, response) = client
            .join(room, json!({ "peer_id": peer, "player_id": player_id }))
            .await;
        (status, response, client)
    }

    /// Joins and waits for `after_join`'s pushes.
    pub async fn join_as(&self, user: &User, player_id: i64, room: &str, peer: &str) -> Client {
        let (status, response, mut client) = self.try_join(user, player_id, room, peer).await;
        assert_eq!(status, "ok", "join refused: {response}");
        // `table_log` is after_join's last push; the others stay buffered.
        let log = client.expect("table_log").await;
        client.buffer.push_back(Msg {
            event: "table_log".into(),
            ack: None,
            payload: log,
        });
        client
    }

    /// A new user and player named `name`.
    pub async fn join_player(&self, room: &str, peer: &str, name: &str) -> Client {
        let (user, player) = self.linked_player(name).await;
        self.join_as(&user, player, room, peer).await
    }

    /// A new player named after its peer id.
    pub async fn join_seat(&self, room: &str, peer: &str) -> Client {
        self.join_player(room, peer, peer).await
    }

    /// The player's account joins again under `peer`.
    pub async fn rejoin(&self, room: &str, user: &User, player_id: i64, peer: &str) -> Client {
        self.join_as(user, player_id, room, peer).await
    }

    /// Leaves and waits until the room has handled the connection going away.
    pub async fn disconnect(&self, client: &mut Client, room: &str) {
        let player_id = client.player_id();
        client.leave().await;
        self.wait_departed(room, player_id).await;
    }

    /// Waits until the room no longer counts `player_id` as connected.
    pub async fn wait_departed(&self, room: &str, player_id: i64) {
        let tables = &self.state().webcam_tables;
        wait_until(|| async {
            tables
                .debug(room)
                .await
                .is_ok_and(|debug| !debug.connections.contains(&player_id))
        })
        .await;
    }
}

/// Polls `condition` until it holds (or panics after the timeout).
pub async fn wait_until<F, Fut>(condition: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    while !condition().await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "condition never held"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Each test starts with Alice seated as `PEER_A` in a fresh room.
pub struct Table {
    pub server: Server,
    pub room: String,
    pub alice: Client,
    pub user: User,
    pub player: i64,
    pub deck: i64,
}

impl Table {
    /// The standard setup.
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    /// The standard setup with an adjusted configuration.
    pub async fn with_config(adjust: impl FnOnce(&mut Config)) -> Self {
        let server = Server::with_config(adjust).await;
        let user = server.user().await;
        let player = server.player("Alice", Some(user.id)).await;
        let deck = server.deck(player, "Birds", "Kangee").await;
        let room = room_id();
        let alice = server.join_as(&user, player, &room, PEER_A).await;
        Self {
            server,
            room,
            alice,
            user,
            player,
            deck,
        }
    }

    /// The table's presence topic.
    pub fn topic(&self) -> String {
        the_gathering::webcam::topic(&self.room)
    }

    /// The room's snapshot.
    pub async fn snapshot(&self) -> the_gathering::webcam::room::Snapshot {
        self.server
            .state()
            .webcam_tables
            .snapshot(&self.room)
            .await
            .expect("room running")
    }

    /// The room's log.
    pub async fn log(&self) -> Vec<the_gathering::webcam::log::LogEntry> {
        self.server
            .state()
            .webcam_tables
            .log(&self.room)
            .await
            .expect("room running")
    }

    /// `peer`'s presence meta.
    pub fn meta(&self, peer: &str) -> Value {
        self.server
            .state()
            .presence
            .get(&self.topic(), peer)
            .expect("present")
    }

    /// Waits until `peer`'s meta satisfies `condition` (presence updates from room events land
    /// just after the triggering reply).
    pub async fn wait_meta(&self, peer: &str, condition: impl Fn(&Value) -> bool) -> Value {
        wait_until(|| async { condition(&self.meta(peer)) }).await;
        self.meta(peer)
    }

    /// The seat of `peer` in the snapshot.
    pub async fn seat(&self, peer: &str) -> the_gathering::webcam::seat::Seat {
        self.snapshot()
            .await
            .seats
            .into_iter()
            .find(|seat| seat.peer_id == peer)
            .expect("seat")
    }

    pub async fn join_player(&self, peer: &str, name: &str) -> Client {
        self.server.join_player(&self.room, peer, name).await
    }

    pub async fn join_seat(&self, peer: &str) -> Client {
        self.server.join_seat(&self.room, peer).await
    }

    pub async fn rejoin(&self, peer: &str) -> Client {
        self.server
            .rejoin(&self.room, &self.user, self.player, peer)
            .await
    }

    pub async fn disconnect(&self, client: &mut Client) {
        self.server.disconnect(client, &self.room).await;
    }
}
