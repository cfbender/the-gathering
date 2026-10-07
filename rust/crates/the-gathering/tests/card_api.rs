//! Ported from `test/the_gathering_web/controllers/api/card_controller_test.exs`,
//! `card_printing_controller_test.exs`, `card_rulings_controller_test.exs`,
//! and `card_image_controller_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use std::time::Duration;

use axum::http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};
use support::{TestApp, fixture_path};
use the_gathering::catalog::image_cache::{CacheStatus, CardImages, ImageError};
use the_gathering::catalog::sync::{self, Source};
use the_gathering::catalog::{CardRef, Catalog, images};
use the_gathering::config::WindowLimit;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAGA: &str = "00000000-0000-0000-0000-000000000001";
const MDFC: &str = "00000000-0000-0000-0000-000000000002";
const MISSING: &str = "00000000-0000-0000-0000-000000000003";
const DOWN: &str = "00000000-0000-0000-0000-000000000004";

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

async fn logged_in(app: &TestApp) {
    let user = app.member("member").await;
    app.log_in(&user).await;
}

async fn sync_fixture(app: &TestApp) {
    let source = Source::File(fixture_path("scryfall_catalog.jsonl"));
    assert_eq!(
        sync::run(app.pool(), &app.state.scryfall, source).await,
        Ok(2)
    );
}

// ---- card_controller_test.exs ----

async fn card_app() -> TestApp {
    let app = TestApp::new().await;
    logged_in(&app).await;
    sync_fixture(&app).await;
    app
}

#[tokio::test]
async fn get_cards_searches_the_local_catalog() {
    let app = card_app().await;
    let body = app
        .get("/api/cards?q=Jotun&limit=20")
        .await
        .assert_json(200);
    let cards = body["data"].as_array().unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["id"], "jotun");
    assert_eq!(cards[0]["name"], "Jötun Grunt");
    assert_eq!(cards[0]["image_uris"], json!({}));
    assert_eq!(cards[0]["game_changer"], false);
    assert_eq!(cards[0]["can_be_commander"], false);
}

#[tokio::test]
async fn summaries_and_details_expose_game_changers() {
    let app = card_app().await;
    app.catalog_card(json!({"id": "rhystic", "oracle_id": "oracle-rhystic", "name": "Rhystic Study", "game_changer": true})).await;
    let body = app.get("/api/cards?q=Rhystic").await.assert_json(200);
    assert_eq!(body["data"][0]["game_changer"], true);
    let body = app.get("/api/cards/rhystic").await.assert_json(200);
    assert_eq!(body["data"]["game_changer"], true);
}

#[tokio::test]
async fn partner_mode_includes_backgrounds_without_loosening_commander_mode() {
    let app = card_app().await;
    app.catalog_card(json!({
        "id": "background", "oracle_id": "oracle-background", "name": "Master Chef",
        "type_line": "Legendary Enchantment — Background",
        "oracle_text": "Commander creatures you own have base power and toughness 3/3.",
        "color_identity": ["G"]
    }))
    .await;
    let body = app
        .get("/api/cards?q=Master%20Chef&partner=true")
        .await
        .assert_json(200);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "background");
    assert_eq!(body["data"][0]["commander_pairing"], "background");
    let body = app
        .get("/api/cards?q=Master%20Chef&commander=true")
        .await
        .assert_json(200);
    assert_eq!(body["data"], json!([]));
}

