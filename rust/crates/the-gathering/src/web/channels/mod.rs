//! A channels server speaking the Phoenix Channels V2 protocol, so the frontend's `phoenix`
//! JS client connects to it unchanged.
//!
//! * One task per WebSocket ([`run_socket`]) decodes V2 JSON frames, answers `heartbeat`,
//!   routes `phx_join` to a new channel task per topic and other events to the joined
//!   channel, and owns a writer task that serializes every outbound frame.
//! * One task per joined channel ([`webcam_table::run`]) handles its events in order, like
//!   a channel process. Broadcasts to its topic are fastlaned to the socket by [`pubsub`],
//!   except intercepted events (`presence_diff`), which go through the channel.
//! * When a channel stops normally the client gets `phx_close`; abnormally, `phx_error`
//!   (and phoenix.js rejoins). When the socket closes, every channel stops silently.
//! * Logging out broadcasts the session's topic on `state.session_disconnects`; sockets
//!   opened with that session close.

pub mod presence;
pub mod protocol;
pub mod pubsub;
pub mod rooms;
pub mod webcam_table;

use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use crate::accounts::User;
use crate::crypto;
use crate::state::AppState;
use crate::web::auth::user_session_topic;

use self::protocol::{Frame, Outbound, Reply, encode};

/// The purpose socket tokens are sealed for.
pub const TOKEN_PURPOSE: &str = "webcam-table-socket";
/// Socket tokens expire after a day.
pub const TOKEN_MAX_AGE_SECONDS: i64 = 86_400;
/// Inbound frames are capped above the largest legitimate signal (SDP offers, card crops).
pub const MAX_FRAME_SIZE: usize = 393_216;
/// The topic prefix the webcam table channel serves.
pub const WEBCAM_TABLE_PREFIX: &str = "webcam_table:";

/// What a socket token carries.
#[derive(serde::Serialize, serde::Deserialize)]
struct SocketToken {
    /// The cookie session's token, base64url.
    session: String,
    /// Unix seconds after which the token is refused.
    expires_at: i64,
}

/// Seals the cookie session token for the browser to pass as the `token` connect param.
/// Encrypted, not just signed, so page scripts cannot read the session token out of it.
pub fn socket_token(state: &AppState, session_token: &[u8]) -> String {
    let token = SocketToken {
        session: crypto::url_encode64_unpadded(session_token),
        expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + TOKEN_MAX_AGE_SECONDS,
    };
    crypto::seal(
        &state.config.secret_key_base,
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
    let session_token = crypto::open(&state.config.secret_key_base, TOKEN_PURPOSE, token)
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

/// What the socket tells a channel task.
#[derive(Debug)]
pub enum ClientMsg {
    /// A frame for this channel.
    Frame(Frame),
    /// A duplicate join replaced this channel: stop with `phx_close`.
    Shutdown,
}

/// The socket a channel belongs to.
#[derive(Clone)]
pub struct SocketCtx {
    /// App state.
    pub state: AppState,
    /// The signed-in user.
    pub user: User,
    /// Outbound frames.
    pub out: mpsc::UnboundedSender<Outbound>,
    /// Tells the socket a channel ended: `(topic, join_ref)`.
    pub exited: mpsc::UnboundedSender<(String, Option<String>)>,
}

impl SocketCtx {
    /// Queues a frame.
    pub fn send(
        &self,
        join_ref: Option<&str>,
        ref_: Option<&str>,
        topic: &str,
        event: &str,
        payload: &serde_json::Value,
    ) {
        let _ = self.out.send(Outbound::Text(
            encode(join_ref, ref_, topic, event, payload).into(),
        ));
    }
}

struct Joined {
    join_ref: Option<String>,
    tx: mpsc::UnboundedSender<ClientMsg>,
}

/// Serves one WebSocket until it closes or its session is revoked.
pub async fn run_socket(state: AppState, socket: WebSocket, user: User, session_token: Vec<u8>) {
    let (mut sink, mut stream) = socket.split();
    let (out, mut outbound) = mpsc::unbounded_channel::<Outbound>();
    let writer = tokio::spawn(async move {
        while let Some(frame) = outbound.recv().await {
            let message = match frame {
                Outbound::Text(text) => Message::Text(text),
                Outbound::Close => break,
            };
            if sink.send(message).await.is_err() {
                return;
            }
        }
        let _ = sink.send(Message::Close(None)).await;
        let _ = sink.close().await;
    });

    let (exited, mut exits) = mpsc::unbounded_channel();
    let ctx = SocketCtx {
        state: state.clone(),
        user,
        out: out.clone(),
        exited,
    };
    let session_topic = user_session_topic(&session_token);
    let mut disconnects = state.session_disconnects.subscribe();
    let mut listening = true;
    let mut channels: HashMap<String, Joined> = HashMap::new();

    loop {
        tokio::select! {
            message = stream.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Some(frame) = Frame::decode(text.as_str()) {
                        route(&ctx, &mut channels, frame);
                    } else {
                        tracing::debug!("dropping undecodable socket frame");
                    }
                }
                Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
            },
            Some((topic, join_ref)) = exits.recv() => {
                if channels.get(&topic).is_some_and(|joined| joined.join_ref == join_ref) {
                    channels.remove(&topic);
                }
            },
            revoked = disconnects.recv(), if listening => match revoked {
                Ok(topic) if topic == session_topic => break,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => listening = false,
            },
        }
    }
    // Dropping the channel senders stops every channel task.
    channels.clear();
    let _ = out.send(Outbound::Close);
    drop(out);
    let _ = writer.await;
}

fn route(ctx: &SocketCtx, channels: &mut HashMap<String, Joined>, frame: Frame) {
    let reply = |reply: Reply| {
        ctx.send(
            frame.join_ref.as_deref(),
            frame.ref_.as_deref(),
            &frame.topic,
            "phx_reply",
            &reply.payload(),
        );
    };
    match (frame.topic.as_str(), frame.event.as_str()) {
        ("phoenix", "heartbeat") => reply(Reply::ok()),
        (topic, "phx_join") => {
            if !topic.starts_with(WEBCAM_TABLE_PREFIX) {
                reply(Reply::reason("unmatched topic"));
                return;
            }
            // A duplicate join closes the earlier channel first, as Phoenix does.
            if let Some(previous) = channels.remove(topic) {
                let _ = previous.tx.send(ClientMsg::Shutdown);
            }
            let (tx, rx) = mpsc::unbounded_channel();
            channels.insert(
                topic.to_owned(),
                Joined {
                    join_ref: frame.join_ref.clone(),
                    tx,
                },
            );
            tokio::spawn(webcam_table::run(ctx.clone(), frame, rx));
        }
        (topic, _) => match channels.get(topic) {
            Some(joined) if frame.join_ref.is_none() || frame.join_ref == joined.join_ref => {
                let _ = joined.tx.send(ClientMsg::Frame(frame));
            }
            _ => ctx.send(
                frame.join_ref.as_deref(),
                frame.ref_.as_deref(),
                &frame.topic,
                "phx_reply",
                &json!({ "status": "error", "response": { "reason": "unmatched topic" } }),
            ),
        },
    }
}
