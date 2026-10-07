//! Ported from `test/the_gathering/catalog_test.exs`, `catalog/sync_test.exs`,
//! `catalog/backfill_test.exs`, and the commander rules in `catalog/card_data_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use std::io::Write;

use serde_json::{Value, json};
use support::{TestApp, fixture, fixture_path};
use the_gathering::catalog::backfill::{self, Cursor};
use the_gathering::catalog::sync::{self, Source};
use the_gathering::catalog::{Card, Catalog};

fn catalog(app: &TestApp) -> Catalog {
    Catalog {
        pool: app.pool().clone(),
    }
}

async fn run(app: &TestApp, source: Source) -> Result<i64, String> {
    sync::run(app.pool(), &app.state.scryfall, source).await
}

async fn sync_fixture(app: &TestApp) {
    assert_eq!(
        run(app, Source::File(fixture_path("scryfall_catalog.jsonl"))).await,
        Ok(2)
    );
}

async fn get(app: &TestApp, id: &str) -> Card {
    catalog(app)
        .get_card(id)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("card {id}"))
}

/// `CatalogTest.insert_card/4`.
async fn insert(app: &TestApp, id: &str, oracle_id: &str, name: &str, overrides: Value) {
    let mut record = json!({
        "id": id, "oracle_id": oracle_id, "name": name, "collector_number": id, "type_line": "Instant"
    });
    for (key, value) in overrides.as_object().unwrap() {
        record[key] = value.clone();
    }
    app.card(record).await;
}

async fn search(
    app: &TestApp,
    query: &str,
    limit: Option<i64>,
    commander: Option<bool>,
    partner: bool,
) -> Vec<String> {
    catalog(app)
        .search(query, limit, commander, partner)
        .await
        .unwrap()
        .into_iter()
        .map(|card| card.id)
        .collect()
}

#[tokio::test]
async fn ranks_exact_names_before_prefixes_and_substrings() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(&app, "exact", "oracle-exact", "Bolt", json!({})).await;
    insert(&app, "prefix", "oracle-prefix", "Bolt Hound", json!({})).await;
    insert(
        &app,
        "substring",
        "oracle-substring",
        "Thunder Bolt Adept",
        json!({}),
    )
    .await;
    assert_eq!(
        search(&app, "bolt", None, None, false).await,
        ["exact", "prefix", "printing-latest", "substring"]
    );
}

#[tokio::test]
async fn folds_accents_in_names() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    let found = catalog(&app)
        .search("Jotun", None, None, false)
        .await
        .unwrap();
    assert_eq!(
        found
            .iter()
            .map(|card| card.name.as_str())
            .collect::<Vec<_>>(),
        ["Jötun Grunt"]
    );
}

#[tokio::test]
async fn matches_omitted_straight_and_curly_apostrophes_while_retaining_ranking() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(
        &app,
        "jeska-exact",
        "oracle-jeska-exact",
        "Jeska's Will",
        json!({}),
    )
    .await;
    insert(
        &app,
        "jeska-prefix",
        "oracle-jeska-prefix",
        "Jeskas Willpower",
        json!({}),
    )
    .await;
    insert(
        &app,
        "jeska-substring",
        "oracle-jeska-substring",
        "Copy of Jeska’s Will",
        json!({}),
    )
    .await;
    let expected = ["jeska-exact", "jeska-prefix", "jeska-substring"];
    for query in ["Jeskas Will", "Jeska's Will", "Jeska’s Will"] {
        assert_eq!(
            search(&app, query, None, None, false).await,
            expected,
            "{query}"
        );
    }
}

#[tokio::test]
async fn matches_partial_unique_names() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(
        &app,
        "lumra",
        "oracle-lumra",
        "Lumra, Bellow of the Woods",
        json!({}),
    )
    .await;
    assert_eq!(search(&app, "lumra", None, None, false).await, ["lumra"]);
}