#[tokio::test]
async fn partner_mode_allows_any_legendary_creature_for_rule_0_pairings() {
    let app = card_app().await;
    app.catalog_card(json!({
        "id": "clara", "oracle_id": "oracle-clara", "name": "Clara Oswald",
        "type_line": "Legendary Creature — Human Advisor",
        "oracle_text": "If a triggered ability of a Doctor you control triggers, that ability triggers an additional time.\nDoctor's companion (You can have two commanders if the other is the Doctor.)"
    }))
    .await;
    app.catalog_card(json!({
        "id": "krenko", "oracle_id": "oracle-krenko", "name": "Krenko, Mob Boss",
        "type_line": "Legendary Creature — Goblin Warrior",
        "oracle_text": "{T}: Create X 1/1 red Goblin creature tokens."
    }))
    .await;
    let ids = |body: Value| {
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|card| card["id"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(app
            .get("/api/cards?q=Clara&partner=true")
            .await
            .assert_json(200)),
        [json!("clara")]
    );
    assert_eq!(
        ids(app
            .get("/api/cards?q=Krenko&partner=true")
            .await
            .assert_json(200)),
        [json!("krenko")]
    );
    let jotun = ids(app
        .get("/api/cards?q=Jotun&partner=true")
        .await
        .assert_json(200));
    assert_eq!(jotun, Vec::<Value>::new());
}

#[tokio::test]
async fn get_card_returns_detail_and_404s_missing_cards() {
    let app = card_app().await;
    let body = app.get("/api/cards/printing-latest").await.assert_json(200);
    assert_eq!(body["data"]["id"], "printing-latest");
    assert_eq!(body["data"]["set_code"], "new");
    assert_eq!(body["data"]["released_at"], "2025-06-01");
    assert_eq!(
        body["data"]["oracle_text"],
        "Catalog Bolt deals 3 damage to any target."
    );
    assert_eq!(body["data"]["cmc"], 1.0);
    assert_eq!(body["data"]["colors"], json!(["R"]));
    let body = app.get("/api/cards/missing").await.assert_json(404);
    assert_eq!(body, json!({"errors": {"detail": "Not Found"}}));
}

#[tokio::test]
async fn get_catalog_returns_sync_status() {
    let app = card_app().await;
    let body = app.get("/api/catalog").await.assert_json(200);
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["card_count"], 2);
    assert_eq!(body["data"]["last_error"], Value::Null);
    assert!(
        body["data"]["last_started_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
}

#[tokio::test]
async fn catalog_admin_triggers_report_their_outcome() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bulk-data"))
        .respond_with(ResponseTemplate::new(503).set_delay(Duration::from_millis(500)))
        .expect(1)
        .mount(&server)
        .await;
    let uri = server.uri();
    let app = TestApp::with_config(|config| config.scryfall_api_base = uri).await;
    let admin = app.admin("admin").await;
    app.log_in_sudo(&admin).await;
    let body = app
        .post("/api/admin/catalog/backfill", json!({}))
        .await
        .assert_json(200);
    assert_eq!(
        body,
        json!({"data": {"decks_split": 0, "decks_linked": 0, "colors_filled": 0, "mvps_linked": 0, "unmatched": []}})
    );

    // The sync downloads from Scryfall in the background; a second trigger while it runs
    // reports that it is already running, and the failed run is recorded.
    let mut finished = app.state.catalog_sync.subscribe();
    let first = app
        .post("/api/admin/catalog/sync", json!({}))
        .await
        .assert_json(202);
    assert_eq!(first, json!({"data": {"status": "started"}}));
    let second = app
        .post("/api/admin/catalog/sync", json!({}))
        .await
        .assert_json(202);
    assert_eq!(second, json!({"data": {"status": "already_running"}}));
    finished.changed().await.unwrap();
    assert!(!app.state.catalog_sync.running());
    let status = app.get("/api/catalog").await.assert_json(200);
    assert_eq!(status["data"]["status"], "failed");
}

// ---- card_printing_controller_test.exs ----

fn scryfall_card(id: &str, name: &str) -> Value {
    json!({
        "id": id,
        "oracle_id": format!("oracle-{id}"),
        "name": name,
        "type_line": "Legendary Creature",
        "games": ["paper"],
        "set": "new",
        "set_name": "New Set",
        "collector_number": "9",
        "lang": "en",
        "image_uris": {
            "art_crop": format!("https://img.example/{id}-default.jpg"),
            "normal": format!("https://img.example/{id}-default-card.jpg")
        }
    })
}

fn merge(mut base: Value, overrides: &Value) -> Value {
    for (key, value) in overrides.as_object().unwrap() {
        base[key] = value.clone();
    }
    base
}

async fn printing_app(server: &MockServer) -> TestApp {
    printing_app_with(server, |_| {}).await
}

async fn printing_app_with(
    server: &MockServer,
    adjust: impl FnOnce(&mut the_gathering::config::Config),
) -> TestApp {
    let uri = server.uri();
    let app = TestApp::with_config(|config| {
        config.scryfall_api_base = uri;
        adjust(config);
    })
    .await;
    logged_in(&app).await;
    for (id, name) in [
        ("commander", "Tymna the Weaver"),
        ("partner", "Thrasios, Triton Hero"),
    ] {
        app.catalog_card(scryfall_card(id, name)).await;
        sqlx::query(
            "INSERT INTO card_printings (id, oracle_id, name, set_code, set_name, collector_number, lang, image_uris)
             VALUES (?, ?, ?, 'old', 'Original Set', '42', 'en', ?)",
        )
        .bind(format!("{id}-alternate"))
        .bind(format!("oracle-{id}"))
        .bind(name)
        .bind(
            json!({
                "art_crop": format!("https://img.example/{id}-alternate.jpg"),
                "normal": format!("https://img.example/{id}-alternate-card.jpg")
            })
            .to_string(),
        )
        .execute(app.pool())
        .await
        .unwrap();
    }
    app
}

fn catalog(app: &TestApp) -> Catalog {
    Catalog {
        pool: app.pool().clone(),
    }
}

async fn mount_card(server: &MockServer, id: &str, body: Value, times: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/cards/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn lists_and_caches_english_paper_printings_with_pagination_and_front_face_images() {
    let server = MockServer::start().await;
    let printing = merge(
        scryfall_card("commander", "Tymna the Weaver"),
        &json!({"game_changer": true, "id": "double-faced-print", "image_uris": null,
               "card_faces": [{"image_uris": {"art_crop": "https://img.example/front.jpg"}}]}),
    );
    let other = scryfall_card("partner", "Thrasios, Triton Hero");
    let digital = merge(
        printing.clone(),
        &json!({"id": "digital", "games": ["arena"]}),
    );
    let japanese = merge(printing.clone(), &json!({"id": "japanese", "lang": "ja"}));
    let memorabilia = merge(
        printing.clone(),
        &json!({"id": "memorabilia", "set_type": "memorabilia"}),
    );
    Mock::given(method("GET"))
        .and(path("/cards/search"))
        .and(query_param(
            "q",
            "oracleid:oracle-commander game:paper lang:en",
        ))
        .and(query_param("unique", "prints"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": [printing, other, digital, japanese, memorabilia], "has_more": true}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let app = printing_app(&server).await;

    let body = app
        .get("/api/card-printings?card_id=commander&page=2")
        .await
        .assert_json(200);
    assert_eq!(body["has_more"], true);
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["id"], "double-faced-print");
    assert_eq!(data[0]["lang"], "en");
    assert_eq!(data[0]["game_changer"], true);

    let stored = catalog(&app)
        .get_printing("double-faced-print")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.image_uris["art_crop"],
        "https://img.example/front.jpg"
    );
    assert!(stored.game_changer);
    for id in ["digital", "japanese", "memorabilia"] {
        assert!(
            catalog(&app).get_printing(id).await.unwrap().is_none(),
            "{id}"
        );
    }
    assert!(
        catalog(&app)
            .get_card("double-faced-print")
            .await
            .unwrap()
            .is_none()
    );
    let body = app
        .get("/api/card-printings/double-faced-print")
        .await
        .assert_json(200);
    assert_eq!(body["data"]["set_code"], "new");
    let request = &server.received_requests().await.unwrap()[0];
    assert!(request.headers.get("user-agent").is_some());
    assert!(
        !request
            .url
            .query()
            .unwrap_or_default()
            .contains("include_multilingual")
    );
}

#[tokio::test]
async fn serves_sibling_and_early_core_printings_without_catalog_membership() {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    for (id, name, set, number, lang) in [
        (
            "a51fb64d-cc0c-400d-971f-78c28d42043b",
            "Sol Talisman",
            "mh2",
            "236",
            "en",
        ),
        (
            "0a7cb0f8-2946-4b00-a192-0b31c8e1ec5c",
            "Nettlecyst",
            "mkc",
            "233",
            "en",
        ),
        (
            "97fa5f07-46ba-408d-a861-bdb1791cc188",
            "Serra Angel",
            "3ed",
            "40",
            "en",
        ),
        (
            "555e2c50-4d68-4ed1-b2eb-bd31dfc9f569",
            "Lightning Bolt",
            "3ed",
            "162",
            "it",
        ),
    ] {
        assert!(catalog(&app).get_card(id).await.unwrap().is_none());
        let card = merge(
            scryfall_card(id, name),
            &json!({"set": set, "collector_number": number, "lang": lang}),
        );
        mount_card(&server, id, card, 1).await;
        Mock::given(method("GET"))
            .and(path(format!("/cards/{id}/rulings")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .expect(1)
            .mount(&server)
            .await;

        let body = app
            .get(&format!("/api/card-printings/{id}/details"))
            .await
            .assert_json(200);
        assert_eq!(body["data"]["id"], id);
        assert_eq!(body["data"]["name"], name);
        assert_eq!(body["data"]["set_code"], set);
        assert_eq!(body["data"]["collector_number"], number);
        assert_eq!(body["data"]["lang"], lang);
        assert_eq!(
            body["data"]["image_uris"]["normal"],
            format!("https://img.example/{id}-default-card.jpg")
        );
        let body = app
            .get(&format!("/api/card-printings/{id}/rulings"))
            .await
            .assert_json(200);
        assert_eq!(body, json!({"data": []}));
    }
}

#[tokio::test]
async fn a_selectable_sibling_face_without_its_own_scan_still_returns_its_name_and_rules() {
    let server = MockServer::start().await;
    let card = merge(
        scryfall_card(MDFC, "Front // Back"),
        &json!({"image_uris": null, "layout": "modal_dfc", "image_status": "missing", "card_faces": [
            {"name": "Front"},
            {"name": "Back", "oracle_text": "Draw a card.", "type_line": "Sorcery"}
        ]}),
    );
    let mut card = card;
    card.as_object_mut().unwrap().remove("image_uris");
    mount_card(&server, MDFC, card, 1).await;
    let app = printing_app(&server).await;
    let id = format!("{MDFC}-1");
    let body = app
        .get(&format!("/api/card-printings/{id}/details"))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["id"], id);
    assert_eq!(body["data"]["name"], "Back");
    assert_eq!(body["data"]["oracle_text"], "Draw a card.");
    assert_eq!(body["data"]["type_line"], "Sorcery");
    assert_eq!(body["data"]["image_uris"], json!({}));
}

#[tokio::test]
async fn fetches_full_printing_details_by_scryfall_id_and_caches_the_printing() {
    let server = MockServer::start().await;
    let card = merge(
        scryfall_card(SAGA, "Kiora Bests the Sea God"),
        &json!({
            "mana_cost": "{5}{U}{U}",
            "type_line": "Enchantment — Saga",
            "oracle_text": "I — Create an 8/8 blue Kraken.",
            "power": null,
            "layout": "saga",
            "rarity": "mythic",
            "released_at": "2020-01-24",
            "prices": {"usd": "0.25", "usd_foil": "1.10", "usd_etched": null, "eur": "0.20"},
            "scryfall_uri": "https://scryfall.com/card/thb/52",
            "image_uris": {
                "small": "https://img.example/saga-small.jpg",
                "normal": "https://img.example/saga-normal.jpg",
                "png": "https://img.example/saga.png"
            }
        }),
    );
    mount_card(&server, SAGA, card, 1).await;
    let app = printing_app(&server).await;

    let response = app
        .get(&format!("/api/card-printings/{SAGA}/details"))
        .await;
    assert_eq!(
        response.header("cache-control"),
        Some("private, max-age=3600")
    );
    let body = response.assert_json(200)["data"].clone();
    assert_eq!(body["name"], "Kiora Bests the Sea God");
    assert_eq!(body["game_changer"], false);
    assert_eq!(body["mana_cost"], "{5}{U}{U}");
    assert_eq!(body["type_line"], "Enchantment — Saga");
    assert_eq!(body["oracle_text"], "I — Create an 8/8 blue Kraken.");
    assert_eq!(body["set_code"], "new");
    assert_eq!(body["set_name"], "New Set");
    assert_eq!(body["collector_number"], "9");
    assert_eq!(body["layout"], "saga");
    assert_eq!(body["rarity"], "mythic");
    assert_eq!(body["released_at"], "2020-01-24");
    assert_eq!(body["scryfall_uri"], "https://scryfall.com/card/thb/52");
    assert_eq!(
        body["prices"],
        json!({"usd": "0.25", "usd_foil": "1.10", "usd_etched": null})
    );
    assert_eq!(
        body["image_uris"],
        json!({"small": "https://img.example/saga-small.jpg", "normal": "https://img.example/saga-normal.jpg"})
    );
    assert!(body.get("games").is_none());
    assert_eq!(
        catalog(&app)
            .get_printing(SAGA)
            .await
            .unwrap()
            .unwrap()
            .set_name,
        "New Set"
    );

    // Every seat asks for the card; within a day the rest are answered without Scryfall.
    let cached = app
        .get(&format!("/api/card-printings/{SAGA}/details"))
        .await
        .assert_json(200);
    assert_eq!(cached["data"], body);
}

#[tokio::test]
async fn refetches_printing_details_once_the_cached_copy_is_a_day_old() {
    let server = MockServer::start().await;
    let first = merge(
        scryfall_card(SAGA, "Kiora Bests the Sea God"),
        &json!({"prices": {"usd": "0.25"}}),
    );
    let second = merge(
        scryfall_card(SAGA, "Kiora Bests the Sea God"),
        &json!({"prices": {"usd": "0.30"}}),
    );
    Mock::given(method("GET"))
        .and(path(format!("/cards/{SAGA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(first))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    mount_card(&server, SAGA, second, 1).await;
    let app = printing_app(&server).await;

    let body = app
        .get(&format!("/api/card-printings/{SAGA}/details"))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["prices"]["usd"], "0.25");
    let stale = the_gathering::db::UtcDateTime::now().plus(time::Duration::seconds(-86_400));
    sqlx::query("UPDATE card_details_cache SET fetched_at = ?")
        .bind(stale)
        .execute(app.pool())
        .await
        .unwrap();
    let body = app
        .get(&format!("/api/card-printings/{SAGA}/details"))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["prices"]["usd"], "0.30");
}

#[tokio::test]
async fn selects_each_printed_face_without_mixing_text_images_or_cached_ids() {
    let server = MockServer::start().await;
    let mut card = merge(
        scryfall_card(MDFC, "Valki, God of Lies // Tibalt, Cosmic Impostor"),
        &json!({"layout": "modal_dfc", "game_changer": true, "card_faces": [
            {
                "name": "Valki, God of Lies", "mana_cost": "{1}{B}", "type_line": "Legendary Creature — God",
                "oracle_text": "When Valki enters, each opponent reveals their hand.",
                "power": "2", "toughness": "1", "image_uris": {"normal": "https://img.example/valki.jpg"}
            },
            {
                "name": "Tibalt, Cosmic Impostor", "mana_cost": "{5}{B}{R}",
                "type_line": "Legendary Planeswalker — Tibalt",
                "oracle_text": "You may play cards exiled with Tibalt.", "loyalty": "5",
                "image_uris": {"normal": "https://img.example/tibalt.jpg"}
            }
        ]}),
    );
    card.as_object_mut().unwrap().remove("image_uris");
    mount_card(&server, MDFC, card, 2).await;
    let app = printing_app(&server).await;

    let body = app
        .get(&format!("/api/card-printings/{MDFC}/details"))
        .await
        .assert_json(200)["data"]
        .clone();
    assert_eq!(body["mana_cost"], "{1}{B}");
    assert_eq!(body["name"], "Valki, God of Lies");
    assert_eq!(body["game_changer"], true);
    assert_eq!(body["power"], "2");
    assert_eq!(body["toughness"], "1");
    assert_eq!(
        body["prices"],
        json!({"usd": null, "usd_foil": null, "usd_etched": null})
    );
    assert_eq!(
        body["oracle_text"],
        "When Valki enters, each opponent reveals their hand."
    );
    assert_eq!(
        body["image_uris"],
        json!({"normal": "https://img.example/valki.jpg"})
    );

    let back_id = format!("{MDFC}-1");
    let back = app
        .get(&format!("/api/card-printings/{back_id}/details"))
        .await
        .assert_json(200)["data"]
        .clone();
    assert_eq!(back["id"], back_id);
    assert_eq!(back["name"], "Tibalt, Cosmic Impostor");
    assert_eq!(back["game_changer"], true);
    assert_eq!(
        back["oracle_text"],
        "You may play cards exiled with Tibalt."
    );
    assert_eq!(back["mana_cost"], "{5}{B}{R}");
    assert_eq!(back["type_line"], "Legendary Planeswalker — Tibalt");
    assert_eq!(back["loyalty"], "5");
    assert_eq!(back["power"], Value::Null);
    assert_eq!(back["toughness"], Value::Null);
    assert_eq!(
        back["image_uris"],
        json!({"normal": "https://img.example/tibalt.jpg"})
    );
    assert_eq!(
        catalog(&app)
            .get_printing(MDFC)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Valki, God of Lies"
    );
    assert_eq!(
        catalog(&app)
            .get_printing(&back_id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Tibalt, Cosmic Impostor"
    );

    let shown = app
        .get(&format!("/api/card-printings/{back_id}"))
        .await
        .assert_json(200);
    let expected: serde_json::Map<String, Value> = [
        "id",
        "name",
        "game_changer",
        "set_code",
        "set_name",
        "collector_number",
        "lang",
        "image_uris",
    ]
    .into_iter()
    .map(|key| (key.to_owned(), back[key].clone()))
    .collect();
    assert_eq!(shown, json!({"data": expected}));
}

#[tokio::test]
async fn supports_transform_reversible_and_token_faces_including_face_level_oracle_ids() {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    for layout in ["transform", "reversible_card", "double_faced_token"] {
        let id = uuid::Uuid::new_v4().to_string();
        let back_id = format!("{id}-1");
        let mut card = merge(
            scryfall_card(&id, "Front // Back"),
            &json!({"layout": layout, "set_type": "token", "card_faces": [
                {"name": "Front", "oracle_id": "front-oracle"},
                {"name": "Back", "oracle_id": "back-oracle", "layout": "normal", "type_line": "Token Creature — Spirit",
                 "oracle_text": "Flying", "image_uris": {"normal": "https://img.example/back.jpg"}}
            ]}),
        );
        card.as_object_mut().unwrap().remove("oracle_id");
        card.as_object_mut().unwrap().remove("image_uris");
        mount_card(&server, &id, card, 1).await;
        let body = app
            .get(&format!("/api/card-printings/{back_id}/details"))
            .await
            .assert_json(200);
        assert_eq!(body["data"]["name"], "Back");
        assert_eq!(body["data"]["layout"], layout);
        assert_eq!(body["data"]["oracle_id"], "back-oracle");
        assert_eq!(body["data"]["oracle_text"], "Flying");
        let stored = catalog(&app).get_printing(&back_id).await.unwrap().unwrap();
        assert_eq!(stored.image_uris["normal"], "https://img.example/back.jpg");
    }
}

const HALVES: [(&str, [&str; 2], &str); 4] = [
    (
        "split",
        ["Mirror Room", "Fractured Realm"],
        "Enchantment — Room",
    ),
    ("split", ["Fire", "Ice"], "Instant"),
    ("split", ["Cut", "Ribbons"], "Sorcery"),
    (
        "flip",
        ["Budoka Gardener", "Dokai, Weaver of Life"],
        "Creature — Human Monk",
    ),
];

#[tokio::test]
async fn split_and_flip_halves_select_their_own_rules_but_retain_the_shared_front_image() {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    for (layout, names, type_line) in HALVES {
        let id = uuid::Uuid::new_v4().to_string();
        let faces: Vec<Value> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                json!({"name": name, "oracle_text": format!("Rules {i}"), "type_line": type_line,
                       "mana_cost": if i == 0 { "{2}{G}" } else { "" }})
            })
            .collect();
        let card = merge(
            scryfall_card(&id, &names.join(" // ")),
            &json!({"layout": layout, "card_faces": faces, "power": "99"}),
        );
        mount_card(&server, &id, card, 2).await;
        for (i, name) in names.iter().enumerate() {
            let face_id = if i == 0 {
                id.clone()
            } else {
                format!("{id}-1")
            };
            let data = app
                .get(&format!("/api/card-printings/{face_id}/details"))
                .await
                .assert_json(200)["data"]
                .clone();
            assert_eq!(data["id"], face_id);
            assert_eq!(data["name"], *name);
            assert_eq!(data["oracle_text"], format!("Rules {i}"));
            assert_eq!(data["type_line"], type_line);
            assert_eq!(data["power"], Value::Null);
            assert_eq!(data["mana_cost"], if i == 0 { "{2}{G}" } else { "" });
            assert_eq!(
                data["image_uris"]["normal"],
                format!("https://img.example/{id}-default-card.jpg")
            );
            let stored = catalog(&app).get_printing(&face_id).await.unwrap().unwrap();
            assert_eq!(stored.name, *name);
            assert_eq!(json!(stored.image_uris), data["image_uris"]);
        }
    }
}

#[tokio::test]
async fn alternate_printings_preserve_either_split_or_flip_half_without_changing_full_card_requests()
 {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    for (layout, names, _) in HALVES {
        let id = uuid::Uuid::new_v4().to_string();
        let full_name = names.join(" // ");
        let card = merge(
            scryfall_card(&id, &full_name),
            &json!({"layout": layout, "card_faces": names.iter().map(|name| json!({"name": name})).collect::<Vec<_>>()}),
        );
        app.catalog_card(card.clone()).await;
        Mock::given(method("GET"))
            .and(path("/cards/search"))
            .and(query_param(
                "q",
                format!("oracleid:oracle-{id} game:paper lang:en"),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [card], "has_more": false})),
            )
            .expect(3)
            .mount(&server)
            .await;
        for (name, suffix) in [(names[0], ""), (names[1], "-1"), (full_name.as_str(), "")] {
            let body = app
                .get(&format!("/api/card-printings?name={}", encode(name)))
                .await
                .assert_json(200);
            let data = body["data"].as_array().unwrap();
            assert_eq!(data.len(), 1);
            assert_eq!(data[0]["id"], format!("{id}{suffix}"));
            assert_eq!(data[0]["name"], name);
            assert_eq!(
                data[0]["image_uris"]["normal"],
                format!("https://img.example/{id}-default-card.jpg")
            );
        }
    }
}

#[tokio::test]
async fn prepare_and_adventure_retain_their_shared_image_and_combined_rules() {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    for layout in ["prepare", "adventure"] {
        let id = uuid::Uuid::new_v4().to_string();
        let card = merge(
            scryfall_card(&id, "Studious First-Year // Rampant Growth"),
            &json!({"layout": layout, "card_faces": [
                {"mana_cost": "{G}", "oracle_text": "When this creature enters, prepare."},
                {"mana_cost": "{1}{G}", "oracle_text": "Search for a basic land."}
            ]}),
        );
        mount_card(&server, &id, card, 1).await;
        let body = app
            .get(&format!("/api/card-printings/{id}/details"))
            .await
            .assert_json(200);
        assert_eq!(
            body["data"]["name"],
            "Studious First-Year // Rampant Growth"
        );
        assert_eq!(body["data"]["mana_cost"], "{G}");
        assert_eq!(
            body["data"]["oracle_text"],
            "When this creature enters, prepare.\n//\nSearch for a basic land."
        );
        assert_eq!(
            body["data"]["image_uris"]["normal"],
            format!("https://img.example/{id}-default-card.jpg")
        );
    }
}

#[tokio::test]
async fn rejects_malformed_or_unavailable_faces_and_reports_misses_and_outages() {
    let server = MockServer::start().await;
    mount_card(&server, SAGA, scryfall_card(SAGA, "Single-faced card"), 1).await;
    Mock::given(method("GET"))
        .and(path(format!("/cards/{MISSING}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cards/{DOWN}")))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let app = printing_app(&server).await;
    for suffix in ["-0", "-2", "-01", "-1-1", ":back", "-x"] {
        let id = format!("{MDFC}{suffix}");
        app.get(&format!("/api/card-printings/{}/details", encode(&id)))
            .await
            .assert_json(400);
        app.get(&format!("/api/card-printings/{}/rulings", encode(&id)))
            .await
            .assert_json(400);
    }
    app.get(&format!("/api/card-printings/{SAGA}-1/details"))
        .await
        .assert_json(400);
    app.get(&format!("/api/card-printings/{MISSING}/details"))
        .await
        .assert_json(404);
    app.get(&format!("/api/card-printings/{DOWN}/details"))
        .await
        .assert_json(502);
    app.clear_cookies();
    app.get(&format!("/api/card-printings/{MDFC}/details"))
        .await
        .assert_json(401);
}

#[tokio::test]
async fn queues_a_details_lookup_behind_the_shared_scryfall_limit_instead_of_failing() {
    let server = MockServer::start().await;
    mount_card(
        &server,
        SAGA,
        scryfall_card(SAGA, "Kiora Bests the Sea God"),
        1,
    )
    .await;
    let app = printing_app_with(&server, |config| config.scryfall_rate_limit = 1).await;
    // Another seat's lookup of the same card has just taken this window's only slot.
    app.state.scryfall.limiter().hit(
        "scryfall_card",
        WindowLimit {
            limit: 1,
            scale: Duration::from_millis(100),
        },
    );
    let body = app
        .get(&format!("/api/card-printings/{SAGA}/details"))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["name"], "Kiora Bests the Sea God");
}

