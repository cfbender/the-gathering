//! Test harness: a fresh migrated SQLite database per test, the real router, a cookie jar,
//! and automatic CSRF tokens (Phoenix's ConnTest skipped CSRF; this sends valid tokens).
#![allow(dead_code)]

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

    /// Inserts a catalog card from Scryfall-shaped JSON merged over the defaults the
    /// Elixir tests used (`insert_card!/1`): English paper, `tst` set, common, Commander
    /// legal, released 2024-01-01. `id`, `oracle_id`, and `name` are required.
    pub async fn card(&self, overrides: Value) -> the_gathering::catalog::card_data::CardData {
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
    pub async fn player(&self, name: &str) -> i64 {
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
    pub async fn deck(
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
    pub async fn game(&self, seats: &[(i64, Option<i64>, i64, &str, Option<&str>)]) -> i64 {
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

/// A file under the repository's `test/support/fixtures`.
pub fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test/support/fixtures")
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

