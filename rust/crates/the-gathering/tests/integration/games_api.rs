//! The games API, the deck chooser, and Game Changer flags in JSON.
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
use the_gathering::accounts::User;
use the_gathering::catalog::{CardRef, Catalog};
use the_gathering::games::{Deck, Player, Seat};
use the_gathering::web::api::games::{deck_summary, seat_json};

struct Ctx {
    app: TestApp,
    user: User,
    alice: Player,
    bob: Player,
    deck: Deck,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    app.log_in(&user).await;
    app.card(
        "kangee",
        "Kangee, Sky Warden",
        &[],
        json!({"art_crop": "https://cards.example/kangee-art.jpg"}),
        true,
    )
    .await;
    app.card(
        "swan-song",
        "Swan Song",
        &[],
        json!({"art_crop": "https://cards.example/swan-song-art.jpg", "normal": "https://cards.example/swan-song.jpg"}),
        false,
    )
    .await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let deck = app.deck(alice.id, "Birds", "Kangee, Sky Warden").await;
    Ctx {
        app,
        user,
        alice,
        bob,
        deck,
    }
}

fn game_attrs(alice: &Player, bob: &Player) -> Value {
    json!({"played_at": "2026-09-20T12:00:00Z", "seats": [
        {"player_id": alice.id, "seat": 1, "result": "win"},
        {"player_id": bob.id, "seat": 2, "result": "loss"},
    ]})
}

#[tokio::test]
async fn accepts_and_exposes_format_for_a_two_winner_game() {
    let ctx = setup().await;
    let cara = ctx.app.player("Cara").await;
    let drew = ctx.app.player("Drew").await;
    let seats: Vec<Value> = [&ctx.alice, &ctx.bob, &cara, &drew]
        .iter()
        .enumerate()
        .map(|(index, player)| json!({"player_id": player.id, "seat": index + 1, "result": if index < 2 { "win" } else { "loss" }}))
        .collect();
    let created = ctx
        .app
        .post("/api/games", json!({"game": {"played_at": "2026-09-19T18:30:00Z", "format": "two_headed_giant", "seats": seats}}))
        .await
        .assert_json(201);
    assert_eq!(created["data"]["format"], "two_headed_giant");
    assert_eq!(
        created["data"]["seats"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|seat| seat["result"] == "win")
            .count(),
        2
    );
}

#[tokio::test]
async fn post_api_games_creates_nested_seats_and_returns_the_documented_shape() {
    let ctx = setup().await;
    let payload = json!({"game": {
        "created_by_user_id": ctx.user.id + 1000,
        "played_at": "2026-09-19T18:30:00Z",
        "duration_minutes": 57,
        "turns": 9,
        "win_condition": "commander_damage",
        "notes": "Close finish",
        "source": "csv",
        "external_id": "forged-import-id",
        "seats": [
            {"player_id": ctx.alice.id, "deck_id": ctx.deck.id, "seat": 1, "result": "win", "kills": 0,
             "mvp_card_id": "swan-song", "mvp_card_name": "Swan Song"},
            {"player_id": ctx.bob.id, "seat": 2, "result": "loss"},
        ],
    }});
    let response = ctx.app.post("/api/games", payload).await.assert_json(201);
    let data = &response["data"];
    assert!(data["id"].is_i64());
    assert_eq!(data["source"], "manual");
    assert_eq!(data["created_by_user_id"], ctx.user.id);
    assert_eq!(data["duration_minutes"], 57);
    assert_eq!(data["win_condition"], "commander_damage");
    assert_eq!(data["external_id"], Value::Null);
    let first = &data["seats"][0];
    assert_eq!(first["seat"], 1);
    assert_eq!(first["result"], "win");
    assert_eq!(first["kills"], 0);
    assert_eq!(first["player"]["id"], ctx.alice.id);
    assert_eq!(first["player"]["name"], "Alice");
    assert_eq!(first["deck"]["id"], ctx.deck.id);
    assert_eq!(first["deck"]["commander_name"], "Kangee, Sky Warden");
    assert_eq!(first["deck"]["player"], Value::Null);
    assert_eq!(first["mvp_card_name"], "Swan Song");
    assert_eq!(
        first["mvp_art_crop_url"],
        "https://cards.example/swan-song-art.jpg"
    );
    assert_eq!(
        first["mvp_image_url"],
        "https://cards.example/swan-song.jpg"
    );
    assert_eq!(
        first["deck"]["commander_art_crop_url"],
        "https://cards.example/kangee-art.jpg"
    );
    let second = &data["seats"][1];
    assert_eq!(second["seat"], 2);
    assert_eq!(second["result"], "loss");
    assert_eq!(second["kills"], Value::Null);
    assert_eq!(second["player"]["name"], "Bob");
    assert_eq!(second["deck"], Value::Null);
}