#[tokio::test]
async fn supports_name_only_or_obsolete_catalog_references_and_surfaces_upstream_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/cards/search"))
        .and(query_param(
            "q",
            "oracleid:oracle-commander game:paper lang:en",
        ))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"error": "Unavailable"})))
        .expect(1)
        .mount(&server)
        .await;
    let app = printing_app(&server).await;
    app.get("/api/card-printings?card_id=obsolete&name=Tymna%20the%20Weaver")
        .await
        .assert_json(502);
    app.get("/api/card-printings?page=0").await.assert_json(400);
    app.get("/api/card-printings?name=Unknown")
        .await
        .assert_json(404);
    app.get("/api/card-printings/missing")
        .await
        .assert_json(404);
    app.clear_cookies();
    app.get("/api/card-printings?card_id=commander")
        .await
        .assert_json(401);
}

#[tokio::test]
async fn saved_printing_art_survives_a_catalog_replacement_and_a_missing_crop_falls_back() {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    sync_fixture(&app).await;
    assert!(catalog(&app).get_card("commander").await.unwrap().is_none());
    assert!(
        catalog(&app)
            .get_printing("commander-alternate")
            .await
            .unwrap()
            .is_some()
    );
    let art = catalog(&app)
        .art_crop_urls(&[
            CardRef::Card(
                Some("printing-latest".into()),
                Some("Lightning Bolt".into()),
            ),
            CardRef::Printing(Some("missing".into())),
        ])
        .await
        .unwrap();
    assert_eq!(
        art.art_crop_url(Some("printing-latest"), None, Some("missing"))
            .as_deref(),
        Some("https://img.example/latest-art.jpg")
    );
}

