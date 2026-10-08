//! The webcam table's realtime connection: Socket.IO, served by socketioxide on `/socket.io/`.
//!
//! * A socket authenticates while connecting with the sealed token from
//!   `GET /api/webcam-table/config` (`auth: { token }`); a refused token gets a `connect_error`.
//! * Every event a socket sends is forwarded, in arrival order, to one task per socket
//!   ([`run_socket`]). A `join` starts a table channel ([`webcam_table::run`]) and later events go
//!   to it; a socket sits at one table at a time, and joining again replaces the channel.
//! * Events that expect a reply are acknowledged with `{"ok": response}` or `{"error": reason}`.
//! * Table broadcasts go to the Socket.IO room named after the table's topic, and every presence
//!   change sends the room the full roster ([`presence`]).
//! * When a channel fails (its room or SFU connection crashed) the client gets `rejoin` and joins
//!   again with a new peer id. When the socket disconnects, its channel stops silently.
//! * Logging out broadcasts the session's topic on `state.session_disconnects`; sockets opened with
//!   that session disconnect.

pub mod presence;
pub mod rooms;
pub mod webcam_table;

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use socketioxide::extract::{AckSender, Event, Extension, SocketRef, TryData};
use socketioxide::handler::{ConnectHandler, FromMessageParts, MessageHandler};
use socketioxide::layer::SocketIoLayer;
use socketioxide::socket::Socket;
use socketioxide::{SocketIo, TransportType};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::accounts::User;
use crate::crypto;
use crate::state::{AppState, WeakAppState};
use crate::web::auth::user_session_topic;

/// The purpose socket tokens are sealed for.
pub const TOKEN_PURPOSE: &str = "webcam-table-socket";
/// Socket tokens expire after a day.
pub const TOKEN_MAX_AGE_SECONDS: i64 = 86_400;
/// Inbound messages are capped above the largest legitimate signal (SDP offers, card crops).
pub const MAX_FRAME_SIZE: usize = 393_216;
/// Packets queued for one socket before further emits to it are dropped. A table sends bursts
/// (a join's snapshot, every seat's candidates), so this is well above socketioxide's 128.
const MAX_BUFFERED_PACKETS: usize = 1_024;

/// What a socket token carries.
#[derive(serde::Serialize, serde::Deserialize)]
struct SocketToken {
    /// The cookie session's token, base64url.
    session: String,
    /// Unix seconds after which the token is refused.
    expires_at: i64,
}

/// Seals the cookie session token for the browser to send when it connects. Encrypted, not just
/// signed, so page scripts cannot read the session token out of it.
pub fn socket_token(state: &AppState, session_token: &[u8]) -> String {
    let token = SocketToken {
        session: crypto::url_encode64_unpadded(session_token),
        expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + TOKEN_MAX_AGE_SECONDS,
    };
    crypto::seal(
        &state.config.secret_key,
        TOKEN_PURPOSE,
        &serde_json::to_vec(&token).unwrap_or_default(),
    )
}

/// Authenticates a socket token. The session token is looked up rather than trusted, so
/// logging out (which deletes it) also refuses new sockets.
pub async fn authenticate(
    state: &AppState,
    token: &str,
) -> Result<Option<(User, Vec<u8>)>, sqlx::Error> {
    let session_token = crypto::open(&state.config.secret_key, TOKEN_PURPOSE, token)
        .and_then(|plain| serde_json::from_slice::<SocketToken>(&plain).ok())
        .filter(|token| token.expires_at > time::OffsetDateTime::now_utc().unix_timestamp())
        .and_then(|token| crypto::url_decode64_unpadded(&token.session));
    let Some(session_token) = session_token else {
        return Ok(None);
    };
    Ok(state
        .accounts
        .get_user_by_session_token(&session_token)
        .await?
        .map(|(user, _)| (user, session_token)))
}

