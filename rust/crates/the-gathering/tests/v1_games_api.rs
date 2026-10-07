//! Ported from `test/the_gathering_web/controllers/api/v1/game_controller_test.exs`.

// Test crates: helpers outside `#[test]` functions may unwrap and index freely, like the
// tests themselves (clippy.toml only exempts `#[test]` bodies).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty
)]

mod support;

use serde_json::json;
use support::TestApp;
use the_gathering::accounts::User;
use the_gathering::games::{Game, Player};

struct Ctx {
    app: TestApp,
    user: User,
    token: String,
    api_key_id: i64,
    me: Player,
    alice: Player,
    early: Game,
    late: Game,
    others: Game,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let (token, key) = app
        .state
        .accounts
        .create_api_key(user.id, &json!({"name": "script"}))
        .await
        .unwrap();
    let me = app.player_with(json!({"name": "Me"}), Some(user.id)).await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let early = app
        .simple_game("2026-09-01T18:00:00Z", me.id, alice.id, None)
        .await;
    let late = app
        .simple_game("2026-09-20T18:00:00Z", me.id, bob.id, None)
        .await;
    let others = app
        .simple_game("2026-09-21T03:00:00Z", alice.id, bob.id, None)
        .await;
    Ctx {
        app,
        user,
        token,
        api_key_id: key.id,
        me,
        alice,
        early,
        late,
        others,
    }
}

async fn ids(ctx: &Ctx, query: &str) -> Vec<i64> {
    let body = ctx
        .app
        .get_bearer(&format!("/api/v1/games{query}"), &ctx.token)
        .await
        .assert_json(200);
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|game| game["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn lists_every_game_the_owner_can_see_newest_first_with_pagination() {
    let ctx = setup().await;
    let response = ctx.app.get_bearer("/api/v1/games", &ctx.token).await;
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    let body = response.assert_json(200);
    let ids: Vec<i64> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|game| game["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [ctx.others.id, ctx.late.id, ctx.early.id]);
    assert_eq!(
        body["pagination"],
        json!({"page": 1, "per_page": 20, "total": 3, "total_pages": 1})
    );
}

#[tokio::test]
async fn filters_by_the_owners_linked_player_with_player_id_me() {
    let ctx = setup().await;
    assert_eq!(
        ids(&ctx, "?player_id=me").await,
        [ctx.late.id, ctx.early.id]
    );
    let _ = &ctx.me;
}

#[tokio::test]
async fn filters_by_player_id_and_inclusive_local_dates() {
    let ctx = setup().await;
    assert_eq!(
        ids(&ctx, &format!("?player_id={}", ctx.alice.id)).await,
        [ctx.others.id, ctx.early.id]
    );
    assert_eq!(
        ids(&ctx, "?date_from=2026-09-02&date_to=2026-09-20").await,
        [ctx.late.id]
    );
    assert_eq!(
        ids(
            &ctx,
            "?date_from=2026-09-20&date_to=2026-09-20&tz=America/New_York"
        )
        .await,
        [ctx.others.id, ctx.late.id]
    );
}

#[tokio::test]
async fn rejects_invalid_filters_instead_of_widening_the_result() {
    let ctx = setup().await;
    for query in [
        "player_id=abc",
        "player_id=0",
        "date_from=yesterday",
        "date_to=2026-13-01",
        "tz=Mars/Olympus",
        "per_page=-1",
    ] {
        ctx.app
            .get_bearer(&format!("/api/v1/games?{query}"), &ctx.token)
            .await
            .assert_json(400);
    }
}

#[tokio::test]
async fn player_id_me_is_not_found_when_the_owner_has_no_linked_player() {
    let ctx = setup().await;
    let unlinked = ctx.app.unique_member().await;
    let (token, _key) = ctx
        .app
        .state
        .accounts
        .create_api_key(unlinked.id, &json!({"name": "unlinked"}))
        .await
        .unwrap();
    ctx.app
        .get_bearer("/api/v1/games?player_id=me", &token)
        .await
        .assert_json(404);
}

#[tokio::test]
async fn records_when_a_key_was_last_used() {
    let ctx = setup().await;
    ctx.app
        .get_bearer("/api/v1/games", &ctx.token)
        .await
        .assert_json(200);
    let keys = ctx
        .app
        .state
        .accounts
        .list_api_keys(ctx.user.id)
        .await
        .unwrap();
    assert!(
        keys.iter()
            .find(|key| key.id == ctx.api_key_id)
            .unwrap()
            .last_used_at
            .is_some()
    );
}

#[tokio::test]
async fn rejects_missing_malformed_unknown_and_revoked_keys() {
    let ctx = setup().await;
    ctx.app.clear_cookies();
    assert_eq!(
        ctx.app.get("/api/v1/games").await.assert_json(401),
        json!({"errors": {"detail": "Unauthorized"}})
    );
    ctx.app
        .get_bearer("/api/v1/games", "tg_not-a-real-key")
        .await
        .assert_json(401);
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Basic {}", ctx.token).parse().unwrap(),
    );
    ctx.app
        .request_with(axum::http::Method::GET, "/api/v1/games", None, headers)
        .await
        .assert_json(401);
    ctx.app
        .state
        .accounts
        .delete_api_key(ctx.user.id, ctx.api_key_id)
        .await
        .unwrap();
    ctx.app
        .get_bearer("/api/v1/games", &ctx.token)
        .await
        .assert_json(401);
}

#[tokio::test]
async fn keys_stop_working_as_soon_as_the_owner_is_disabled() {
    let ctx = setup().await;
    ctx.app
        .state
        .accounts
        .disable_user(&ctx.user)
        .await
        .unwrap();
    ctx.app
        .get_bearer("/api/v1/games", &ctx.token)
        .await
        .assert_json(401);
}

#[tokio::test]
async fn keys_do_not_authenticate_session_only_api_routes() {
    let ctx = setup().await;
    ctx.app
        .get_bearer("/api/games", &ctx.token)
        .await
        .assert_json(401);
    ctx.app
        .get_bearer("/api/session/api-keys", &ctx.token)
        .await
        .assert_json(401);
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", ctx.token).parse().unwrap(),
    );
    let body = json!({"game": {"played_at": "2026-09-20T18:00:00Z", "seats": [
        {"player_id": ctx.me.id, "seat": 1, "result": "win"},
        {"player_id": ctx.alice.id, "seat": 2, "result": "loss"},
    ]}});
    ctx.app
        .request_with(axum::http::Method::POST, "/api/games", Some(body), headers)
        .await
        .assert_json(401);
}