// ---- card_printing_controller_test.exs (deck surfaces) ----

struct DeckCtx {
    app: TestApp,
    player: the_gathering::games::Player,
    deck: the_gathering::games::Deck,
}

/// The printing setup plus the signed-in member's linked player and a partner deck.
async fn deck_ctx(server: &MockServer) -> DeckCtx {
    let app = printing_app(server).await;
    let user = app
        .state
        .accounts
        .get_user_by_username("member")
        .await
        .unwrap()
        .unwrap();
    let player = app.player("Printing owner").await;
    let player = app
        .state
        .games
        .link_player_to_user(&player, &user)
        .await
        .unwrap();
    let deck = app
        .deck_with(json!({
            "player_id": player.id,
            "name": "Partners",
            "commander_card_id": "commander",
            "commander_name": "Tymna the Weaver",
            "partner_card_id": "partner",
            "partner_name": "Thrasios, Triton Hero",
            "color_identity": "WUBG"
        }))
        .await;
    DeckCtx { app, player, deck }
}

async fn save(ctx: &DeckCtx, attrs: Value) -> Value {
    ctx.app
        .patch(&format!("/api/decks/{}", ctx.deck.id), json!({"deck": attrs}))
        .await
        .assert_json(200)["data"]
        .clone()
}

#[track_caller]
fn assert_art(deck: &Value) {
    assert_eq!(
        deck["commander_art_crop_url"],
        "https://img.example/commander-alternate.jpg"
    );
    assert_eq!(
        deck["partner_art_crop_url"],
        "https://img.example/partner-alternate.jpg"
    );
}

