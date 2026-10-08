//! Test harness: a fresh migrated SQLite database per test, the real router, a cookie jar,
//! and automatic CSRF tokens on state-changing requests.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

pub mod discord;

use std::path::PathBuf;
use std::sync::Mutex;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use the_gathering::accounts::User;
use the_gathering::config::Config;
use the_gathering::state::AppState;
use the_gathering::web::session::{COOKIE, SessionData, decode_cookie, encode_cookie};
use the_gathering::{db, web};
use tower::ServiceExt;

/// The password fixtures use.
pub const PASSWORD: &str = "long-enough-password";

/// A running app on a scratch database.
pub struct TestApp {
    pub state: AppState,
    pub router: Router,
    pub cookie: Mutex<Option<String>>,
    _dir: tempfile::TempDir,
}

/// A response with its body read.
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
}

impl TestResponse {
    /// The body as JSON (panics on invalid JSON).
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "invalid JSON ({error}): {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Asserts the status and returns the JSON body.
    #[track_caller]
    pub fn assert_json(&self, status: u16) -> Value {
        assert_eq!(
            self.status.as_u16(),
            status,
            "unexpected status; body: {}",
            self.text()
        );
        self.json()
    }

    /// A header value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The redirect target of a 302.
    #[track_caller]
    pub fn redirected_to(&self) -> String {
        assert_eq!(
            self.status,
            StatusCode::FOUND,
            "not a redirect: {}",
            self.text()
        );
        self.header("location").unwrap_or_default().to_owned()
    }
}

impl TestApp {
    /// A new app with the test configuration.
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    /// A new app after adjusting the configuration.
    pub async fn with_config(adjust: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut config = Config::for_test(dir.path().join("test.db"), dir.path().join("data"));
        config.priv_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../priv");
        adjust(&mut config);
        let pool = db::connect(&config.database_path, 5)
            .await
            .expect("database");
        db::migrate::run(&pool).await.expect("migrations");
        let state = AppState::new(config, pool).expect("state");
        let router = web::router(state.clone());
        Self {
            state,
            router,
            cookie: Mutex::new(None),
            _dir: dir,
        }
    }

    /// The database pool.
    pub fn pool(&self) -> &db::Pool {
        &self.state.pool
    }

    /// The current session (decrypted from the jar).
    pub fn session(&self) -> SessionData {
        self.cookie
            .lock()
            .unwrap()
            .as_deref()
            .and_then(|cookie| decode_cookie(&self.state.session_key, cookie))
            .unwrap_or_default()
    }

    fn store_session(&self, data: &SessionData) {
        *self.cookie.lock().unwrap() = Some(encode_cookie(&self.state.session_key, data));
    }

    /// The session's CSRF token, adding one to the jar's session if it has none.
    pub fn csrf_token(&self) -> String {
        let mut data = self.session();
        if let Some(token) = &data.csrf_token {
            return token.clone();
        }
        let token = "test-csrf-token".to_owned();
        data.csrf_token = Some(token.clone());
        self.store_session(&data);
        token
    }

    /// The raw cookie value in the jar.
    pub fn cookie_value(&self) -> Option<String> {
        self.cookie.lock().unwrap().clone()
    }

    /// Forgets the cookie (a new browser).
    pub fn clear_cookies(&self) {
        *self.cookie.lock().unwrap() = None;
    }

    /// Sends a request; mutating requests carry a valid CSRF token.
    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> TestResponse {
        self.request_with(method, path, body, HeaderMap::new())
            .await
    }

    /// Sends a request with extra headers.
    pub async fn request_with(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        headers: HeaderMap,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method.clone()).uri(path);
        if !matches!(method, Method::GET | Method::HEAD) && !headers.contains_key("x-csrf-token") {
            builder = builder.header("x-csrf-token", self.csrf_token());
        }
        if let Some(cookie) = self.cookie.lock().unwrap().clone() {
            builder = builder.header(header::COOKIE, format!("{COOKIE}={cookie}"));
        }
        for (name, value) in &headers {
            builder = builder.header(name, value);
        }
        let request = match body {
            Some(body) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        for value in headers.get_all(header::SET_COOKIE) {
            let value = value.to_str().unwrap();
            if let Some(rest) = value.strip_prefix(&format!("{COOKIE}=")) {
                let cookie = rest.split(';').next().unwrap_or_default().to_owned();
                *self.cookie.lock().unwrap() = Some(cookie);
            }
        }
        let body = response.into_body().collect().await.unwrap().to_bytes();
        TestResponse {
            status,
            headers,
            body,
        }
    }

