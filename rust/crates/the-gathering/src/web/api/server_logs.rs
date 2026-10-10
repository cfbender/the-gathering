//! `GET /api/admin/server-logs`: live server logs as Server-Sent Events.
//!
//! Routed behind `require_authenticated_user`, `require_admin`, and `require_sudo_mode`; the
//! browser's `EventSource` sends the session cookie (same origin, read-only, so no CSRF
//! token and no credentials in the query string). Nothing is replayed: the stream carries
//! entries logged while it is open. Responses are `Cache-Control: no-store`.
//!
//! Events, each with a JSON `data` line:
//!
//! - `ready` `{"heartbeat_seconds": 5}`: the stream is live. Sent first, with `retry: 3000`.
//! - `log` `{"id", "timestamp", "level", "target", "message", "fields", "request"}`: one
//!   entry; `level` is `debug`, `info`, `warning`, or `error`, `fields` maps names to
//!   strings, and `request` is `{"method", "path", "request_id"}` or `null`.
//! - `gap` `{"dropped": n}`: `n` entries were lost because this stream fell behind.
//! - `unauthorized` `{"reason"}`: terminal. `reason` is `signed_out` (the session ended),
//!   `forbidden` (no longer an administrator), or `sudo_required` (the password
//!   confirmation expired). The stream then closes; clients must not reconnect without
//!   reauthenticating.
//!
//! The session is checked again every [`HEARTBEAT`] and before each batch of entries is
//! sent, and logging out ends the stream at once. A batch holds at most [`MAX_BATCH`]
//! entries gathered over [`BATCH_WINDOW`], which also bounds those checks. When the
//! check itself fails (the database is unavailable) the stream closes without a terminal
//! event and the browser reconnects through the route guards.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::{Stream, StreamExt};
use serde_json::json;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::{broadcast, watch};
use tokio::time::{Interval, MissedTickBehavior};

use crate::accounts::{self, User};
use crate::error::ApiError;
use crate::logs::LogEntry;
use crate::state::AppState;
use crate::web::auth::user_session_topic;
use crate::web::session::Session;

/// How often the session is checked while no entries arrive.
pub const HEARTBEAT: Duration = Duration::from_secs(5);
/// How long entries are gathered into one batch after the first arrives.
pub const BATCH_WINDOW: Duration = Duration::from_millis(200);
/// Most entries sent per batch.
pub const MAX_BATCH: usize = 200;
/// Minutes since the password confirmation that the stream stays open (`require_sudo_mode`).
const SUDO_MINUTES: i64 = 10;

/// Why a stream was ended for authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    /// The session token is gone (logout, revocation, expiry, disabled account).
    SignedOut,
    /// The user is no longer an administrator.
    Forbidden,
    /// The password confirmation expired.
    SudoRequired,
}

impl Denial {
    /// The `reason` sent to the client.
    pub fn reason(self) -> &'static str {
        match self {
            Self::SignedOut => "signed_out",
            Self::Forbidden => "forbidden",
            Self::SudoRequired => "sudo_required",
        }
    }
}

/// One stream message, before it is encoded as an SSE event.
#[derive(Clone, Debug)]
pub enum Frame {
    /// The stream is live.
    Ready,
    /// A log entry.
    Log(Arc<LogEntry>),
    /// Entries lost because the stream fell behind.
    Gap(u64),
    /// Terminal: the session may no longer read logs.
    Unauthorized(Denial),
}

impl Frame {
    fn into_event(self) -> Event {
        match self {
            Self::Ready => Event::default()
                .event("ready")
                .retry(Duration::from_secs(3))
                .data(json!({ "heartbeat_seconds": HEARTBEAT.as_secs() }).to_string()),
            Self::Log(entry) => Event::default()
                .event("log")
                .data(entry.to_json().to_string()),
            Self::Gap(dropped) => Event::default()
                .event("gap")
                .data(json!({ "dropped": dropped }).to_string()),
            Self::Unauthorized(denial) => Event::default()
                .event("unauthorized")
                .data(json!({ "reason": denial.reason() }).to_string()),
        }
    }
}

/// `GET /api/admin/server-logs`.
pub async fn server_logs_stream(
    State(state): State<AppState>,
    session: Session,
) -> Result<Response, ApiError> {
    let token = session.user_token().ok_or(ApiError::Unauthorized)?;
    // The route guards checked the request; check the token the stream will keep checking.
    let user = state.accounts.get_user_by_session_token(&token).await?;
    authorize(
        user.as_ref().map(|(user, _)| user),
        state.config.dev_auto_login,
    )
    .map_err(|denial| match denial {
        Denial::SignedOut => ApiError::Unauthorized,
        Denial::Forbidden => ApiError::Forbidden,
        Denial::SudoRequired => ApiError::SudoRequired,
    })?;
    let events = frames(state, token).map(|frame| Ok::<_, Infallible>(frame.into_event()));
    let mut response = Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    // Reverse proxies must pass events through as they are written.
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(response)
}