/// The Socket.IO layer for the router and the handle that emits to sockets and rooms.
pub fn build() -> (SocketIoLayer, SocketIo) {
    SocketIo::builder()
        .transports([TransportType::Websocket])
        .ws_max_message_size(MAX_FRAME_SIZE)
        .ws_max_frame_size(MAX_FRAME_SIZE)
        .max_payload(u64::try_from(MAX_FRAME_SIZE).unwrap_or(u64::MAX))
        .max_buffer_size(MAX_BUFFERED_PACKETS)
        .build_layer()
}

/// Serves table sockets on `state.io`. The handlers hold the state weakly, since the state owns
/// the Socket.IO handle that owns them.
pub fn serve(state: &AppState) {
    let weak = state.downgrade();
    let authorize = move |socket: SocketRef, TryData(auth): TryData<ConnectAuth>| {
        let weak = weak.clone();
        async move { authorize(&weak, &socket, auth.ok()).await }
    };
    state.io.ns("/", connected.with(authorize));
}

/// The `auth` payload a client connects with.
#[derive(serde::Deserialize)]
struct ConnectAuth {
    token: String,
}

/// Why a connection was refused; the client receives it as the `connect_error` message.
#[derive(Debug, thiserror::Error)]
enum Refused {
    #[error("unauthorized")]
    Unauthorized,
    #[error("unavailable")]
    Unavailable,
}

/// An authenticated socket waiting for its task to start.
struct Connection {
    state: AppState,
    user: User,
    session_token: Vec<u8>,
    inbox: mpsc::UnboundedReceiver<Inbound>,
}

/// Hands the [`Connection`] from the middleware to the connect handler (socket extensions must
/// be `Clone`).
#[derive(Clone)]
struct Pending(Arc<Mutex<Option<Connection>>>);

/// Connect middleware: authenticates the token and starts forwarding events before the client
/// learns it is connected, so its first event cannot be missed.
async fn authorize(
    weak: &WeakAppState,
    socket: &SocketRef,
    auth: Option<ConnectAuth>,
) -> Result<(), Refused> {
    let state = weak.upgrade().ok_or(Refused::Unavailable)?;
    let Some(auth) = auth else {
        return Err(Refused::Unauthorized);
    };
    let (user, session_token) = match authenticate(&state, &auth.token).await {
        Ok(Some(authenticated)) => authenticated,
        Ok(None) => return Err(Refused::Unauthorized),
        Err(error) => {
            tracing::error!("authenticating a table socket failed: {error}");
            return Err(Refused::Unavailable);
        }
    };
    let (tx, inbox) = mpsc::unbounded_channel();
    socket.on_fallback(Forward(tx.clone()));
    socket.on_disconnect(move || {
        let _ = tx.send(Inbound::Disconnected);
        async {}
    });
    socket
        .extensions
        .insert(Pending(Arc::new(Mutex::new(Some(Connection {
            state,
            user,
            session_token,
            inbox,
        })))));
    Ok(())
}

async fn connected(socket: SocketRef, Extension(pending): Extension<Pending>) {
    socket.extensions.remove::<Pending>();
    let connection = pending
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(connection) = connection {
        run_socket(connection, socket).await;
    }
}

/// An event from the client, with the means to acknowledge it.
pub struct ClientEvent {
    /// Event name.
    pub event: String,
    /// The event's first argument (`null` when it has none or it is not JSON).
    pub payload: Value,
    /// Acknowledges the event; does nothing when the client did not ask for a reply.
    pub ack: AckSender,
}

enum Inbound {
    Event(ClientEvent),
    Disconnected,
}

/// Forwards every event to the socket's task as it is parsed. socketioxide runs ordinary
/// handlers as separate tasks, which would let events overtake each other.
struct Forward(mpsc::UnboundedSender<Inbound>);

/// Marks [`Forward`]'s [`MessageHandler`] impl.
struct Forwarded;