#[tokio::test]
async fn unrelated_members_cannot_update_or_delete_a_game() {
    let ctx = setup().await;
    let creator = ctx.app.unique_member().await;
    let game = ctx
        .app
        .game(game_attrs(&ctx.alice, &ctx.bob), Some(creator.id))
        .await;
    let path = format!("/api/games/{}", game.id);
    assert_eq!(
        ctx.app
            .patch(&path, json!({"game": {"notes": "tampered"}}))
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
    ctx.app.delete(&path).await.assert_json(403);
    assert!(
        ctx.app
            .state
            .games
            .get_game(game.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn records_ten_participants_through_record_game_and_rejects_eleven() {
    let ctx = setup().await;
    let mut seats = Vec::new();
    for index in 1..=11 {
        let player = ctx.app.player(&format!("Seat {index}")).await;
        seats.push(json!({"player_id": player.id, "seat": index, "result": if index == 10 { "win" } else { "loss" }}));
    }
    let response = ctx
        .app
        .post(
            "/api/games",
            json!({"game": {"played_at": "2026-09-23T18:30:00Z", "seats": seats[..10]}}),
        )
        .await
        .assert_json(201);
    let numbers: Vec<i64> = response["data"]["seats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seat| seat["seat"].as_i64().unwrap())
        .collect();
    assert_eq!(numbers, (1..=10).collect::<Vec<_>>());
    assert_eq!(response["data"]["seats"][9]["result"], "win");

    let response = ctx
        .app
        .post(
            "/api/games",
            json!({"game": {"played_at": "2026-09-23T18:30:00Z", "seats": seats}}),
        )
        .await;
    let body = response.assert_json(422);
    assert!(body["errors"]["seats"].is_array());
    assert_eq!(
        body["errors"]["seats"][10],
        json!({"seat": ["must be less than or equal to 10"]})
    );
}

#[tokio::test]
async fn the_creator_can_update_and_delete_a_game_without_changing_its_provenance() {
    let ctx = setup().await;
    let game = ctx
        .app
        .game(game_attrs(&ctx.alice, &ctx.bob), Some(ctx.user.id))
        .await;
    let response = ctx
        .app
        .patch(
            &format!("/api/games/{}", game.id),
            json!({"game": {"notes": "creator edit", "win_condition": "alternate_win_con", "source": "discord", "external_id": "forged"}}),
        )
        .await
        .assert_json(200);
    assert_eq!(response["data"]["notes"], "creator edit");
    assert_eq!(response["data"]["win_condition"], "alternate_win_con");
    assert_eq!(response["data"]["source"], "manual");
    assert_eq!(response["data"]["external_id"], Value::Null);

    let doomed = ctx
        .app
        .game(game_attrs(&ctx.alice, &ctx.bob), Some(ctx.user.id))
        .await;
    assert_eq!(
        ctx.app
            .delete(&format!("/api/games/{}", doomed.id))
            .await
            .status
            .as_u16(),
        204
    );
    assert!(
        ctx.app
            .state
            .games
            .get_game(doomed.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn editing_keeps_seat_ids_while_swapping_seat_numbers_and_changing_format() {
    let ctx = setup().await;
    let carol = ctx.app.player("Carol").await;
    let dave = ctx.app.player("Dave").await;
    let game = ctx
        .app
        .game(
            json!({"played_at": "2026-09-20T12:00:00Z", "seats": [
                {"player_id": ctx.alice.id, "seat": 1, "result": "win"},
                {"player_id": ctx.bob.id, "seat": 2, "result": "loss"},
                {"player_id": carol.id, "seat": 3, "result": "loss"},
                {"player_id": dave.id, "seat": 4, "result": "loss"},
            ]}),
            Some(ctx.user.id),
        )
        .await;
    let seat_id = |player: &Player| {
        game.seats
            .iter()
            .find(|seat: &&Seat| seat.player_id == player.id)
            .unwrap()
            .id
    };
    let seats = json!([
        {"id": seat_id(&ctx.alice), "player_id": ctx.alice.id, "seat": 1, "result": "win"},
        {"id": seat_id(&carol), "player_id": carol.id, "seat": 2, "result": "win"},
        {"id": seat_id(&ctx.bob), "player_id": ctx.bob.id, "seat": 3, "result": "loss"},
        {"id": seat_id(&dave), "player_id": dave.id, "seat": 4, "result": "loss"},
    ]);
    let path = format!("/api/games/{}", game.id);
    let response = ctx
        .app
        .patch(
            &path,
            json!({"game": {"format": "two_headed_giant", "seats": seats}}),
        )
        .await
        .assert_json(200);
    assert_eq!(response["data"]["format"], "two_headed_giant");
    let rows: Vec<(Value, Value, Value)> = response["data"]["seats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seat| {
            (
                seat["seat"].clone(),
                seat["player"]["name"].clone(),
                seat["id"].clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (json!(1), json!("Alice"), json!(seat_id(&ctx.alice))),
            (json!(2), json!("Carol"), json!(seat_id(&carol))),
            (json!(3), json!("Bob"), json!(seat_id(&ctx.bob))),
            (json!(4), json!("Dave"), json!(seat_id(&dave))),
        ]
    );

    let mut invalid = seats.clone();
    for seat in invalid.as_array_mut().unwrap() {
        seat["result"] = json!("win");
    }
    ctx.app
        .patch(&path, json!({"game": {"seats": invalid}}))
        .await
        .assert_json(422);
    let mut numbers: Vec<i64> = ctx
        .app
        .state
        .games
        .get_game(game.id)
        .await
        .unwrap()
        .unwrap()
        .seats
        .iter()
        .map(|seat| seat.seat)
        .collect();
    numbers.sort_unstable();
    assert_eq!(numbers, [1, 2, 3, 4]);
}

#[tokio::test]
async fn rejects_a_win_condition_outside_the_canonical_enum() {
    let ctx = setup().await;
    let mut attrs = game_attrs(&ctx.alice, &ctx.bob);
    attrs["win_condition"] = json!("combo");
    let response = ctx
        .app
        .post("/api/games", json!({"game": attrs}))
        .await
        .assert_json(422);
    assert_eq!(response["errors"]["win_condition"], json!(["is invalid"]));
}

#[tokio::test]
async fn a_seated_linked_player_can_update_and_delete_a_game() {
    let ctx = setup().await;
    let creator = ctx.app.unique_member().await;
    let alice = ctx
        .app
        .state
        .games
        .link_player_to_user(&ctx.alice, &ctx.user)
        .await
        .unwrap();
    let game = ctx
        .app
        .game(game_attrs(&alice, &ctx.bob), Some(creator.id))
        .await;
    let response = ctx
        .app
        .patch(
            &format!("/api/games/{}", game.id),
            json!({"game": {"notes": "participant edit"}}),
        )
        .await
        .assert_json(200);
    assert_eq!(response["data"]["notes"], "participant edit");
    let doomed = ctx
        .app
        .game(game_attrs(&alice, &ctx.bob), Some(creator.id))
        .await;
    assert_eq!(
        ctx.app
            .delete(&format!("/api/games/{}", doomed.id))
            .await
            .status
            .as_u16(),
        204
    );
    assert!(
        ctx.app
            .state
            .games
            .get_game(doomed.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_administrator_can_update_and_delete_any_game() {
    let ctx = setup().await;
    let creator = ctx.app.unique_member().await;
    let admin = ctx.app.unique_admin().await;
    let game = ctx
        .app
        .game(game_attrs(&ctx.alice, &ctx.bob), Some(creator.id))
        .await;
    ctx.app.log_in(&admin).await;
    let response = ctx
        .app
        .patch(
            &format!("/api/games/{}", game.id),
            json!({"game": {"notes": "admin edit"}}),
        )
        .await
        .assert_json(200);
    assert_eq!(response["data"]["notes"], "admin edit");
    let doomed = ctx
        .app
        .game(game_attrs(&ctx.alice, &ctx.bob), Some(creator.id))
        .await;
    assert_eq!(
        ctx.app
            .delete(&format!("/api/games/{}", doomed.id))
            .await
            .status
            .as_u16(),
        204
    );
    assert!(
        ctx.app
            .state
            .games
            .get_game(doomed.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn post_api_games_requires_a_signed_in_user() {
    let ctx = setup().await;
    ctx.app.clear_cookies();
    let response = ctx
        .app
        .post(
            "/api/games",
            json!({"game": game_attrs(&ctx.alice, &ctx.bob)}),
        )
        .await;
    assert_eq!(
        response.assert_json(401),
        json!({"errors": {"detail": "Unauthorized"}})
    );
    let (games, pagination) = ctx.app.state.games.list_games(&json!({})).await.unwrap();
    assert!(games.is_empty());
    assert_eq!(
        serde_json::to_value(pagination).unwrap(),
        json!({"page": 1, "per_page": 20, "total": 0, "total_pages": 1})
    );
}

#[tokio::test]
async fn get_api_games_id_returns_the_same_nested_resource_shape() {
    let ctx = setup().await;
    let game = ctx
        .app
        .game(
            json!({"played_at": "2026-09-19T18:30:00Z", "seats": [
                {"player_id": ctx.alice.id, "deck_id": ctx.deck.id, "seat": 1, "result": "win"},
                {"player_id": ctx.bob.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let response = ctx
        .app
        .get(&format!("/api/games/{}", game.id))
        .await
        .assert_json(200);
    assert_eq!(response["data"]["id"], game.id);
    let names: Vec<Value> = response["data"]["seats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seat| seat["player"]["name"].clone())
        .collect();
    assert_eq!(names, [json!("Alice"), json!("Bob")]);
    assert_eq!(response["data"]["seats"][0]["deck"]["name"], "Birds");

    let index = ctx
        .app
        .get("/api/games?player_id=abc")
        .await
        .assert_json(200);
    assert_eq!(index["pagination"]["total"], 1);
    assert_eq!(index["data"][0]["id"], game.id);
    ctx.app.get("/api/games/abc").await.assert_json(400);
    ctx.app.get("/api/games/0").await.assert_json(404);
}

#[tokio::test]
async fn game_history_is_private_to_signed_in_users() {
    let app = TestApp::new().await;
    for path in ["/api/games", "/api/players", "/api/decks", "/api/cards?q=a"] {
        assert_eq!(
            app.get(path).await.assert_json(401),
            json!({"errors": {"detail": "Unauthorized"}})
        );
    }
}

#[tokio::test]
async fn the_summary_card_renders_a_png_and_rejects_unknown_references() {
    let ctx = setup().await;
    let game = ctx.app.game(game_attrs(&ctx.alice, &ctx.bob), None).await;
    let response = ctx
        .app
        .get(&format!("/api/games/{}/summary", game.id))
        .await;
    assert_eq!(response.status.as_u16(), 200);
    assert_eq!(response.header("content-type"), Some("image/png"));
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    assert!(response.body.starts_with(&[0x89, b'P', b'N', b'G']));
    ctx.app
        .get("/api/games/SB404/summary")
        .await
        .assert_json(404);
    ctx.app
        .get("/api/games/nope!/summary")
        .await
        .assert_json(400);
    ctx.app.get("/api/games/0/summary").await.assert_json(404);
}

#[tokio::test]
async fn summary_cards_find_spellbot_games_and_the_latest_game() {
    let ctx = setup().await;
    let games = &ctx.app.state.games;
    let spellbot = games
        .upsert_game_by_external_id(
            "discord",
            "spellbot:SB42",
            &game_attrs(&ctx.alice, &ctx.bob),
        )
        .await
        .unwrap();
    let mut later = game_attrs(&ctx.alice, &ctx.bob);
    later["played_at"] = json!("2026-09-21T12:00:00Z");
    let latest = ctx.app.game(later, None).await;
    assert_eq!(
        games.find_summary_game("#sb42").await.unwrap().id,
        spellbot.id
    );
    assert_eq!(games.find_summary_game(" ").await.unwrap().id, latest.id);
    assert_eq!(
        games
            .find_summary_game(&spellbot.id.to_string())
            .await
            .unwrap()
            .id,
        spellbot.id
    );
}

#[tokio::test]
async fn summary_images_are_limited_to_thirty_a_minute() {
    let ctx = setup().await;
    let game = ctx.app.game(game_attrs(&ctx.alice, &ctx.bob), None).await;
    for _ in 0..30 {
        ctx.app
            .state
            .rate_limiter
            .hit("summary_images", the_gathering::games::SUMMARY_IMAGES_LIMIT);
    }
    let response = ctx
        .app
        .get(&format!("/api/games/{}/summary", game.id))
        .await;
    assert_eq!(
        response.assert_json(502),
        json!({"errors": {"detail": "Bad Gateway"}})
    );
}

// DeckChooserControllerTest

async fn chooser() -> (TestApp, User, Player) {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let player = app
        .player_with(json!({"name": "Chooser"}), Some(user.id))
        .await;
    app.log_in(&user).await;
    (app, user, player)
}

#[tokio::test]
async fn records_skip_and_choose_outcomes_for_the_signed_in_players_deck() {
    let (app, _user, player) = chooser().await;
    let deck = app.deck(player.id, "Krenko", "Krenko").await;
    let path = format!("/api/deck-chooser/{}/outcomes", deck.id);
    assert_eq!(
        app.post(&path, json!({"outcome": "skipped"}))
            .await
            .assert_json(200),
        json!({"data": {"deck_id": deck.id, "outcome": "skipped", "skip_count": 1}})
    );
    assert_eq!(
        app.post(&path, json!({"outcome": "played"}))
            .await
            .assert_json(200)["data"]["skip_count"],
        0
    );
    app.post(&path, json!({"outcome": "maybe"}))
        .await
        .assert_json(400);
    app.post("/api/deck-chooser/0/outcomes", json!({"outcome": "played"}))
        .await
        .assert_json(404);

    let pick = app.get("/api/deck-chooser").await.assert_json(200);
    assert_eq!(pick["data"]["deck"]["id"], deck.id);
    assert_eq!(pick["data"]["play_count"], 0);
    assert_eq!(pick["data"]["skip_count"], 0);
    assert_eq!(pick["data"]["last_played_at"], Value::Null);
    assert_eq!(pick["data"]["reason"], Value::Null);
}

#[tokio::test]
async fn explains_when_the_user_has_no_linked_player() {
    let (app, _user, _player) = chooser().await;
    let unlinked = app.unique_member().await;
    app.log_in(&unlinked).await;
    assert_eq!(
        app.get("/api/deck-chooser").await.assert_json(200),
        json!({"data": {"deck": null, "reason": "player_not_linked"}})
    );
}

#[tokio::test]
async fn explains_when_no_deck_is_eligible() {
    let (app, _user, _player) = chooser().await;
    assert_eq!(
        app.get("/api/deck-chooser").await.assert_json(200),
        json!({"data": {"deck": null, "reason": "no_eligible_decks"}})
    );
}

// GameChangerJSONTest

#[tokio::test]
async fn batched_summaries_label_commanders_partners_and_mvps_using_current_catalog_identity() {
    let app = TestApp::new().await;
    app.card_with(
        "thrasios",
        "Thrasios, Triton Hero",
        &[],
        json!({}),
        true,
        true,
    )
    .await;
    app.card_with("kraum", "Kraum", &[], json!({}), true, false)
        .await;
    let catalog = Catalog {
        pool: app.pool().clone(),
    };
    let refs = vec![
        (
            Some("thrasios".to_owned()),
            Some("Thrasios, Triton Hero".to_owned()),
        ),
        (Some("kraum".to_owned()), Some("Kraum".to_owned())),
    ];
    let summaries = catalog.card_summaries(&refs).await.unwrap();
    assert!(summaries.get(Some("thrasios"), None).unwrap().game_changer);
    let art = catalog
        .art_crop_urls(
            &refs
                .iter()
                .map(|(id, name)| CardRef::Card(id.clone(), name.clone()))
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    assert!(art.game_changer(Some("obsolete"), Some("Thrasios, Triton Hero")));
    assert!(!art.game_changer(Some("kraum"), Some("Thrasios, Triton Hero")));
    assert!(!art.game_changer(None, Some("Missing")));

    let deck = Deck {
        commander_card_id: Some("kraum".into()),
        commander_name: "Kraum".into(),
        partner_card_id: Some("obsolete".into()),
        partner_name: Some("Thrasios, Triton Hero".into()),
        ..Deck::default()
    };
    let data = deck_summary(&deck, None, &art);
    assert_eq!(data["commander_game_changer"], false);
    assert_eq!(data["partner_game_changer"], true);
    let seat = Seat {
        mvp_card_name: Some("Thrasios, Triton Hero".into()),
        ..Seat::default()
    };
    assert_eq!(seat_json(&seat, &art)["mvp_game_changer"], true);
}