#[tokio::test]
async fn ignores_commas_and_ranks_whole_leading_names_ahead_of_asymmetric_decoys() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(
        &app,
        "bello",
        "oracle-bello",
        "Bello, Bard of the Brambles",
        json!({}),
    )
    .await;
    insert(
        &app,
        "bellowing",
        "oracle-bellowing",
        "Bellowing Crier",
        json!({}),
    )
    .await;
    insert(
        &app,
        "lumra",
        "oracle-lumra",
        "Lumra, Bellow of the Woods",
        json!({}),
    )
    .await;
    insert(
        &app,
        "sephiroth",
        "oracle-sephiroth",
        "Sephiroth, Fabled SOLDIER // Sephiroth, One-Winged Angel",
        json!({}),
    )
    .await;
    insert(
        &app,
        "terra",
        "oracle-terra",
        "Terra, Magical Adept // Esper Terra",
        json!({}),
    )
    .await;
    assert_eq!(
        search(&app, "Bello", None, None, false).await,
        ["bello", "bellowing", "lumra"]
    );
    assert_eq!(
        search(&app, "sephiroth fabled soldier", None, None, false).await,
        ["sephiroth"]
    );
    assert_eq!(
        search(&app, "Terra magical adept", None, None, false).await,
        ["terra"]
    );
}

#[tokio::test]
async fn comma_insensitive_search_preserves_filters_and_limits() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(
        &app,
        "commander",
        "oracle-commander",
        "Bello, Bard of the Brambles",
        json!({"type_line": "Legendary Creature — Raccoon Bard"}),
    )
    .await;
    for number in 1..=25 {
        insert(
            &app,
            &format!("decoy-{number}"),
            &format!("oracle-decoy-{number}"),
            &format!("Bellowing Decoy {number}"),
            json!({}),
        )
        .await;
    }
    assert_eq!(
        search(&app, "Bello", Some(1), Some(true), false).await,
        ["commander"]
    );
    assert_eq!(search(&app, "Bello", Some(50), None, false).await.len(), 26);
}

#[tokio::test]
async fn applies_commander_and_partner_filters_to_apostrophe_insensitive_matches() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(&app, "spell", "oracle-spell", "Hero's Aid", json!({})).await;
    insert(
        &app,
        "partner",
        "oracle-partner",
        "Heros Aid Captain",
        json!({"type_line": "Legendary Creature — Human", "oracle_text": "Partner"}),
    )
    .await;
    assert_eq!(
        search(&app, "heros aid", None, Some(true), false).await,
        ["partner"]
    );
    assert_eq!(
        search(&app, "hero’s aid", None, None, true).await,
        ["partner"]
    );
}

#[tokio::test]
async fn treats_sql_wildcard_characters_literally() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    insert(
        &app,
        "percent",
        "oracle-percent",
        "A 100% Real Card",
        json!({}),
    )
    .await;
    insert(
        &app,
        "percent-decoy",
        "oracle-percent-decoy",
        "A 100X Real Card",
        json!({}),
    )
    .await;
    insert(
        &app,
        "underscore",
        "oracle-underscore",
        "Under_score",
        json!({}),
    )
    .await;
    insert(
        &app,
        "underscore-decoy",
        "oracle-underscore-decoy",
        "UnderXscore",
        json!({}),
    )
    .await;
    assert_eq!(search(&app, "100%", None, None, false).await, ["percent"]);
    assert_eq!(
        search(&app, "under_score", None, None, false).await,
        ["underscore"]
    );
}

// ---- sync_test.exs ----

fn write_lines(dir: &tempfile::TempDir, name: &str, lines: &[Value]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let body: Vec<String> = lines.iter().map(Value::to_string).collect();
    std::fs::write(&path, body.join("\n")).unwrap();
    path
}

#[tokio::test]
async fn publishes_and_refreshes_game_changer_flags_through_staging_and_backfill() {
    let app = TestApp::new().await;
    let dir = tempfile::tempdir().unwrap();
    let cards = [
        json!({"id": "rhystic", "oracle_id": "oracle-rhystic", "name": "Rhystic Study", "game_changer": true}),
        json!({"id": "bolt", "oracle_id": "oracle-bolt", "name": "Lightning Bolt", "game_changer": false}),
    ];
    let source = write_lines(&dir, "game-changers.jsonl", &cards);
    assert_eq!(run(&app, Source::File(source.clone())).await, Ok(2));
    assert!(get(&app, "rhystic").await.game_changer);
    assert!(!get(&app, "bolt").await.game_changer);
    backfill::run(app.pool()).await.unwrap();
    assert!(get(&app, "rhystic").await.game_changer);

    let unflagged: Vec<Value> = cards
        .iter()
        .map(|card| {
            let mut card = card.clone();
            card["game_changer"] = json!(false);
            card
        })
        .collect();
    write_lines(&dir, "game-changers.jsonl", &unflagged);
    assert_eq!(run(&app, Source::File(source)).await, Ok(2));
    assert!(!get(&app, "rhystic").await.game_changer);
}

