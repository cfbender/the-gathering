//! Statistics routes (the views are covered by `tests/stats.rs`): `{"data": stats}`, id
//! casting, and 404s.
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

use serde_json::{Value, json};
use support::TestApp;

#[tokio::test]
async fn stats_routes_wrap_the_views_and_404_unknown_records() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    app.log_in(&user).await;
    app.card("kangee", "Kangee, Sky Warden", &["W", "U"], json!({}), true)
        .await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let deck = app
        .deck_with(json!({"player_id": alice.id, "name": "Birds", "commander_card_id": "kangee", "commander_name": "Kangee, Sky Warden"}))
        .await;
    app.game(
        json!({"played_at": "2026-09-19T18:00:00Z", "duration_minutes": 60, "seats": [
            {"player_id": alice.id, "deck_id": deck.id, "seat": 1, "result": "win"},
            {"player_id": bob.id, "seat": 2, "result": "loss"},
        ]}),
        None,
    )
    .await;

    let overview = app
        .get("/api/stats/overview?tz=America/New_York")
        .await
        .assert_json(200);
    assert_eq!(overview["data"]["games_count"], 1);
    assert_eq!(overview["data"]["average_duration_minutes"], 60.0);
    assert_eq!(overview["data"]["average_turns"], Value::Null);
    assert_eq!(overview["data"]["detailed_stats_from"], Value::Null);

    let player = app
        .get(&format!("/api/stats/players/{}", alice.id))
        .await
        .assert_json(200);
    assert_eq!(player["data"]["record"]["wins"], 1);
    assert_eq!(player["data"]["decks"][0]["color_identity"], "WU");
    app.get("/api/stats/players/abc").await.assert_json(400);
    app.get("/api/stats/players/0").await.assert_json(404);

    let deck_stats = app
        .get(&format!("/api/stats/decks/{}", deck.id))
        .await
        .assert_json(200);
    assert_eq!(deck_stats["data"]["deck"]["name"], "Birds");
    app.get("/api/stats/decks/0").await.assert_json(404);

    let commanders = app.get("/api/stats/commanders").await.assert_json(200);
    assert_eq!(commanders["data"][0]["id"], "kangee");
    assert_eq!(commanders["data"][0]["color_identity"], "WU");
    let commander = app
        .get("/api/stats/commanders/Kangee%2C%20Sky%20Warden")
        .await
        .assert_json(200);
    assert_eq!(commander["data"]["commander"]["id"], "kangee");
    app.get("/api/stats/commanders/nobody")
        .await
        .assert_json(404);

    app.clear_cookies();
    app.get("/api/stats/overview").await.assert_json(401);
}