#[tokio::test]
async fn saves_independent_commander_and_partner_printings_and_resolves_them_on_every_deck_surface()
 {
    let server = MockServer::start().await;
    let ctx = deck_ctx(&server).await;
    let body = save(
        &ctx,
        json!({"commander_printing_id": "commander-alternate", "partner_printing_id": "partner-alternate"}),
    )
    .await;
    assert_eq!(body["commander_card_id"], "commander");
    assert_eq!(body["partner_card_id"], "partner");
    assert_eq!(body["color_identity"], "WUBG");
    assert_art(&body);

    let app = &ctx.app;
    assert_art(
        &app.get(&format!("/api/decks/{}", ctx.deck.id))
            .await
            .assert_json(200)["data"],
    );
    assert_art(&app.get("/api/decks").await.assert_json(200)["data"][0]);
    assert_art(
        &app.get(&format!("/api/players/{}", ctx.player.id))
            .await
            .assert_json(200)["data"]["decks"][0],
    );
    assert_art(&app.get("/api/deck-chooser").await.assert_json(200)["data"]["deck"]);

    let other = app.player("Other pilot").await;
    let mirror = app
        .deck_with(json!({
            "player_id": other.id,
            "name": "Default art",
            "commander_card_id": "commander",
            "commander_name": "Tymna the Weaver"
        }))
        .await;
    let game = app
        .game(
            json!({
                "played_at": "2026-09-21T12:00:00Z",
                "seats": [
                    {"player_id": ctx.player.id, "deck_id": ctx.deck.id, "seat": 1, "result": "win"},
                    {"player_id": other.id, "deck_id": mirror.id, "seat": 2, "result": "loss"}
                ]
            }),
            None,
        )
        .await;
    let body = app
        .get(&format!("/api/games/{}", game.id))
        .await
        .assert_json(200);
    let seats = body["data"]["seats"].as_array().unwrap();
    let selected = seats
        .iter()
        .find(|seat| seat["deck"]["id"] == ctx.deck.id)
        .unwrap();
    assert_art(&selected["deck"]);
    let default = seats
        .iter()
        .find(|seat| seat["deck"]["id"] == mirror.id)
        .unwrap();
    assert_eq!(
        default["deck"]["commander_art_crop_url"],
        "https://img.example/commander-default.jpg"
    );

    let commanders = the_gathering::stats::commanders(app.pool(), &json!({}))
        .await
        .unwrap();
    let commander: Vec<&Value> = commanders
        .iter()
        .filter(|row| row["id"] == "commander")
        .collect();
    assert_eq!(commander.len(), 1);
    assert_eq!((&commander[0]["games"], &commander[0]["wins"]), (&json!(2), &json!(1)));

    let imported = app
        .state
        .games
        .find_or_create_deck(
            ctx.player.id,
            "Imported partners",
            &json!({"commander_name": "Thrasios, Triton Hero", "partner_name": "Tymna the Weaver"}),
        )
        .await
        .unwrap();
    assert_eq!(imported.id, ctx.deck.id);
    assert_eq!(
        imported.commander_printing_id.as_deref(),
        Some("commander-alternate")
    );
}