    pub async fn get(&self, path: &str) -> TestResponse {
        self.request(Method::GET, path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> TestResponse {
        self.request(Method::POST, path, Some(body)).await
    }

    pub async fn patch(&self, path: &str, body: Value) -> TestResponse {
        self.request(Method::PATCH, path, Some(body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> TestResponse {
        self.request(Method::PUT, path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> TestResponse {
        self.request(Method::DELETE, path, None).await
    }

    /// A tracked session token in a fresh session.
    pub async fn log_in(&self, user: &User) {
        let token = self
            .state
            .accounts
            .generate_user_session_token(user)
            .await
            .unwrap();
        self.store_session(&SessionData {
            user_token: Some(token),
            ..SessionData::default()
        });
    }

    /// Logs in with a password authentication just now (sudo mode).
    pub async fn log_in_sudo(&self, user: &User) {
        let user = User {
            authenticated_at: Some(db::UtcDateTime::now()),
            ..user.clone()
        };
        self.log_in(&user).await;
    }

    /// Creates an account with this username and role.
    pub async fn user(&self, username: &str, role: &str) -> User {
        self.state
            .accounts
            .create_user(&input(json!({
                "username": username,
                "display_name": "Test User",
                "password": PASSWORD,
                "role": role,
            })))
            .await
            .unwrap_or_else(|error| panic!("user fixture: {error:?}"))
    }

    /// An administrator.
    pub async fn admin(&self, username: &str) -> User {
        self.user(username, "admin").await
    }

    /// A member.
    pub async fn member(&self, username: &str) -> User {
        self.user(username, "member").await
    }

    /// Reloads a user.
    pub async fn reload(&self, user: &User) -> Option<User> {
        self.state.accounts.get_user(user.id).await.unwrap()
    }

    /// Inserts a catalog card from Scryfall-shaped JSON merged over test defaults: English
    /// paper, `tst` set, common, Commander legal, released 2024-01-01. `id`, `oracle_id`, and
    /// `name` are required.
    pub async fn catalog_card(
        &self,
        overrides: Value,
    ) -> the_gathering::catalog::card_data::CardData {
        let mut record = json!({
            "lang": "en",
            "games": ["paper"],
            "released_at": "2024-01-01",
            "set": "tst",
            "collector_number": "1",
            "layout": "normal",
            "rarity": "common",
            "legalities": {"commander": "legal"}
        });
        for (key, value) in overrides.as_object().expect("card overrides are an object") {
            record[key] = value.clone();
        }
        let scryfall: lotus::scryfall::ScryfallCard =
            serde_json::from_value(record).expect("Scryfall card");
        let card =
            the_gathering::catalog::card_data::from_scryfall(&scryfall).expect("describes a card");
        let mut conn = self.pool().acquire().await.unwrap();
        the_gathering::catalog::card_data::insert_card(&mut conn, &card)
            .await
            .unwrap();
        card
    }

    /// Inserts a player row directly.
    pub async fn sql_player(&self, name: &str) -> i64 {
        let now = db::UtcDateTime::now();
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO players (name, inserted_at, updated_at) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(name)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    /// Inserts a deck row directly; `extra` sets optional columns such as
    /// `partner_name`, `color_identity`, `commander_card_id`, or `decklist_url`.
    pub async fn sql_deck(
        &self,
        player_id: i64,
        name: &str,
        commander_name: &str,
        extra: Value,
    ) -> i64 {
        let now = db::UtcDateTime::now();
        let text = |key: &str| extra.get(key).and_then(Value::as_str).map(str::to_owned);
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO decks (player_id, name, commander_name, partner_name, commander_card_id, partner_card_id,
               color_identity, decklist_url, inserted_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(player_id)
        .bind(name)
        .bind(commander_name)
        .bind(text("partner_name"))
        .bind(text("commander_card_id"))
        .bind(text("partner_card_id"))
        .bind(text("color_identity").unwrap_or_default())
        .bind(text("decklist_url"))
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    /// Inserts a game and its seats directly: `(player_id, deck_id, seat, result, mvp_card_name)`.
    pub async fn sql_game(&self, seats: &[SqlSeat<'_>]) -> i64 {
        let now = db::UtcDateTime::now();
        let game_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO games (played_at, source, inserted_at, updated_at) VALUES (?, 'manual', ?, ?) RETURNING id",
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .unwrap();
        for (player_id, deck_id, seat, result, mvp) in seats {
            sqlx::query(
                "INSERT INTO game_players (game_id, player_id, deck_id, seat, result, mvp_card_name, inserted_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(game_id)
            .bind(player_id)
            .bind(deck_id)
            .bind(seat)
            .bind(result)
            .bind(mvp)
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await
            .unwrap();
        }
        game_id
    }
}

// Games fixtures (players, decks, games, catalog cards), shared by the games, stats, and
// later imports/Discord/webcam ports.

static UNIQUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// `(player_id, deck_id, seat, result, mvp_card_name)` for [`TestApp::sql_game`].
pub type SqlSeat<'a> = (i64, Option<i64>, i64, &'a str, Option<&'a str>);

/// A process-unique number (`System.unique_integer([:positive])`).
pub fn unique() -> u64 {
    UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Builds a `played_at`-style timestamp: `utc("2026-09-19T18:00:00Z")`.
pub fn utc(value: &str) -> db::UtcDateTime {
    db::UtcDateTime::parse(value).expect("timestamp")
}

impl TestApp {
    /// A member with a unique username.
    pub async fn unique_member(&self) -> User {
        self.member(&format!("user{}", unique())).await
    }

    /// An administrator with a unique username.
    pub async fn unique_admin(&self) -> User {
        self.admin(&format!("admin{}", unique())).await
    }

    /// Creates a player named `name`.
    pub async fn player(&self, name: &str) -> the_gathering::games::Player {
        self.player_with(json!({ "name": name }), None).await
    }

    /// Creates a player from `attrs`, optionally linked to an account.
    pub async fn player_with(
        &self,
        attrs: Value,
        user_id: Option<i64>,
    ) -> the_gathering::games::Player {
        self.state
            .games
            .create_player(&input(attrs.clone()), user_id)
            .await
            .unwrap_or_else(|error| panic!("player fixture: {error:?}"))
    }

    /// Creates a deck for a player with a name and commander.
    pub async fn deck(
        &self,
        player_id: i64,
        name: &str,
        commander: &str,
    ) -> the_gathering::games::Deck {
        self.deck_with(json!({ "player_id": player_id, "name": name, "commander_name": commander }))
            .await
    }

    /// Creates a deck from `attrs`.
    pub async fn deck_with(&self, attrs: Value) -> the_gathering::games::Deck {
        self.state
            .games
            .create_deck(&input(attrs.clone()))
            .await
            .unwrap_or_else(|error| panic!("deck fixture: {error:?}"))
    }

    /// Records a game from `attrs`, optionally created by an account.
    pub async fn game(&self, attrs: Value, created_by: Option<i64>) -> the_gathering::games::Game {
        self.state
            .games
            .create_game(&input(attrs.clone()), created_by)
            .await
            .unwrap_or_else(|error| panic!("game fixture: {error:?}"))
    }

    /// A two-seat game the first player won.
    pub async fn simple_game(
        &self,
        played_at: &str,
        winner: i64,
        loser: i64,
        created_by: Option<i64>,
    ) -> the_gathering::games::Game {
        self.game(
            json!({
                "played_at": played_at,
                "seats": [
                    { "player_id": winner, "seat": 1, "result": "win" },
                    { "player_id": loser, "seat": 2, "result": "loss" },
                ],
            }),
            created_by,
        )
        .await
    }

    /// Inserts a catalog card (`%Card{}` with test defaults).
    pub async fn card(
        &self,
        id: &str,
        name: &str,
        colors: &[&str],
        image_uris: Value,
        can_be_commander: bool,
    ) {
        self.card_with(id, name, colors, image_uris, can_be_commander, false)
            .await;
    }

    /// Inserts a catalog card, optionally on the Game Changers list.
    pub async fn card_with(
        &self,
        id: &str,
        name: &str,
        colors: &[&str],
        image_uris: Value,
        can_be_commander: bool,
        game_changer: bool,
    ) {
        let now = db::UtcDateTime::now();
        sqlx::query(
            "INSERT INTO cards (id, oracle_id, name, normalized_name, cmc, type_line, colors, color_identity, image_uris,
                                set_code, collector_number, layout, rarity, commander_legal, can_be_commander,
                                game_changer, inserted_at, updated_at)
             VALUES (?, ?, ?, ?, 0.0, 'Legendary Creature', '[]', ?, ?, 'tst', ?, 'normal', 'rare', 1, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(format!("oracle-{id}"))
        .bind(name)
        .bind(lotus::normalize_name(name))
        .bind(serde_json::to_string(colors).unwrap())
        .bind(image_uris.to_string())
        .bind(id)
        .bind(can_be_commander)
        .bind(game_changer)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await
        .unwrap();
    }

    /// Inserts a cached printing of `oracle-<card>`.
    pub async fn printing(&self, id: &str, card: &str, name: &str, image_uris: Value) {
        sqlx::query(
            "INSERT INTO card_printings (id, oracle_id, name, set_code, set_name, collector_number, image_uris)
             VALUES (?, ?, ?, 'tst', 'Test', ?, ?)",
        )
        .bind(id)
        .bind(format!("oracle-{card}"))
        .bind(name)
        .bind(id)
        .bind(image_uris.to_string())
        .execute(self.pool())
        .await
        .unwrap();
    }

    /// Moves the current session's password authentication `seconds_ago` into the past.
    pub async fn expire_sudo(&self, seconds_ago: i64) {
        let token = self.session().user_token.expect("signed in");
        let at = db::UtcDateTime::now().plus(time::Duration::seconds(-seconds_ago));
        sqlx::query("UPDATE users_tokens SET authenticated_at = ? WHERE token = ?")
            .bind(at)
            .bind(token)
            .execute(self.pool())
            .await
            .unwrap();
    }

    /// Sends a GET with a bearer token and no cookie.
    pub async fn get_bearer(&self, path: &str, token: &str) -> TestResponse {
        self.clear_cookies();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        self.request_with(Method::GET, path, None, headers).await
    }

    /// Updates the server settings.
    pub async fn settings(&self, attrs: Value) {
        self.state
            .accounts
            .update_settings(&input(attrs))
            .await
            .unwrap();
    }
}

/// A file under `tests/fixtures`.
pub fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

/// A fixture's contents.
pub fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixture_path(relative))
        .unwrap_or_else(|error| panic!("fixture {relative}: {error}"))
}

/// A JSON fixture.
pub fn json_fixture(relative: &str) -> Value {
    serde_json::from_slice(&fixture(relative)).expect("JSON fixture")
}

/// Log lines captured by [`capture_logs`].
#[derive(Clone, Default)]
pub struct LogBuffer(std::sync::Arc<Mutex<Vec<u8>>>);

impl LogBuffer {
    /// Forgets what was logged so far.
    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    /// Everything logged so far.
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl std::io::Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

thread_local! {
    static CAPTURE: std::cell::RefCell<Option<LogBuffer>> = const { std::cell::RefCell::new(None) };
}

/// Writes to the current thread's capture buffer, if any.
struct ThreadWriter;

impl std::io::Write for ThreadWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        CAPTURE.with_borrow_mut(|capture| match capture {
            Some(buffer) => buffer.write(bytes),
            None => Ok(bytes.len()),
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for ThreadWriterMaker {
    type Writer = ThreadWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ThreadWriter
    }
}

struct ThreadWriterMaker;

/// Stops capturing when dropped.
pub struct CaptureGuard;

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with_borrow_mut(|capture| *capture = None);
    }
}

/// `ExUnit.CaptureLog` at debug level: captures this thread's logs until the guard drops.
/// `#[tokio::test]` runs on one thread, so requests and spawned tasks log here too.
///
/// One global subscriber writes to a per-thread buffer: a scoped (`set_default`) subscriber
/// would miss callsites whose interest was cached while no subscriber existed.
pub fn capture_logs() -> (CaptureGuard, LogBuffer) {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(ThreadWriterMaker)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("one global subscriber");
    });
    let buffer = LogBuffer::default();
    CAPTURE.with_borrow_mut(|capture| *capture = Some(buffer.clone()));
    (CaptureGuard, buffer)
}

/// A typed request body or domain input built from JSON (panics on a mismatch).
pub fn input<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap_or_else(|error| panic!("input: {error}"))
}
