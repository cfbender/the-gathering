//! Webcam table channel test harness (`test/support/webcam_table_channel_case.ex`): the real
//! router served on 127.0.0.1:0, a Phoenix V2 WebSocket client, and fixtures.
//!
//! Phoenix's `ChannelTest` delivered every push and broadcast to the test process; here each
//! client has its own queue, so tests assert on the client that should receive a message.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use the_gathering::accounts::User;
use the_gathering::config::Config;
use the_gathering::web::channels;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

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

/// One decoded server frame.
#[derive(Clone, Debug)]
pub struct Msg {
    pub join_ref: Option<String>,
    pub ref_: Option<String>,
    pub topic: String,
    pub event: String,
    pub payload: Value,
}

type Sink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// A Phoenix socket with (at most) one joined channel.
pub struct Client {
    sink: Sink,
    rx: mpsc::UnboundedReceiver<Msg>,
    buffer: VecDeque<Msg>,
    next_ref: u64,
    pub topic: String,
    pub join_ref: Option<String>,
    /// The join reply's participant.
    pub participant: Value,
    /// The join reply's `owner`.
    pub owner: bool,
    /// Set once the server closed the WebSocket.
    pub socket_closed: bool,
}

impl Client {
    /// Opens `/socket/websocket` with `token`; the HTTP status when refused.
    pub async fn connect(addr: SocketAddr, token: &str) -> Result<Self, u16> {
        let url = format!(
            "ws://{addr}/socket/websocket?token={}&vsn=2.0.0",
            url::form_urlencoded::byte_serialize(token.as_bytes()).collect::<String>()
        );
        let (stream, _) = match tokio_tungstenite::connect_async(url).await {
            Ok(connected) => connected,
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                return Err(response.status().as_u16());
            }
            Err(error) => panic!("websocket connect failed: {error}"),
        };
        let (sink, mut stream) = stream.split();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                if let Message::Text(text) = message {
                    let (join_ref, ref_, topic, event, payload): (
                        Option<String>,
                        Option<String>,
                        String,
                        String,
                        Value,
                    ) = serde_json::from_str(text.as_str()).expect("server frame");
                    if tx
                        .send(Msg {
                            join_ref,
                            ref_,
                            topic,
                            event,
                            payload,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            let _ = tx.send(Msg {
                join_ref: None,
                ref_: None,
                topic: String::new(),
                event: "socket_closed".into(),
                payload: Value::Null,
            });
        });
        Ok(Self {
            sink,
            rx,
            buffer: VecDeque::new(),
            next_ref: 0,
            topic: String::new(),
            join_ref: None,
            participant: Value::Null,
            owner: false,
            socket_closed: false,
        })
    }

    fn make_ref(&mut self) -> String {
        self.next_ref += 1;
        self.next_ref.to_string()
    }

    /// Sends a raw frame.
    pub async fn send(
        &mut self,
        join_ref: Option<&str>,
        ref_: &str,
        topic: &str,
        event: &str,
        payload: Value,
    ) {
        let frame = json!([join_ref, ref_, topic, event, payload]).to_string();
        self.sink
            .send(Message::Text(frame.into()))
            .await
            .expect("send frame");
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

    /// Joins `topic`; returns the reply status and response.
    pub async fn join(&mut self, topic: &str, payload: Value) -> (String, Value) {
        let join_ref = self.make_ref();
        topic.clone_into(&mut self.topic);
        self.join_ref = Some(join_ref.clone());
        self.send(Some(&join_ref), &join_ref, topic, "phx_join", payload)
            .await;
        let (status, response) = self.reply(&join_ref).await;
        if status == "ok" {
            self.participant = response["participant"].clone();
            self.owner = response["owner"] == json!(true);
        }
        (status, response)
    }

    /// Pushes an event on the joined topic; returns its ref.
    pub async fn push(&mut self, event: &str, payload: Value) -> String {
        let ref_ = self.make_ref();
        let (join_ref, topic) = (self.join_ref.clone(), self.topic.clone());
        self.send(join_ref.as_deref(), &ref_, &topic, event, payload)
            .await;
        ref_
    }

    /// Waits for the reply to `ref_`.
    pub async fn reply(&mut self, ref_: &str) -> (String, Value) {
        let wanted = ref_.to_owned();
        let Some(message) = self
            .take_where(move |message| {
                message.event == "phx_reply" && message.ref_.as_deref() == Some(&wanted)
            })
            .await
        else {
            panic!("no reply to {ref_}; buffered: {:?}", self.events());
        };
        (
            message.payload["status"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            message.payload["response"].clone(),
        )
    }

    /// Pushes and waits for the reply.
    pub async fn call(&mut self, event: &str, payload: Value) -> (String, Value) {
        let ref_ = self.push(event, payload).await;
        self.reply(&ref_).await
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

    /// `assert_push`/`assert_broadcast`: the next `event` (buffered or arriving).
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

    /// `refute_push`: no `event` matching `predicate` within a short wait.
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

    /// `refute_push` for any payload.
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

    /// Leaves the channel: an ok reply, then `phx_close`.
    pub async fn leave(&mut self) {
        let (status, _) = self.call("phx_leave", json!({})).await;
        assert_eq!(status, "ok");
        self.expect("phx_close").await;
    }

    /// Closes the WebSocket.
    pub async fn close(mut self) {
        let _ = self.sink.send(Message::Close(None)).await;
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

    /// `Games.create_player/2`.
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

    /// `Games.create_deck/1`.
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
            .join(
                &format!("webcam_table:{room}"),
                json!({ "peer_id": peer, "player_id": player_id }),
            )
            .await;
        (status, response, client)
    }

    /// `subscribe_and_join!`: joins and waits for `after_join`'s pushes.
    pub async fn join_as(&self, user: &User, player_id: i64, room: &str, peer: &str) -> Client {
        let (status, response, mut client) = self.try_join(user, player_id, room, peer).await;
        assert_eq!(status, "ok", "join refused: {response}");
        // `table_log` is after_join's last push; the others stay buffered.
        let log = client.expect("table_log").await;
        client.buffer.push_back(Msg {
            join_ref: client.join_ref.clone(),
            ref_: None,
            topic: client.topic.clone(),
            event: "table_log".into(),
            payload: log,
        });
        client
    }

    /// `join_player/3`: a new user and player named `name`.
    pub async fn join_player(&self, room: &str, peer: &str, name: &str) -> Client {
        let (user, player) = self.linked_player(name).await;
        self.join_as(&user, player, room, peer).await
    }

    /// `join_seat/2`: a new player named after its peer id.
    pub async fn join_seat(&self, room: &str, peer: &str) -> Client {
        self.join_player(room, peer, peer).await
    }

    /// `rejoin/3`: the player's account joins again under `peer`.
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

    pub fn topic(&self) -> String {
        format!("webcam_table:{}", self.room)
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

    /// `Presence.get_by_key(topic, peer)`'s single meta.
    pub fn meta(&self, peer: &str) -> Value {
        let entry = self
            .server
            .state()
            .presence
            .get_by_key(&self.topic(), peer)
            .expect("present");
        let metas = entry["metas"].as_array().expect("metas");
        assert_eq!(metas.len(), 1, "metas: {metas:?}");
        metas[0].clone()
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