/// Whether `user` may keep reading logs.
pub fn authorize(user: Option<&User>, dev_auto_login: bool) -> Result<(), Denial> {
    let user = user.ok_or(Denial::SignedOut)?;
    if !user.is_admin() {
        return Err(Denial::Forbidden);
    }
    if !dev_auto_login && !accounts::sudo_mode(user, SUDO_MINUTES) {
        return Err(Denial::SudoRequired);
    }
    Ok(())
}

/// The stream for the session holding `token`: `Ready`, then entries until the session
/// loses access, the server shuts down, or the client goes away.
pub fn frames(state: AppState, token: Vec<u8>) -> impl Stream<Item = Frame> + Send + 'static {
    let mut ticker = tokio::time::interval(HEARTBEAT);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let feed = Feed {
        logs: state.logs.subscribe(),
        disconnects: state.session_disconnects.subscribe(),
        shutdown: state.logs.shutdown_signal(),
        topic: user_session_topic(&token),
        state,
        token,
        ticker,
        queue: VecDeque::from([Frame::Ready]),
        finished: false,
    };
    futures_util::stream::unfold(feed, Feed::next)
}

enum Wake {
    Shutdown,
    Disconnect(Result<String, RecvError>),
    Heartbeat,
    Log(Result<Arc<LogEntry>, RecvError>),
}

struct Feed {
    state: AppState,
    token: Vec<u8>,
    topic: String,
    logs: broadcast::Receiver<Arc<LogEntry>>,
    disconnects: broadcast::Receiver<String>,
    shutdown: watch::Receiver<bool>,
    ticker: Interval,
    queue: VecDeque<Frame>,
    finished: bool,
}

impl Feed {
    async fn next(mut self) -> Option<(Frame, Self)> {
        loop {
            if let Some(frame) = self.queue.pop_front() {
                return Some((frame, self));
            }
            if self.finished {
                return None;
            }
            self.step().await;
        }
    }

    async fn step(&mut self) {
        let wake = tokio::select! {
            biased;
            _ = self.shutdown.wait_for(|closed| *closed) => Wake::Shutdown,
            message = self.disconnects.recv() => Wake::Disconnect(message),
            _ = self.ticker.tick() => Wake::Heartbeat,
            received = self.logs.recv() => Wake::Log(received),
        };
        match wake {
            Wake::Shutdown | Wake::Disconnect(Err(RecvError::Closed)) => self.finished = true,
            Wake::Disconnect(Ok(topic)) if topic == self.topic => self.deny(Denial::SignedOut),
            Wake::Disconnect(Ok(_)) => {}
            Wake::Disconnect(Err(RecvError::Lagged(_))) | Wake::Heartbeat => self.recheck().await,
            Wake::Log(received) => self.batch(received).await,
        }
    }