#[tokio::test]
async fn deck_create_list_and_edit_expose_independent_full_card_printing_images() {
    let server = MockServer::start().await;
    let ctx = deck_ctx(&server).await;
    let created = ctx
        .app
        .post(
            "/api/decks",
            json!({"deck": {
                "player_id": ctx.player.id,
                "name": "Table partners",
                "commander_card_id": "commander",
                "commander_name": "Tymna the Weaver",
                "commander_printing_id": "commander-alternate",
                "partner_card_id": "partner",
                "partner_name": "Thrasios, Triton Hero",
                "partner_printing_id": "partner-alternate"
            }}),
        )
        .await
        .assert_json(201)["data"]
        .clone();
    assert_eq!(
        created["commander_image_url"],
        "https://img.example/commander-alternate-card.jpg"
    );
    assert_eq!(
        created["partner_image_url"],
        "https://img.example/partner-alternate-card.jpg"
    );
    assert_eq!(created["partner_name"], "Thrasios, Triton Hero");
    assert_art(&created);

    let listed = ctx.app.get("/api/decks").await.assert_json(200);
    let listed = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|deck| deck["id"] == created["id"])
        .unwrap()
        .clone();
    assert_eq!(listed["partner_image_url"], created["partner_image_url"]);

    let default = save(
        &ctx,
        json!({"commander_card_id": null, "partner_name": null, "partner_card_id": null}),
    )
    .await;
    // Legacy name-only decks fall back to the catalog; no partner means no partner image.
    assert_eq!(
        default["commander_image_url"],
        "https://img.example/commander-default-card.jpg"
    );
    assert_eq!(default["partner_image_url"], Value::Null);
}

#[tokio::test]
async fn rejects_unknown_and_mismatched_printings_atomically_including_on_create() {
    let server = MockServer::start().await;
    let ctx = deck_ctx(&server).await;
    for attrs in [
        json!({"commander_printing_id": "partner-alternate"}),
        json!({"partner_printing_id": "commander-alternate"}),
        json!({"commander_printing_id": "missing"}),
        json!({"commander_name": "Thrasios, Triton Hero", "commander_printing_id": "commander-alternate"}),
    ] {
        let mut attrs = attrs;
        attrs["name"] = json!("Must not change");
        ctx.app
            .patch(&format!("/api/decks/{}", ctx.deck.id), json!({"deck": attrs}))
            .await
            .assert_json(422);
    }
    let deck = ctx.app.state.games.get_deck(ctx.deck.id).await.unwrap().unwrap();
    assert_eq!(deck.name, "Partners");
    match ctx
        .app
        .state
        .games
        .create_deck(&json!({
            "player_id": ctx.player.id,
            "name": "Invalid",
            "commander_name": "Tymna the Weaver",
            "commander_printing_id": "partner-alternate"
        }))
        .await
    {
        Err(the_gathering::games::GamesError::Invalid(errors)) => {
            assert!(errors.has("commander_printing_id"));
        }
        other => panic!("expected a printing error, got {other:?}"),
    }
}

