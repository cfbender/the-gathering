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

use crate::support;

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

#[tokio::test]
async fn salt_stats_rank_players_and_commanders_by_satisfaction() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    app.log_in(&user).await;
    app.card("kangee", "Kangee, Sky Warden", &["W", "U"], json!({}), true)
        .await;
    app.card(
        "atraxa",
        "Atraxa, Praetors' Voice",
        &["W", "U", "B", "G"],
        json!({}),
        false,
    )
    .await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let cara = app.player("Cara").await;
    let kangee = app
        .deck_with(json!({"player_id": alice.id, "name": "Birds", "commander_card_id": "kangee", "commander_name": "Kangee, Sky Warden"}))
        .await;
    let atraxa = app
        .deck_with(json!({"player_id": bob.id, "name": "Counters", "commander_card_id": "atraxa", "commander_name": "Atraxa, Praetors' Voice"}))
        .await;
    let seat = |player: i64, deck: Option<i64>, number: i64, result: &str, rating: Option<i64>| json!({"player_id": player, "deck_id": deck, "seat": number, "result": result, "satisfaction": rating});
    let mut games = Vec::new();
    for (played_at, seats) in [
        (
            "2026-09-19T18:00:00Z",
            vec![
                seat(alice.id, Some(kangee.id), 1, "win", Some(5)),
                seat(bob.id, Some(atraxa.id), 2, "loss", Some(2)),
                seat(cara.id, None, 3, "loss", Some(1)),
            ],
        ),
        (
            "2026-10-03T18:00:00Z",
            vec![
                seat(alice.id, Some(kangee.id), 1, "win", Some(4)),
                seat(cara.id, None, 2, "loss", Some(4)),
            ],
        ),
        (
            "2026-10-10T18:00:00Z",
            vec![
                seat(alice.id, Some(kangee.id), 1, "loss", None),
                seat(bob.id, Some(atraxa.id), 2, "win", None),
            ],
        ),
    ] {
        games.push(
            app.game(json!({"played_at": played_at, "seats": seats}), None)
                .await,
        );
    }

    let salt = app.get("/api/stats/salt").await.assert_json(200)["data"].clone();
    assert_eq!(salt["games_count"], 3);
    assert_eq!(salt["rated_games"], 2);
    assert_eq!(
        salt["saltiest_game"],
        json!({"id": games[0].id, "played_at": "2026-09-19T18:00:00Z", "average": 2.7, "ratings": 3})
    );
    assert_eq!(
        salt["summary"],
        json!({"ratings": 5, "average": 3.2, "counts": [1, 1, 0, 2, 1]})
    );
    let months: Vec<(Value, Value)> = salt["by_month"]
        .as_array()
        .unwrap()
        .iter()
        .map(|month| (month["month"].clone(), month["average"].clone()))
        .collect();
    assert_eq!(
        months,
        [
            (json!("2026-09"), json!(2.7)),
            (json!("2026-10"), json!(4.0))
        ]
    );
    // Saltiest player first: their own average rating.
    let players: Vec<(Value, Value, Value)> = salt["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["name"].clone(),
                row["average"].clone(),
                row["opponent_average"].clone(),
            )
        })
        .collect();
    assert_eq!(
        players,
        [
            (json!("Bob"), json!(2.0), json!(3.0)),
            (json!("Cara"), json!(2.5), json!(3.7)),
            (json!("Alice"), json!(4.5), json!(2.3)),
        ]
    );
    // Saltiest commander first: how the rest of the table rated games against it.
    let commanders: Vec<(Value, Value, Value, Value)> = salt["commanders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["id"].clone(),
                row["games"].clone(),
                row["average"].clone(),
                row["opponent_average"].clone(),
            )
        })
        .collect();
    assert_eq!(
        commanders,
        [
            (json!("kangee"), json!(3), json!(4.5), json!(2.3)),
            (json!("atraxa"), json!(2), json!(2.0), json!(3.0)),
        ]
    );

    let player = app
        .get(&format!("/api/stats/players/{}", alice.id))
        .await
        .assert_json(200);
    assert_eq!(
        player["data"]["satisfaction"],
        json!({"ratings": 2, "average": 4.5, "counts": [0, 0, 0, 1, 1],
               "opponents": {"ratings": 3, "average": 2.3, "counts": [1, 1, 0, 1, 0]}})
    );
    let deck = app
        .get(&format!("/api/stats/decks/{}", atraxa.id))
        .await
        .assert_json(200);
    assert_eq!(deck["data"]["satisfaction"]["average"], 2.0);
    assert_eq!(deck["data"]["satisfaction"]["opponents"]["average"], 3.0);

    // The commander pages, overview, and recent game lists carry the same figures.
    let commander = app
        .get("/api/stats/commanders/kangee")
        .await
        .assert_json(200);
    assert_eq!(commander["data"]["satisfaction"]["average"], 4.5);
    assert_eq!(
        commander["data"]["satisfaction"]["opponents"]["average"],
        2.3
    );
    let listed = app.get("/api/stats/commanders").await.assert_json(200);
    let atraxa_row = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "atraxa")
        .unwrap()
        .clone();
    assert_eq!(atraxa_row["opponent_average"], 3.0);
    let overview = app.get("/api/stats/overview").await.assert_json(200);
    assert_eq!(overview["data"]["satisfaction"]["average"], 3.2);
    let averages: Vec<Value> = overview["data"]["recent_games"]
        .as_array()
        .unwrap()
        .iter()
        .map(|game| game["average_satisfaction"].clone())
        .collect();
    assert_eq!(averages, [Value::Null, json!(4.0), json!(2.7)]);
    let profile = app
        .get(&format!("/api/players/{}", alice.id))
        .await
        .assert_json(200);
    let ratings: Vec<Value> = profile["data"]["recent_games"]
        .as_array()
        .unwrap()
        .iter()
        .map(|game| game["satisfaction"].clone())
        .collect();
    assert_eq!(ratings, [Value::Null, json!(4), json!(5)]);
}