    /// Gathers entries for [`BATCH_WINDOW`] after the first, then sends them if the session
    /// still may read them.
    async fn batch(&mut self, first: Result<Arc<LogEntry>, RecvError>) {
        let mut frames = Vec::new();
        match first {
            Ok(entry) => frames.push(Frame::Log(entry)),
            Err(RecvError::Lagged(dropped)) => frames.push(Frame::Gap(dropped)),
            Err(RecvError::Closed) => {
                self.finished = true;
                return;
            }
        }
        tokio::time::sleep(BATCH_WINDOW).await;
        let mut entries = frames.len();
        while entries < MAX_BATCH {
            match self.logs.try_recv() {
                Ok(entry) => {
                    frames.push(Frame::Log(entry));
                    entries += 1;
                }
                Err(TryRecvError::Lagged(dropped)) => frames.push(Frame::Gap(dropped)),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
        self.recheck().await;
        if !self.finished {
            self.queue.extend(frames);
        }
    }

    async fn recheck(&mut self) {
        match self
            .state
            .accounts
            .get_user_by_session_token(&self.token)
            .await
        {
            Ok(found) => {
                let user = found.map(|(user, _)| user);
                if let Err(denial) = authorize(user.as_ref(), self.state.config.dev_auto_login) {
                    self.deny(denial);
                }
            }
            Err(_) => {
                // Fail closed; the client reconnects through the route guards.
                self.queue.clear();
                self.finished = true;
            }
        }
    }

    fn deny(&mut self, denial: Denial) {
        self.queue.clear();
        self.queue.push_back(Frame::Unauthorized(denial));
        self.finished = true;
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use tracing::Level;

    use super::*;
    use crate::config::Config;
    use crate::logs::{LogHub, LogRecord, RequestContext};

    struct Fixture {
        state: AppState,
        token: Vec<u8>,
        _dir: tempfile::TempDir,
    }

    async fn fixture(logs: LogHub) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::for_test(dir.path().join("test.db"), dir.path().join("data"));
        let pool = crate::db::connect(&config.database_path, 1).await.unwrap();
        crate::db::migrate::run(&pool).await.unwrap();
        let state = AppState::new_with_logs(config, pool, logs).unwrap();
        let admin = state
            .accounts
            .create_admin("admin", "long-enough-password")
            .await
            .ok()
            .unwrap();
        let token = state
            .accounts
            .generate_user_session_token(&admin)
            .await
            .unwrap();
        Fixture {
            state,
            token,
            _dir: dir,
        }
    }

    fn log(hub: &LogHub, message: &str) {
        hub.publish(LogRecord {
            level: Level::INFO,
            target: "the_gathering::test".into(),
            message: message.into(),
            fields: vec![("game_id".into(), "7".into())],
            request: Some(RequestContext {
                method: "GET".into(),
                path: "/api/games".into(),
                request_id: "request-id-of-twenty-chars".into(),
            }),
        });
    }

    async fn next(stream: &mut (impl Stream<Item = Frame> + Unpin)) -> Option<Frame> {
        tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn authorization_needs_an_admin_with_recent_sudo() {
        let fixture = fixture(LogHub::default()).await;
        let (mut user, _) = fixture
            .state
            .accounts
            .get_user_by_session_token(&fixture.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(authorize(Some(&user), false), Ok(()));
        assert_eq!(authorize(None, false), Err(Denial::SignedOut));
        user.authenticated_at =
            Some(crate::db::UtcDateTime::now().plus(time::Duration::minutes(-(SUDO_MINUTES + 1))));
        assert_eq!(authorize(Some(&user), false), Err(Denial::SudoRequired));
        assert_eq!(authorize(Some(&user), true), Ok(()));
        user.role = "member".into();
        assert_eq!(authorize(Some(&user), true), Err(Denial::Forbidden));
    }

    #[tokio::test]
    async fn streams_entries_with_their_request_until_the_session_is_revoked() {
        let fixture = fixture(LogHub::default()).await;
        let hub = fixture.state.logs.clone();
        let mut stream = pin!(frames(fixture.state.clone(), fixture.token.clone()));
        assert!(matches!(next(&mut stream).await, Some(Frame::Ready)));

        log(&hub, "first");
        let Some(Frame::Log(entry)) = next(&mut stream).await else {
            panic!("expected a log entry");
        };
        let json = entry.to_json();
        assert_eq!(json["message"], "first");
        assert_eq!(json["level"], "info");
        assert_eq!(json["request"]["method"], "GET");
        assert_eq!(json["fields"]["game_id"], "7");

        fixture
            .state
            .accounts
            .delete_user_session_token(&fixture.token)
            .await
            .unwrap();
        log(&hub, "after revocation");
        assert!(matches!(
            next(&mut stream).await,
            Some(Frame::Unauthorized(Denial::SignedOut))
        ));
        assert!(next(&mut stream).await.is_none());
    }

    #[tokio::test]
    async fn logging_out_ends_the_stream_at_once() {
        let fixture = fixture(LogHub::default()).await;
        let mut stream = pin!(frames(fixture.state.clone(), fixture.token.clone()));
        assert!(matches!(next(&mut stream).await, Some(Frame::Ready)));
        fixture.state.disconnect_session(&fixture.token);
        assert!(matches!(
            next(&mut stream).await,
            Some(Frame::Unauthorized(Denial::SignedOut))
        ));
        assert!(next(&mut stream).await.is_none());
    }

    #[tokio::test]
    async fn withholds_entries_after_demotion_or_sudo_expiry() {
        let fixture = fixture(LogHub::default()).await;
        let hub = fixture.state.logs.clone();
        let mut stream = pin!(frames(fixture.state.clone(), fixture.token.clone()));
        assert!(matches!(next(&mut stream).await, Some(Frame::Ready)));
        sqlx::query("UPDATE users SET role = 'member'")
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        log(&hub, "secret after demotion");
        assert!(matches!(
            next(&mut stream).await,
            Some(Frame::Unauthorized(Denial::Forbidden))
        ));

        let fixture = fixture_with_expired_sudo().await;
        let hub = fixture.state.logs.clone();
        let mut stream = pin!(frames(fixture.state.clone(), fixture.token.clone()));
        assert!(matches!(next(&mut stream).await, Some(Frame::Ready)));
        log(&hub, "after sudo expired");
        assert!(matches!(
            next(&mut stream).await,
            Some(Frame::Unauthorized(Denial::SudoRequired))
        ));
    }

    async fn fixture_with_expired_sudo() -> Fixture {
        let fixture = fixture(LogHub::default()).await;
        let old = crate::db::UtcDateTime::now().plus(time::Duration::minutes(-30));
        sqlx::query("UPDATE users_tokens SET authenticated_at = ?")
            .bind(old)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        fixture
    }

    #[tokio::test]
    async fn reports_entries_lost_while_behind() {
        let fixture = fixture(LogHub::new(4)).await;
        let hub = fixture.state.logs.clone();
        let mut stream = pin!(frames(fixture.state.clone(), fixture.token.clone()));
        assert!(matches!(next(&mut stream).await, Some(Frame::Ready)));
        for n in 0..10 {
            log(&hub, &format!("entry {n}"));
        }
        assert!(matches!(next(&mut stream).await, Some(Frame::Gap(6))));
        let Some(Frame::Log(entry)) = next(&mut stream).await else {
            panic!("expected a log entry");
        };
        assert_eq!(entry.message, "entry 6");
    }

    /// The handler behind the guards the router puts it under, with the session layers.
    fn guarded_router(state: &AppState) -> axum::Router {
        use axum::middleware::{from_fn, from_fn_with_state};

        use crate::web::auth::{
            current_user_layer, require_admin, require_authenticated_user, require_sudo_mode,
        };
        use crate::web::session::{csrf_layer, session_layer};

        axum::Router::new()
            .route(
                "/api/admin/server-logs",
                axum::routing::get(server_logs_stream),
            )
            .route_layer(from_fn_with_state(state.clone(), require_sudo_mode))
            .route_layer(from_fn(require_admin))
            .route_layer(from_fn(require_authenticated_user))
            .layer(from_fn_with_state(state.clone(), current_user_layer))
            .layer(from_fn(csrf_layer))
            .layer(from_fn_with_state(state.clone(), session_layer))
            .with_state(state.clone())
    }

    async fn get_stream(state: &AppState, token: Option<Vec<u8>>) -> Response {
        use tower::ServiceExt;

        let cookie = crate::web::session::encode_cookie(
            &state.session_key,
            &crate::web::session::SessionData {
                user_token: token,
                ..Default::default()
            },
        );
        let request = axum::http::Request::get("/api/admin/server-logs")
            .header(
                header::COOKIE,
                format!("{}={cookie}", crate::web::session::COOKIE),
            )
            .header(header::ACCEPT, "text/event-stream")
            .body(axum::body::Body::empty())
            .unwrap();
        guarded_router(state).oneshot(request).await.unwrap()
    }

    #[tokio::test]
    async fn the_endpoint_streams_uncached_events_to_a_confirmed_admin() {
        use http_body_util::BodyExt;

        let fixture = fixture(LogHub::default()).await;
        let response = get_stream(&fixture.state, Some(fixture.token.clone())).await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/event-stream"
        );
        let mut body = response.into_body();
        let frame = tokio::time::timeout(Duration::from_secs(10), body.frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
        assert!(text.contains("event: ready"), "{text}");
        assert!(text.contains("retry: 3000"), "{text}");

        log(&fixture.state.logs, "over http");
        let frame = tokio::time::timeout(Duration::from_secs(10), body.frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
        assert!(text.contains("event: log"), "{text}");
        assert!(text.contains(r#""message":"over http""#), "{text}");
    }

    #[tokio::test]
    async fn the_endpoint_refuses_without_a_session_or_sudo() {
        let fixture = fixture_with_expired_sudo().await;
        let response = get_stream(&fixture.state, None).await;
        assert_eq!(response.status(), 401);
        let response = get_stream(&fixture.state, Some(fixture.token.clone())).await;
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn shutdown_ends_the_stream() {
        let fixture = fixture(LogHub::default()).await;
        let mut stream = pin!(frames(fixture.state.clone(), fixture.token.clone()));
        assert!(matches!(next(&mut stream).await, Some(Frame::Ready)));
        fixture.state.logs.shut_down();
        assert!(next(&mut stream).await.is_none());
    }
}