#[tokio::test]
async fn chooses_the_latest_preferred_paper_printing_and_is_idempotent() {
    let app = TestApp::new().await;
    sync_fixture(&app).await;
    let card = get(&app, "printing-latest").await;
    assert_eq!(card.oracle_id, "oracle-bolt");
    assert_eq!(card.set_code, "new");
    assert_eq!(
        card.image_uris["normal"],
        "https://img.example/latest-normal.jpg"
    );
    assert!(
        catalog(&app)
            .get_card("printing-promo")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        catalog(&app)
            .get_card("printing-digital")
            .await
            .unwrap()
            .is_none()
    );

    sync_fixture(&app).await;
    assert_eq!(catalog(&app).count_cards().await.unwrap(), 2);
    assert_eq!(get(&app, "printing-latest").await.set_code, "new");
}

#[tokio::test]
async fn reads_gzip_generations() {
    let app = TestApp::new().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.jsonl.gz");
    let mut encoder = flate2::write::GzEncoder::new(
        std::fs::File::create(&path).unwrap(),
        flate2::Compression::fast(),
    );
    encoder
        .write_all(&fixture("scryfall_catalog.jsonl"))
        .unwrap();
    encoder.finish().unwrap();
    assert_eq!(run(&app, Source::GzipFile(path)).await, Ok(2));
    assert_eq!(get(&app, "jotun").await.name, "Jötun Grunt");
}