#[tokio::test]
async fn clears_defaults_explicitly_and_stale_printings_when_a_card_changes_or_partner_is_removed() {
    let server = MockServer::start().await;
    let ctx = deck_ctx(&server).await;
    save(
        &ctx,
        json!({"commander_printing_id": "commander-alternate", "partner_printing_id": "partner-alternate"}),
    )
    .await;
    assert_art(&save(&ctx, json!({"name": "Renamed"})).await);
    let default = save(&ctx, json!({"commander_printing_id": null})).await;
    assert_eq!(
        default["commander_art_crop_url"],
        "https://img.example/commander-default.jpg"
    );
    assert_eq!(default["partner_printing_id"], "partner-alternate");
    save(&ctx, json!({"commander_printing_id": "commander-alternate"})).await;
    let changed = save(
        &ctx,
        json!({
            "commander_card_id": "partner",
            "commander_name": "Thrasios, Triton Hero",
            "partner_card_id": null,
            "partner_name": null
        }),
    )
    .await;
    assert_eq!(changed["commander_printing_id"], Value::Null);
    assert_eq!(changed["partner_printing_id"], Value::Null);
    assert_eq!(
        changed["commander_art_crop_url"],
        "https://img.example/partner-default.jpg"
    );
    assert_eq!(changed["partner_art_crop_url"], Value::Null);
}

#[tokio::test]
async fn saved_deck_printing_art_survives_a_catalog_replacement() {
    let server = MockServer::start().await;
    let ctx = deck_ctx(&server).await;
    save(
        &ctx,
        json!({"commander_printing_id": "commander-alternate", "partner_printing_id": "partner-alternate"}),
    )
    .await;
    sync_fixture(&ctx.app).await;
    assert!(
        catalog(&ctx.app)
            .get_card("commander")
            .await
            .unwrap()
            .is_none()
    );
    assert_art(&save(&ctx, json!({"name": "After sync"})).await);
}

// ---- card_rulings_controller_test.exs ----

const PRINTING: &str = "00000000-0000-0000-0000-000000000001";
const EXPIRED: &str = "00000000-0000-0000-0000-000000000002";

async fn cached_rulings(app: &TestApp, id: &str) -> Option<Value> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT rulings FROM card_rulings_cache WHERE id = ?")
            .bind(id)
            .fetch_optional(app.pool())
            .await
            .unwrap();
    row.map(|rulings| serde_json::from_str(&rulings).unwrap())
}

#[tokio::test]
async fn fetches_exact_printing_rulings_and_caches_only_the_public_fields() {
    let server = MockServer::start().await;
    let ruling = json!({"source": "wotc", "published_at": "2026-01-23", "comment": "Draw a card."});
    let mut exposed = ruling.clone();
    exposed["oracle_id"] = json!("not-exposed");
    Mock::given(method("GET"))
        .and(path(format!("/cards/{PRINTING}/rulings")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [exposed]})))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/cards/{PRINTING}/rulings")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [ruling]})))
        .expect(1)
        .mount(&server)
        .await;
    let app = printing_app(&server).await;

    for _ in 0..2 {
        let body = app
            .get(&format!("/api/card-printings/{PRINTING}/rulings"))
            .await
            .assert_json(200);
        assert_eq!(body, json!({"data": [ruling]}));
    }
    assert_eq!(cached_rulings(&app, PRINTING).await, Some(json!([ruling])));

    let back = format!("{PRINTING}-1");
    for _ in 0..2 {
        let body = app
            .get(&format!("/api/card-printings/{back}/rulings"))
            .await
            .assert_json(200);
        assert_eq!(body, json!({"data": [ruling]}));
    }
    assert_eq!(cached_rulings(&app, &back).await, Some(json!([ruling])));
    assert_eq!(cached_rulings(&app, PRINTING).await, Some(json!([ruling])));
    let request = &server.received_requests().await.unwrap()[0];
    assert!(request.headers.get("user-agent").is_some());
}

#[tokio::test]
async fn caches_empty_results_and_refreshes_expired_results() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/cards/{EXPIRED}/rulings")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .expect(1)
        .mount(&server)
        .await;
    let app = printing_app(&server).await;
    let stale = the_gathering::db::UtcDateTime::now().plus(time::Duration::seconds(-86_400));
    sqlx::query("INSERT INTO card_rulings_cache (id, rulings, fetched_at) VALUES (?, ?, ?)")
        .bind(EXPIRED)
        .bind(json!([{"comment": "Outdated"}]).to_string())
        .bind(stale)
        .execute(app.pool())
        .await
        .unwrap();
    for _ in 0..2 {
        let body = app
            .get(&format!("/api/card-printings/{EXPIRED}/rulings"))
            .await
            .assert_json(200);
        assert_eq!(body, json!({"data": []}));
    }
    assert_eq!(cached_rulings(&app, EXPIRED).await, Some(json!([])));
}

#[tokio::test]
async fn rulings_require_authentication_and_do_not_cache_missing_malformed_or_failed_responses() {
    let server = MockServer::start().await;
    let app = printing_app(&server).await;
    for (status, body, expected) in [
        (404, json!({}), 404),
        (503, json!({}), 502),
        (200, json!({"data": "invalid"}), 502),
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        Mock::given(method("GET"))
            .and(path(format!("/cards/{id}/rulings")))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
        app.get(&format!("/api/card-printings/{id}/rulings"))
            .await
            .assert_json(expected);
        assert_eq!(cached_rulings(&app, &id).await, None);
    }
    // A transport failure (the connection is refused) is a bad gateway too.
    let unreachable =
        TestApp::with_config(|config| config.scryfall_api_base = "http://127.0.0.1:9".into()).await;
    logged_in(&unreachable).await;
    unreachable
        .get(&format!("/api/card-printings/{PRINTING}/rulings"))
        .await
        .assert_json(502);
    assert_eq!(cached_rulings(&unreachable, PRINTING).await, None);
    app.clear_cookies();
    app.get("/api/card-printings/secret/rulings")
        .await
        .assert_json(401);
}

// ---- card_image_controller_test.exs ----

const SOURCE: &str =
    "https://cards.scryfall.io/normal/back/a/b/abcdef01-2345-6789-abcd-ef0123456789.jpg?123";
const SOURCE_PATH: &str = "/normal/back/a/b/abcdef01-2345-6789-abcd-ef0123456789.jpg";
const JPEG: [u8; 7] = [255, 216, 255, 224, 1, 2, 3];

async fn image_app(server: &MockServer) -> TestApp {
    let uri = server.uri();
    let app = TestApp::with_config(|config| config.card_image_base = uri).await;
    logged_in(&app).await;
    app
}