impl MessageHandler<socketioxide::adapter::LocalAdapter, Forwarded> for Forward {
    fn call(&self, socket: Arc<Socket>, mut value: socketioxide::handler::Value, ack: Option<i64>) {
        let Ok(Event(event)) = Event::from_message_parts(&socket, &mut value, &ack) else {
            return;
        };
        let Ok(TryData(payload)) = TryData::<Value>::from_message_parts(&socket, &mut value, &ack);
        let Ok(ack) = AckSender::from_message_parts(&socket, &mut value, &ack);
        let _ = self.0.send(Inbound::Event(ClientEvent {
            event,
            payload: payload.unwrap_or(Value::Null),
            ack,
        }));
    }
}

/// How an event is answered.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// Acknowledged as `{"ok": response}`.
    Ok(Value),
    /// Acknowledged as `{"error": reason}`.
    Error(String),
}

impl Reply {
    /// An empty ok response.
    pub fn ok() -> Self {
        Self::Ok(json!({}))
    }

    /// An error.
    pub fn error(reason: impl Into<String>) -> Self {
        Self::Error(reason.into())
    }

    /// Acknowledges the event this answers.
    pub fn send(&self, ack: AckSender) {
        let body = match self {
            Self::Ok(response) => json!({ "ok": response }),
            Self::Error(reason) => json!({ "error": reason }),
        };
        let _ = ack.send(&body);
    }
}

impl From<Result<(), String>> for Reply {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(reason) => Self::Error(reason),
        }
    }
}

/// What the socket tells its table channel.
pub enum ClientMsg {
    /// An event for the table.
    Event(ClientEvent),
    /// A new join replaces this channel: stop without telling the client.
    Shutdown,
}

/// The socket a channel belongs to.
#[derive(Clone)]
pub struct SocketCtx {
    /// App state.
    pub state: AppState,
    /// The signed-in user.
    pub user: User,
    /// The Socket.IO socket.
    pub socket: SocketRef,
}

struct Joined {
    tx: mpsc::UnboundedSender<ClientMsg>,
    task: JoinHandle<()>,
}

impl Joined {
    /// Stops the channel and waits until it has left its room.
    async fn stop(self) {
        let _ = self.tx.send(ClientMsg::Shutdown);
        let _ = self.task.await;
    }
}

/// Serves one socket until it disconnects or its session is revoked.
async fn run_socket(connection: Connection, socket: SocketRef) {
    let Connection {
        state,
        user,
        session_token,
        mut inbox,
    } = connection;
    let ctx = SocketCtx {
        state: state.clone(),
        user,
        socket: socket.clone(),
    };
    let session_topic = user_session_topic(&session_token);
    let mut disconnects = state.session_disconnects.subscribe();
    let mut listening = true;
    let mut table: Option<Joined> = None;

    loop {
        tokio::select! {
            inbound = inbox.recv() => match inbound {
                Some(Inbound::Event(event)) if event.event == "join" => {
                    // A table's channel joins and leaves the socket's room, so the previous one
                    // must be gone before the next starts.
                    if let Some(previous) = table.take() {
                        previous.stop().await;
                    }
                    let (tx, rx) = mpsc::unbounded_channel();
                    let task = tokio::spawn(webcam_table::run(ctx.clone(), event, rx));
                    table = Some(Joined { tx, task });
                }
                Some(Inbound::Event(event)) => match &table {
                    Some(joined) if !joined.tx.is_closed() => {
                        let _ = joined.tx.send(ClientMsg::Event(event));
                    }
                    _ => Reply::error("not joined").send(event.ack),
                },
                Some(Inbound::Disconnected) | None => break,
            },
            revoked = disconnects.recv(), if listening => match revoked {
                Ok(topic) if topic == session_topic => {
                    let _ = socket.clone().disconnect();
                    break;
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => listening = false,
            },
        }
    }
    // Dropping the sender stops the channel silently.
    if let Some(joined) = table {
        drop(joined.tx);
        let _ = joined.task.await;
    }
}
