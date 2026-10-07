//! Test harness: a fresh migrated SQLite database per test, the real router, a cookie jar,
//! and automatic CSRF tokens (Phoenix's `ConnTest` skipped CSRF; this sends valid tokens).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

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
use the_gathering::web::session::{COOKIE, Session};
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
            panic!("invalid JSON ({error}): {}", String::from_utf8_lossy(&self.body))
        })
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Asserts the status and returns the JSON body.
    #[track_caller]
    pub fn assert_json(&self, status: u16) -> Value {
        assert_eq!(self.status.as_u16(), status, "unexpected status; body: {}", self.text());
        self.json()
    }

    /// A header value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The redirect target of a 302.
    #[track_caller]
    pub fn redirected_to(&self) -> String {
        assert_eq!(self.status, StatusCode::FOUND, "not a redirect: {}", self.text());
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
        let pool = db::connect(&config.database_path, 5).await.expect("database");
        db::migrate::run(&pool).await.expect("migrations");
        let state = AppState::new(config, pool).expect("state");
        let router = web::router(state.clone());
        Self { state, router, cookie: Mutex::new(None), _dir: dir }
    }

    /// The database pool.
    pub fn pool(&self) -> &db::Pool {
        &self.state.pool
    }

    /// The current session (decoded from the jar).
    pub fn session(&self) -> Session {
        let cookie = self.cookie.lock().unwrap().clone().unwrap_or_default();
        Session::from_cookie(&cookie, &self.state.config.secret_key_base)
    }

    fn store_session(&self, session: &Session) {
        *self.cookie.lock().unwrap() = Some(session.to_cookie(&self.state.config.secret_key_base));
    }

    /// Forgets the cookie (a new browser).
    pub fn clear_cookies(&self) {
        *self.cookie.lock().unwrap() = None;
    }

    /// Sends a request; mutating requests carry a valid CSRF token.
    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> TestResponse {
        self.request_with(method, path, body, HeaderMap::new()).await
    }

    /// Sends a request with extra headers.
    pub async fn request_with(&self, method: Method, path: &str, body: Option<Value>, headers: HeaderMap) -> TestResponse {
        let mut builder = Request::builder().method(method.clone()).uri(path);
        if !matches!(method, Method::GET | Method::HEAD) && !headers.contains_key("x-csrf-token") {
            let session = self.session();
            let token = session.csrf_token();
            self.store_session(&session);
            builder = builder.header("x-csrf-token", token);
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
        TestResponse { status, headers, body }
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

    /// `ConnCase.log_in_user/2`: a tracked session token in a fresh session.
    pub async fn log_in(&self, user: &User) {
        let token = self.state.accounts.generate_user_session_token(user).await.unwrap();
        let session = Session::default();
        session.put_bytes("user_token", &token);
        self.store_session(&session);
    }

    /// Logs in with a password authentication just now (sudo mode).
    pub async fn log_in_sudo(&self, user: &User) {
        let user = User { authenticated_at: Some(db::UtcDateTime::now()), ..user.clone() };
        self.log_in(&user).await;
    }

    /// `AccountsFixtures.user_fixture/1`.
    pub async fn user(&self, username: &str, role: &str) -> User {
        self.state
            .accounts
            .create_user(&json!({
                "username": username,
                "display_name": "Test User",
                "password": PASSWORD,
                "role": role,
            }))
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
}

// Games fixtures (players, decks, games, catalog cards), shared by the games, stats, and
// later imports/Discord/webcam ports.

static UNIQUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A process-unique number (`System.unique_integer([:positive])`).
pub fn unique() -> u64 {
    UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Builds a `played_at`-style timestamp: `utc("2026-09-19T18:00:00Z")`.
pub fn utc(value: &str) -> db::UtcDateTime {
    db::UtcDateTime::parse(value).expect("timestamp")
}

impl TestApp {
    /// `AccountsFixtures.user_fixture/0`: a member with a unique username.
    pub async fn unique_member(&self) -> User {
        self.member(&format!("user{}", unique())).await
    }

    /// `AccountsFixtures.admin_fixture/0`: an administrator with a unique username.
    pub async fn unique_admin(&self) -> User {
        self.admin(&format!("admin{}", unique())).await
    }

    /// `Games.create_player(%{name: name})`.
    pub async fn player(&self, name: &str) -> the_gathering::games::Player {
        self.player_with(json!({ "name": name }), None).await
    }

    /// `Games.create_player(attrs, user_id)`.
    pub async fn player_with(&self, attrs: Value, user_id: Option<i64>) -> the_gathering::games::Player {
        self.state
            .games
            .create_player(&attrs, user_id)
            .await
            .unwrap_or_else(|error| panic!("player fixture: {error:?}"))
    }

    /// `Games.create_deck(%{player_id, name, commander_name})`.
    pub async fn deck(&self, player_id: i64, name: &str, commander: &str) -> the_gathering::games::Deck {
        self.deck_with(json!({ "player_id": player_id, "name": name, "commander_name": commander })).await
    }

    /// `Games.create_deck(attrs)`.
    pub async fn deck_with(&self, attrs: Value) -> the_gathering::games::Deck {
        self.state.games.create_deck(&attrs).await.unwrap_or_else(|error| panic!("deck fixture: {error:?}"))
    }

    /// `Games.create_game(attrs, created_by_user_id)`.
    pub async fn game(&self, attrs: Value, created_by: Option<i64>) -> the_gathering::games::Game {
        self.state
            .games
            .create_game(&attrs, created_by)
            .await
            .unwrap_or_else(|error| panic!("game fixture: {error:?}"))
    }

    /// A two-seat game the first player won.
    pub async fn simple_game(&self, played_at: &str, winner: i64, loser: i64, created_by: Option<i64>) -> the_gathering::games::Game {
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
    pub async fn card(&self, id: &str, name: &str, colors: &[&str], image_uris: Value, can_be_commander: bool) {
        self.card_with(id, name, colors, image_uris, can_be_commander, false).await;
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
        let token = self.session().get_bytes("user_token").expect("signed in");
        let at = db::UtcDateTime::now().add(time::Duration::seconds(-seconds_ago));
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
        headers.insert(header::AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        self.request_with(Method::GET, path, None, headers).await
    }

    /// Updates the server settings (`Accounts.update_settings/1`).
    pub async fn settings(&self, attrs: Value) {
        self.state.accounts.update_settings(&attrs).await.unwrap();
    }
}