fn image_files(app: &TestApp) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(app.state.config.data_dir.join("card-images"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jpg"))
        .collect()
}

#[tokio::test]
async fn serves_unchanged_bytes_for_image_accept_headers_caches_across_views_and_revalidates() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(SOURCE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(JPEG.to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    let app = image_app(&server).await;
    let url = images::url(SOURCE);
    let mut accept = HeaderMap::new();
    accept.insert(
        "accept",
        HeaderValue::from_static("image/avif,image/webp,image/*,*/*;q=0.8"),
    );

    let first = app
        .request_with(Method::GET, &url, None, accept.clone())
        .await;
    assert_eq!(first.status.as_u16(), 200);
    assert_eq!(first.body.as_ref(), JPEG);
    assert_eq!(first.header("x-card-image-cache"), Some("miss"));
    assert_eq!(
        first.header("cache-control"),
        Some("private, max-age=86400")
    );
    assert_eq!(first.header("content-type"), Some("image/jpeg"));
    assert_eq!(first.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(image_files(&app).len(), 1);
    let request = &server.received_requests().await.unwrap()[0];
    assert_eq!(request.url.query(), Some("123"));
    assert_eq!(request.headers.get("accept").unwrap(), "image/jpeg");

    let second = app
        .request_with(Method::GET, &url, None, accept.clone())
        .await;
    assert_eq!(second.status.as_u16(), 200);
    assert_eq!(second.body.as_ref(), JPEG);
    assert_eq!(second.header("x-card-image-cache"), Some("hit"));
    let etag = second.header("etag").unwrap().to_owned();
    let mut revalidate = accept;
    revalidate.insert("if-none-match", HeaderValue::from_str(&etag).unwrap());
    let third = app.request_with(Method::GET, &url, None, revalidate).await;
    assert_eq!(third.status.as_u16(), 304);
    assert!(third.body.is_empty());
}

#[tokio::test]
async fn rejects_arbitrary_origins_credentials_ports_traversal_and_redirects() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(SOURCE_PATH))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://localhost/private"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let app = image_app(&server).await;
    for source in [
        "http://cards.scryfall.io/normal/a.jpg".to_owned(),
        "https://localhost/a.jpg".to_owned(),
        SOURCE.replace("cards.scryfall.io", "cards.scryfall.io.evil.test"),
        SOURCE.replace("cards.scryfall.io", "user@cards.scryfall.io"),
        SOURCE.replace("cards.scryfall.io", "cards.scryfall.io:443"),
        SOURCE.replace("/normal/", "/normal/../"),
        format!("{SOURCE}&extra=1"),
    ] {
        app.get(&format!("/api/card-images?url={}", encode(&source)))
            .await
            .assert_json(400);
    }
    app.get("/api/card-images").await.assert_json(400);
    app.get(&images::url(SOURCE)).await.assert_json(502);
}

#[tokio::test]
async fn deduplicates_in_flight_fetches_and_bounds_distinct_upstream_concurrency() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(JPEG.to_vec())
                .set_delay(Duration::from_millis(600)),
        )
        .expect(5)
        .mount(&server)
        .await;
    let app = image_app(&server).await;
    let cache = app.state.card_images.clone();

    let first = tokio::spawn({
        let cache = cache.clone();
        async move { cache.fetch(SOURCE).await }
    });
    while server.received_requests().await.unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let duplicate = tokio::spawn({
        let cache = cache.clone();
        async move { cache.fetch(SOURCE).await }
    });
    let rest: Vec<_> = (1..=4)
        .map(|i| {
            let cache = cache.clone();
            tokio::spawn(async move { cache.fetch(&format!("{SOURCE}{i}")).await })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(cache.active_downloads(), 4);
    assert_eq!(server.received_requests().await.unwrap().len(), 4);

    let (body, status) = first.await.unwrap().unwrap();
    assert_eq!(body.as_ref(), JPEG);
    assert_eq!(status, CacheStatus::Miss);
    assert_eq!(duplicate.await.unwrap().unwrap().0.as_ref(), JPEG);
    for task in rest {
        let (body, status) = task.await.unwrap().unwrap();
        assert_eq!(body.as_ref(), JPEG);
        assert_eq!(status, CacheStatus::Miss);
    }
}

#[tokio::test]
async fn does_not_cache_failures_or_non_images() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not a JPEG"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(JPEG.to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    let app = image_app(&server).await;
    app.get(&images::url(SOURCE)).await.assert_json(502);
    assert_eq!(image_files(&app), Vec::<std::path::PathBuf>::new());
    let response = app.get(&images::url(SOURCE)).await;
    assert_eq!(response.status.as_u16(), 200);
    assert_eq!(response.body.as_ref(), JPEG);
}

#[tokio::test]
async fn honors_cdn_rate_limiting_across_different_image_urls() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "90")
                .set_body_string("slow down"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let app = image_app(&server).await;
    app.get(&images::url(SOURCE)).await.assert_json(502);
    app.get(&images::url(&format!("{SOURCE}1")))
        .await
        .assert_json(502);
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    assert!(app.state.card_images.paused_until() >= now + 89);
}

#[tokio::test]
async fn prunes_expired_and_oldest_files_at_startup_without_losing_fresh_cached_data() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("card-images");
    std::fs::create_dir_all(&directory).unwrap();
    let now = std::time::SystemTime::now();
    let old = directory.join("old.jpg");
    let expired = directory.join("expired.jpg");
    let fresh = directory.join("fresh.jpg");
    std::fs::write(&old, "x".repeat(20)).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(now - Duration::from_secs(10))
        .unwrap();
    std::fs::write(&expired, JPEG).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&expired)
        .unwrap()
        .set_modified(now - Duration::from_hours(30 * 24))
        .unwrap();
    // A sparse file tests the real 512 MiB limit without allocating that much RAM or disk.
    std::fs::File::create(&fresh)
        .unwrap()
        .set_len(512 * 1024 * 1024 - 10)
        .unwrap();

    CardImages::new(root.path(), "http://127.0.0.1:9").unwrap();
    assert!(!expired.exists());
    assert!(!old.exists());
    assert!(fresh.exists());
}

#[tokio::test]
async fn aborts_an_oversized_response_instead_of_storing_it() {
    let server = MockServer::start().await;
    let mut body = JPEG.to_vec();
    body.extend(std::iter::repeat_n(0, 2 * 1024 * 1024));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(1)
        .mount(&server)
        .await;
    let app = image_app(&server).await;
    app.get(&images::url(SOURCE)).await.assert_json(502);
    assert_eq!(image_files(&app), Vec::<std::path::PathBuf>::new());
}

#[tokio::test]
async fn image_fetches_require_a_session_and_an_accepted_source() {
    let app = TestApp::new().await;
    app.get(&images::url(SOURCE)).await.assert_json(401);
    assert_eq!(
        app.state
            .card_images
            .fetch("https://evil.example/a.jpg")
            .await,
        Err(ImageError::BadRequest)
    );
}