#[tokio::test]
async fn downloads_the_default_cards_bulk_file_from_scryfall() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(&fixture("scryfall_catalog.jsonl")).unwrap();
    let body = gzip.finish().unwrap();
    Mock::given(method("GET"))
        .and(path("/bulk-data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"type": "oracle_cards", "jsonl_download_uri": format!("{}/wrong.jsonl.gz", server.uri())},
            {"type": "default_cards", "jsonl_download_uri": format!("{}/default.jsonl.gz", server.uri()),
             "updated_at": "2026-10-01T09:10:11.123+00:00"}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/default.jsonl.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;
    let app = TestApp::with_config(|config| config.scryfall_api_base = server.uri()).await;
    assert_eq!(run(&app, Source::Scryfall).await, Ok(2));
    let status = catalog(&app).sync_status().await.unwrap();
    assert_eq!(status.status, "succeeded");
    assert_eq!(
        status.scryfall_updated_at.unwrap().to_string(),
        "2026-10-01T09:10:11Z"
    );
}

#[tokio::test]
async fn records_successful_and_failed_state_transitions_without_replacing_a_good_catalog() {
    let app = TestApp::new().await;
    assert_eq!(catalog(&app).sync_status().await.unwrap().status, "never");
    sync_fixture(&app).await;
    let succeeded = catalog(&app).sync_status().await.unwrap();
    assert_eq!(succeeded.status, "succeeded");
    assert_eq!(succeeded.card_count, 2);
    assert!(succeeded.last_started_at.is_some());
    assert!(succeeded.last_finished_at.is_some());
    assert_eq!(succeeded.last_error, None);

    assert!(
        run(&app, Source::File("/does/not/exist.jsonl".into()))
            .await
            .is_err()
    );
    let failed = catalog(&app).sync_status().await.unwrap();
    assert_eq!(failed.status, "failed");
    assert!(failed.last_error.unwrap().contains("could not stream"));
    assert_eq!(catalog(&app).count_cards().await.unwrap(), 2);
}

async fn good_catalog(app: &TestApp) -> Vec<Card> {
    sync_fixture(app).await;
    vec![get(app, "printing-latest").await, get(app, "jotun").await]
}

async fn assert_unchanged(app: &TestApp, original: &[Card]) {
    assert_eq!(catalog(app).count_cards().await.unwrap(), 2);
    for card in original {
        assert_eq!(&get(app, &card.id).await, card);
    }
}

#[tokio::test]
async fn rejects_an_empty_generation_without_replacing_a_good_catalog() {
    let app = TestApp::new().await;
    let original = good_catalog(&app).await;
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.jsonl");
    std::fs::write(&empty, "").unwrap();
    assert_eq!(
        run(&app, Source::File(empty)).await,
        Err("staged catalog generation is empty".to_owned())
    );
    assert_unchanged(&app, &original).await;
    let status = catalog(&app).sync_status().await.unwrap();
    assert_eq!(status.status, "failed");
    assert!(
        status
            .last_error
            .unwrap()
            .contains("staged catalog generation is empty")
    );
}

#[tokio::test]
async fn rejects_an_all_filtered_generation_without_replacing_a_good_catalog() {
    let app = TestApp::new().await;
    let original = good_catalog(&app).await;
    let dir = tempfile::tempdir().unwrap();
    let filtered = write_lines(
        &dir,
        "filtered.jsonl",
        &[json!({"set_type": "memorabilia"})],
    );
    assert_eq!(
        run(&app, Source::File(filtered)).await,
        Err("staged catalog generation is empty".to_owned())
    );
    assert_unchanged(&app, &original).await;
}

#[tokio::test]
async fn does_not_replace_a_good_catalog_when_decoding_fails_after_a_batch_is_staged() {
    let app = TestApp::new().await;
    let original = good_catalog(&app).await;
    let dir = tempfile::tempdir().unwrap();
    let contents = String::from_utf8(fixture("scryfall_catalog.jsonl")).unwrap();
    let first = contents.lines().next().unwrap();
    let partial = dir.path().join("partial.jsonl");
    std::fs::write(
        &partial,
        format!("{}not-json\n", format!("{first}\n").repeat(250)),
    )
    .unwrap();
    let reason = run(&app, Source::File(partial)).await.unwrap_err();
    assert!(reason.contains("invalid Scryfall bulk JSON"), "{reason}");
    assert_unchanged(&app, &original).await;
}

// ---- backfill_test.exs ----

async fn backfill_card(
    app: &TestApp,
    id: &str,
    name: &str,
    colors: &[&str],
    type_line: &str,
    commander: bool,
) {
    // Rows written the way `%Card{}` inserts did, including commander status that the
    // type line alone would not give (the token "Angel").
    app.card(json!({
        "id": id, "oracle_id": format!("oracle-{id}"), "name": name, "collector_number": id,
        "type_line": type_line, "color_identity": colors, "rarity": "rare"
    }))
    .await;
    sqlx::query("UPDATE cards SET can_be_commander = ? WHERE id = ?")
        .bind(commander)
        .bind(id)
        .execute(app.pool())
        .await
        .unwrap();
}

async fn backfill_setup(app: &TestApp) -> i64 {
    backfill_card(
        app,
        "frodo",
        "Frodo, Adventurous Hobbit",
        &["W", "G"],
        "Legendary Creature",
        true,
    )
    .await;
    backfill_card(
        app,
        "sam",
        "Sam, Loyal Attendant",
        &["W", "B"],
        "Legendary Creature",
        true,
    )
    .await;
    backfill_card(
        app,
        "eowyn",
        "Éowyn, Shieldmaiden",
        &["W"],
        "Legendary Creature",
        true,
    )
    .await;
    backfill_card(
        app,
        "a-alrund",
        "A-Alrund, God of the Cosmos // A-Hakka, Whispering Raven",
        &["U"],
        "Legendary Creature",
        true,
    )
    .await;
    backfill_card(
        app,
        "alrund",
        "Alrund, God of the Cosmos // Hakka, Whispering Raven",
        &["U"],
        "Legendary Creature",
        true,
    )
    .await;
    backfill_card(app, "angel-token", "Angel", &[], "Token Creature", false).await;
    backfill_card(app, "angel", "Angel", &["W"], "Legendary Creature", true).await;
    backfill_card(app, "rhystic", "Rhystic Study", &[], "Enchantment", false).await;
    app.player("Cody").await
}

#[derive(Debug, sqlx::FromRow)]
struct DeckRow {
    name: String,
    commander_name: String,
    partner_name: Option<String>,
    commander_card_id: Option<String>,
    partner_card_id: Option<String>,
    color_identity: String,
}

async fn deck(app: &TestApp, id: i64) -> DeckRow {
    sqlx::query_as(
        "SELECT name, commander_name, partner_name, commander_card_id, partner_card_id, color_identity FROM decks WHERE id = ?",
    )
    .bind(id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn splits_piped_partners_links_both_cards_and_renames_default_deck_names() {
    let app = TestApp::new().await;
    let player = backfill_setup(&app).await;
    let piped = "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)";
    let default_named = app
        .deck(player, piped, piped, json!({"color_identity": "WBG"}))
        .await;
    let custom = app.deck(player, "Hobbit friends", piped, json!({})).await;

    let summary = backfill::run(app.pool()).await.unwrap();
    assert_eq!(
        (
            summary.decks_split,
            summary.decks_linked,
            summary.colors_filled
        ),
        (2, 2, 1)
    );
    assert_eq!(summary.unmatched, Vec::<String>::new());

    let row = deck(&app, default_named).await;
    assert_eq!(row.name, "Frodo, Adventurous Hobbit / Sam, Loyal Attendant");
    assert_eq!(row.commander_name, "Frodo, Adventurous Hobbit");
    assert_eq!(row.partner_name.as_deref(), Some("Sam, Loyal Attendant"));
    assert_eq!(row.commander_card_id.as_deref(), Some("frodo"));
    assert_eq!(row.partner_card_id.as_deref(), Some("sam"));
    // An existing color identity is trusted, not recomputed.
    assert_eq!(row.color_identity, "WBG");

    let row = deck(&app, custom).await;
    assert_eq!(row.name, "Hobbit friends");
    assert_eq!(row.color_identity, "WBG");
}

#[tokio::test]
async fn matches_accented_names_front_faces_and_prefers_real_commanders_over_tokens() {
    let app = TestApp::new().await;
    let player = backfill_setup(&app).await;
    let eowyn = app
        .deck(player, "E", "Eowyn, Shieldmaiden", json!({}))
        .await;
    let alrund = app
        .deck(player, "A", "Alrund, God of the Cosmos", json!({}))
        .await;
    let angel = app.deck(player, "Ang", "Angel", json!({})).await;
    let missing = app.deck(player, "M", "Nobody Here", json!({})).await;

    let summary = backfill::run(app.pool()).await.unwrap();
    assert_eq!(summary.decks_linked, 3);
    assert_eq!(summary.unmatched, ["Nobody Here"]);
    assert_eq!(
        deck(&app, eowyn).await.commander_card_id.as_deref(),
        Some("eowyn")
    );
    assert_eq!(
        deck(&app, alrund).await.commander_card_id.as_deref(),
        Some("alrund")
    );
    assert_eq!(
        deck(&app, angel).await.commander_card_id.as_deref(),
        Some("angel")
    );
    assert_eq!(deck(&app, missing).await.commander_card_id, None);

    // Rerunning is a no-op apart from retrying the unmatched deck.
    let again = backfill::run(app.pool()).await.unwrap();
    assert_eq!((again.decks_linked, again.decks_split), (0, 0));
    assert_eq!(again.unmatched, ["Nobody Here"]);
}

#[tokio::test]
async fn links_mvp_cards_recorded_by_name_only() {
    let app = TestApp::new().await;
    let player = backfill_setup(&app).await;
    let other = app.player("Jules").await;
    let deck = app
        .deck(player, "F", "Frodo, Adventurous Hobbit", json!({}))
        .await;
    let deck2 = app
        .deck(other, "S", "Sam, Loyal Attendant", json!({}))
        .await;
    let game = app
        .game(&[
            (player, Some(deck), 1, "win", Some("Rhystic Study")),
            (other, Some(deck2), 2, "loss", None),
        ])
        .await;

    assert_eq!(backfill::run(app.pool()).await.unwrap().mvps_linked, 1);
    let mvp: Option<String> = sqlx::query_scalar(
        "SELECT mvp_card_id FROM game_players WHERE game_id = ? AND result = 'win'",
    )
    .bind(game)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(mvp.as_deref(), Some("rhystic"));
}

#[tokio::test]
async fn bounded_repair_returns_a_cursor_and_reports_update_conflicts() {
    let app = TestApp::new().await;
    let player = backfill_setup(&app).await;
    app.deck(
        player,
        "Frodo, Adventurous Hobbit / Sam, Loyal Attendant",
        "Frodo, Adventurous Hobbit",
        json!({"commander_card_id": "frodo"}),
    )
    .await;
    let piped_name = "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)";
    let piped = app.deck(player, piped_name, piped_name, json!({})).await;

    let first = backfill::repair_batch(app.pool(), Cursor::default(), 1)
        .await
        .unwrap();
    assert!(!first.done);
    assert_eq!(first.conflicts.len(), 1);
    assert_eq!(first.conflicts[0].resource, "deck");
    assert_eq!(first.conflicts[0].id, piped);
    assert!(first.conflicts[0].fields.contains(&"name"));
    assert_eq!(deck(&app, piped).await.commander_card_id, None);

    let second = backfill::repair_batch(app.pool(), first.cursor, 1)
        .await
        .unwrap();
    assert!(second.done);
}

#[test]
fn splits_partner_notation() {
    assert_eq!(
        backfill::split_partners("Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)"),
        (
            "Frodo, Adventurous Hobbit".to_owned(),
            Some("Sam, Loyal Attendant".to_owned())
        )
    );
    assert_eq!(
        backfill::split_partners(" Solo "),
        ("Solo".to_owned(), None)
    );
}

// ---- card_data_test.exs (commander rules now live in lotus) ----

#[test]
fn derives_commander_eligibility_without_treating_backgrounds_as_commanders() {
    assert!(lotus::can_be_commander(
        "Legendary Creature — Human Wizard",
        ""
    ));
    assert!(!lotus::can_be_commander("Legendary Artifact", ""));
    assert!(lotus::can_be_commander(
        "Legendary Planeswalker — Test",
        "Test can be your commander."
    ));
    assert!(!lotus::can_be_commander(
        "Legendary Enchantment — Background",
        ""
    ));
}

#[test]
fn recognizes_pairing_wording() {
    let pairing = |type_line: &str, text: &str| {
        lotus::commander_pairing(type_line, text).map(lotus::CommanderPairing::as_str)
    };
    let creature = "Legendary Creature — Human";
    assert_eq!(pairing(creature, "Partner"), Some("partner"));
    assert_eq!(
        pairing(creature, "Friends forever"),
        Some("friends_forever")
    );
    assert_eq!(
        pairing(creature, "Choose a Background"),
        Some("choose_a_background")
    );
    assert_eq!(
        pairing("Legendary Enchantment — Background", ""),
        Some("background")
    );
    assert_eq!(
        pairing(
            creature,
            "Reach\nChoose a Background (You can have a Background as a second commander.)"
        ),
        Some("choose_a_background")
    );
    assert_eq!(
        pairing(
            creature,
            "Goad that creature.\nPartner—Friends forever (You can have two commanders if both have this ability.)"
        ),
        Some("friends_forever")
    );
    assert_eq!(
        pairing(
            creature,
            "Menace\nPartner—Survivors (You can have two commanders if both have this ability.)"
        ),
        Some("partner")
    );
    assert_eq!(
        pairing(
            creature,
            "Partner with Toothy, Imaginary Friend (When this creature enters, …)"
        ),
        Some("partner_with")
    );
    assert_eq!(
        pairing(
            "Legendary Creature — Human Advisor",
            "Doctor's companion (You can have two commanders if the other is the Doctor.)"
        ),
        Some("doctors_companion")
    );
    assert_eq!(
        pairing("Legendary Creature — Time Lord Doctor", "Haste"),
        Some("doctor")
    );
    assert_eq!(
        pairing("Legendary Creature — Time Lord Scientist", ""),
        None
    );
    assert_eq!(pairing("Creature — Time Lord Doctor", ""), None);
    assert_eq!(
        pairing(
            creature,
            "Whenever you cast a Doctor spell or creature spell with doctor's companion, draw a card."
        ),
        None
    );
    assert_eq!(pairing(creature, "Partners in crime"), None);
}
