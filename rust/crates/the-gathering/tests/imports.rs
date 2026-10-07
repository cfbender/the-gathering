//! Ported from `test/the_gathering/imports_test.exs` and `test/the_gathering/imports/*_test.exs`
//! (CSV transfer, Google Sheet parsing, Mythic Track, portable transfer, and sheet
//! reconciliation).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::needless_pass_by_value
)]

mod support;

use std::collections::HashMap;

use serde_json::{Map, Value, json};
use support::{TestApp, utc};
use the_gathering::games::{Game, GameResult, Player};
use the_gathering::imports::google_sheet;
use the_gathering::imports::preview::{self, Source};
use the_gathering::imports::sheet_preview::{self, SheetPreview};
use the_gathering::imports::sheet_resolution::{Choice, ResolvedRow};
use the_gathering::imports::{
    ImportError, ImportResult, Preview, commit, csv_transfer, portable, sheet_commit,
};

// Helpers

async fn preview_csv(app: &TestApp, csv: &str) -> Preview {
    csv_transfer::preview(&app.state, csv).await.unwrap()
}

async fn import_csv(
    app: &TestApp,
    csv: &str,
    user_id: Option<i64>,
    revision: Option<&str>,
) -> Result<ImportResult, ImportError> {
    csv_transfer::run(&app.state, csv, user_id, revision).await
}

fn validation(result: Result<ImportResult, ImportError>) -> Preview {
    match result {
        Err(ImportError::Validation(preview)) => *preview,
        other => panic!("expected a validation preview, got {other:?}"),
    }
}

async fn game(app: &TestApp, id: i64) -> Game {
    app.state.games.get_game(id).await.unwrap().unwrap()
}

async fn count(app: &TestApp, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(app.pool())
        .await
        .unwrap()
}

async fn player_named(app: &TestApp, name: &str) -> Option<Player> {
    app.state
        .games
        .list_players(true)
        .await
        .unwrap()
        .into_iter()
        .find(|player| player.name == name)
}

async fn player_by_discord(app: &TestApp, discord_id: &str) -> Option<i64> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM players WHERE discord_id = ?")
        .bind(discord_id)
        .fetch_optional(app.pool())
        .await
        .unwrap()
}

// imports_test.exs

const CSV: &str =
    "game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
friday-1,2026-09-18,Alice,Birds,\"Kangee, Sky Warden\",1,win,Swan Song,75,10,Close game
friday-1,2026-09-18,Bob,Goblins,Krenko,2,loss,,75,10,Close game
";

#[tokio::test]
async fn previews_the_native_seat_per_row_template_and_reports_matches_and_creates() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let birds = app.deck(alice.id, "Birds", "Kangee, Sky Warden").await;

    let preview = preview_csv(&app, CSV).await;
    assert!(preview.valid);
    assert!(preview.errors.is_empty());
    assert_eq!(preview.games.len(), 1);
    let game = &preview.games[0];
    assert_eq!(game.game_id, "friday-1");
    assert_eq!(game.played_at, utc("2026-09-18T12:00:00Z"));
    let seats: Vec<(i64, &str, &str)> = game
        .seats
        .iter()
        .map(|seat| (seat.line, seat.player.as_str(), seat.result.as_str()))
        .collect();
    assert_eq!(seats, [(2, "Alice", "win"), (3, "Bob", "loss")]);
    assert_eq!(
        json!(preview.players),
        json!({"create": ["Bob"], "matched": [{"id": alice.id, "name": "Alice"}]})
    );
    assert_eq!(
        json!(preview.decks.matched),
        json!([{"id": birds.id, "player_id": alice.id, "player": "Alice", "name": "Birds", "commander": "Kangee, Sky Warden"}])
    );
    assert_eq!(
        json!(preview.decks.create),
        json!([{"player": "Bob", "name": "Goblins", "commander": "Krenko"}])
    );
}

#[tokio::test]
async fn reports_invalid_rows_with_csv_line_numbers() {
    let app = TestApp::new().await;
    let csv = CSV.replace("Goblins,Krenko,2,loss", "Goblins,Krenko,nope,victory");
    let preview = preview_csv(&app, &csv).await;
    assert!(!preview.valid);
    let errors = json!(preview.errors);
    let errors = errors.as_array().unwrap();
    assert!(
        errors.contains(
            &json!({"line": 3, "field": "seat", "message": "must be a positive integer"})
        )
    );
    assert!(
        errors.contains(
            &json!({"line": 3, "field": "result", "message": "must be win, loss, or draw"})
        )
    );
}

#[tokio::test]
async fn imports_players_decks_and_games_and_skips_the_same_normalized_game_on_reimport() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let first = import_csv(&app, CSV, Some(user.id), None).await.unwrap();
    assert_eq!((first.created, first.skipped), (1, 0));
    assert_eq!(first.game_ids.len(), 1);
    let again = import_csv(&app, CSV, Some(user.id), None).await.unwrap();
    assert_eq!((again.created, again.skipped), (0, 1));
    assert_eq!(again.game_ids, first.game_ids);

    let game = game(&app, first.game_ids[0]).await;
    assert_eq!(game.source.as_str(), "csv");
    assert_eq!(game.created_by_user_id, Some(user.id));
    let seats: Vec<(String, String, Option<String>)> = game
        .seats
        .iter()
        .map(|seat| {
            (
                seat.player.name.clone(),
                seat.deck.as_ref().unwrap().name.clone(),
                seat.mvp_card_name.clone(),
            )
        })
        .collect();
    assert_eq!(
        seats,
        [
            (
                "Alice".to_owned(),
                "Birds".to_owned(),
                Some("Swan Song".to_owned())
            ),
            ("Bob".to_owned(), "Goblins".to_owned(), None)
        ]
    );
}

#[tokio::test]
async fn reuses_a_players_deck_with_the_same_commander_even_when_the_deck_name_differs() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let alice = app.player("Alice").await;
    let birds = app
        .deck(alice.id, "Feathered friends", "Kangee, Sky Warden")
        .await;

    let preview = preview_csv(&app, CSV).await;
    assert_eq!(preview.decks.matched.len(), 1);
    assert_eq!(preview.decks.matched[0].id, birds.id);
    assert_eq!(preview.decks.matched[0].name, "Feathered friends");
    assert_eq!(
        json!(preview.decks.create),
        json!([{"player": "Bob", "name": "Goblins", "commander": "Krenko"}])
    );

    let result = import_csv(&app, CSV, Some(user.id), None).await.unwrap();
    assert_eq!(result.created, 1);
    let game = game(&app, result.game_ids[0]).await;
    let alice_seat = game
        .seats
        .iter()
        .find(|seat| seat.player.name == "Alice")
        .unwrap();
    assert_eq!(alice_seat.deck_id, Some(birds.id));
    assert_eq!(
        app.state
            .games
            .list_decks(false, Some(alice.id))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn commit_links_only_imported_rows_instead_of_running_global_repair() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    app.card("kangee", "Kangee, Sky Warden", &["W", "U"], json!({}), true)
        .await;
    app.card("krenko", "Krenko", &["R"], json!({}), true).await;
    app.card("swan-song", "Swan Song", &["U"], json!({}), false)
        .await;
    let unrelated = app.player("Unrelated").await;
    let unrelated_deck = app
        .deck(unrelated.id, "Old deck", "Kangee, Sky Warden")
        .await;

    let result = import_csv(&app, CSV, Some(user.id), None).await.unwrap();
    let game = game(&app, result.game_ids[0]).await;
    let alice = game
        .seats
        .iter()
        .find(|seat| seat.player.name == "Alice")
        .unwrap();
    let bob = game
        .seats
        .iter()
        .find(|seat| seat.player.name == "Bob")
        .unwrap();
    let alice_deck = alice.deck.as_ref().unwrap();
    assert_eq!(alice_deck.commander_card_id.as_deref(), Some("kangee"));
    assert_eq!(alice_deck.color_identity, "WU");
    assert_eq!(alice.mvp_card_id.as_deref(), Some("swan-song"));
    assert_eq!(
        bob.deck.as_ref().unwrap().commander_card_id.as_deref(),
        Some("krenko")
    );
    let unrelated_deck = app
        .state
        .games
        .get_deck(unrelated_deck.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unrelated_deck.commander_card_id, None);
}

#[tokio::test]
async fn rejects_two_winners_without_persisting_any_part_of_the_file() {
    let app = TestApp::new().await;
    let invalid = CSV.replace("Bob,Goblins,Krenko,2,loss", "Bob,Goblins,Krenko,2,win");
    let preview = validation(import_csv(&app, &invalid, Some(42), None).await);
    assert!(!preview.valid);
    assert!(preview.errors.iter().all(|error| error.field == "result"));
    assert_eq!(count(&app, "players").await, 0);
    assert_eq!(count(&app, "decks").await, 0);
    assert_eq!(count(&app, "games").await, 0);
}

#[tokio::test]
async fn accepts_the_official_mythic_track_spreadsheet_headers() {
    let app = TestApp::new().await;
    let csv = "Date,Format,Playgroup,GameName,Bracket,Platform,Player1,Player2,Player3,Player4,Player1Commander,Player2Commander,Player3Commander,Player4Commander,Player1Mulligans,Player2Mulligans,Player3Mulligans,Player4Mulligans,Winner,GameTimeMinutes,TotalTurns,WinCondition,Tags,Notes
1/2/2024,1,Friends,Game,3,1,Alice,Bob,,,Kangee,Krenko,,,,,,,Bob,88,8,11,,Imported
";
    let preview = preview_csv(&app, csv).await;
    assert!(preview.valid, "{:?}", preview.errors);
    let game = &preview.games[0];
    assert_eq!(
        (game.duration_minutes, game.turns, game.notes.as_deref()),
        (Some(88), Some(8), Some("Imported"))
    );
    let seats: Vec<(&str, &str, &str)> = game
        .seats
        .iter()
        .map(|seat| {
            (
                seat.player.as_str(),
                seat.deck.as_str(),
                seat.result.as_str(),
            )
        })
        .collect();
    assert_eq!(
        seats,
        [("Alice", "Kangee", "loss"), ("Bob", "Krenko", "win")]
    );
}

// csv_transfer_test.exs

struct TransferCtx {
    game: Game,
    alice: Player,
    bob: Player,
    alice_deck: i64,
}

async fn transfer_setup(app: &TestApp) -> TransferCtx {
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let carol = app.player("Carol").await;
    let alice_deck = app.deck(alice.id, "Birds", "Kangee").await;
    let bob_deck = app.deck(bob.id, "Goblins", "Krenko").await;
    let carol_deck = app.deck(carol.id, "Cats", "Arahbo").await;
    let created = app
        .game(
            json!({
                "played_at": "2026-09-10T12:00:00Z",
                "duration_minutes": 91,
                "turns": 13,
                "win_condition": "commander_damage",
                "notes": "Keep this note",
                "source": "mythic_track",
                "external_id": "source-17",
                "seats": [
                    {"player_id": alice.id, "deck_id": alice_deck.id, "seat": 1, "result": "win", "kills": 2,
                     "mvp_card_name": "Swan Song", "notes": "Alice note"},
                    {"player_id": bob.id, "deck_id": bob_deck.id, "seat": 2, "result": "loss", "kills": 1,
                     "eliminated_turn": 9, "eliminated_by_player_id": alice.id, "notes": "Bob note"},
                    {"player_id": carol.id, "deck_id": carol_deck.id, "seat": 3, "result": "loss", "kills": 0}
                ]
            }),
            None,
        )
        .await;
    TransferCtx {
        game: game(app, created.id).await,
        alice,
        bob,
        alice_deck: alice_deck.id,
    }
}

const TRANSFER_HEADER: &str = "game_id,date,player,deck,commander,seat,result,kills,win_condition,notes,action,source,external_id,portable_id\n";

fn update_rows(game: &Game, game_id: &str, identity: &str, conflicting: Option<&str>) -> String {
    let (source, external_id, portable_id) = match identity {
        "source" => (
            game.source.as_str().to_owned(),
            game.external_id.clone().unwrap(),
            String::new(),
        ),
        "portable_id" => (
            String::new(),
            String::new(),
            game.portable_id.clone().unwrap(),
        ),
        _ => (
            game.source.as_str().to_owned(),
            game.external_id.clone().unwrap(),
            conflicting.unwrap().to_owned(),
        ),
    };
    [
        ("Bob", "Goblins", "Krenko", 1, "loss", ""),
        ("Alice", "Angels", "Giada", 2, "win", "0"),
        ("Dave", "Dragons", "Miirym", 3, "loss", "0"),
    ]
    .iter()
    .map(|(player, deck, commander, seat, result, kills)| {
        format!(
            "{game_id},2026-09-11,{player},{deck},{commander},{seat},{result},{kills},combat_damage,,update,{source},{external_id},{portable_id}\n"
        )
    })
    .collect::<Vec<_>>()
    .concat()
}

fn update_csv(game: &Game, identity: &str, conflicting: Option<&str>) -> String {
    format!(
        "{TRANSFER_HEADER}{}",
        update_rows(game, "reviewed", identity, conflicting)
    )
}

async fn assert_invalid(app: &TestApp, csv: &str, message: &str) {
    let preview = preview_csv(app, csv).await;
    assert!(!preview.valid);
    assert!(
        preview
            .errors
            .iter()
            .any(|error| error.message.contains(message)),
        "{:?}",
        preview.errors
    );
}

async fn assert_invalid_commit(app: &TestApp, csv: &str, revision: Option<&str>, message: &str) {
    let preview = validation(import_csv(app, csv, None, revision).await);
    assert!(!preview.valid);
    assert!(
        preview
            .errors
            .iter()
            .any(|error| error.message.contains(message)),
        "{:?}",
        preview.errors
    );
}

fn change(field: &str, player: Option<&str>, before: Value, after: Value) -> Value {
    json!({"field": field, "player": player, "before": before, "after": after})
}

#[tokio::test]
async fn preview_is_a_dry_run_with_a_revision_and_material_changes() {
    let app = TestApp::new().await;
    let ctx = transfer_setup(&app).await;
    let csv = update_csv(&ctx.game, "portable_id", None);
    let players = count(&app, "players").await;
    let decks = count(&app, "decks").await;

    let preview = preview_csv(&app, &csv).await;
    assert!(preview.valid, "{:?}", preview.errors);
    assert_eq!(preview.revision.as_ref().unwrap().len(), 64);
    let review = preview.review.unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(review[0].action, "update");
    assert_eq!(review[0].target_id, Some(ctx.game.id));
    let changes = json!(review[0].changes);
    let changes = changes.as_array().unwrap();
    for expected in [
        change(
            "played_at",
            None,
            json!("2026-09-10T12:00:00Z"),
            json!("2026-09-11T12:00:00Z"),
        ),
        change(
            "win_condition",
            None,
            json!("Commander Damage"),
            json!("Combat Damage"),
        ),
        change("participant", Some("Carol"), json!("Carol"), Value::Null),
        change("participant", Some("Dave"), Value::Null, json!("Dave")),
    ] {
        assert!(changes.contains(&expected), "{expected} not in {changes:?}");
    }
    assert_eq!(count(&app, "players").await, players);
    assert_eq!(count(&app, "decks").await, decks);
    assert_eq!(count(&app, "game_players").await, 3);
}

#[tokio::test]
async fn commit_preserves_identity_and_retained_seat_metadata_while_replacing_swapping_and_changing_a_deck()
 {
    let app = TestApp::new().await;
    let ctx = transfer_setup(&app).await;
    let csv = update_csv(&ctx.game, "source", None);
    let preview = preview_csv(&app, &csv).await;
    let old: HashMap<String, i64> = ctx
        .game
        .seats
        .iter()
        .map(|seat| (seat.player.name.clone(), seat.id))
        .collect();

    let result = import_csv(&app, &csv, None, preview.revision.as_deref())
        .await
        .unwrap();
    assert_eq!(
        (result.created, result.updated, result.skipped),
        (0, Some(1), 0)
    );
    assert_eq!(result.game_ids, [ctx.game.id]);
    let saved = game(&app, ctx.game.id).await;
    assert_eq!(
        (&saved.source, &saved.external_id, &saved.portable_id),
        (
            &ctx.game.source,
            &ctx.game.external_id,
            &ctx.game.portable_id
        )
    );
    assert_eq!(saved.played_at, utc("2026-09-11T12:00:00Z"));
    assert_eq!(
        saved
            .win_condition
            .map(the_gathering::games::WinCondition::as_str),
        Some("combat_damage")
    );
    assert_eq!(
        (saved.duration_minutes, saved.turns, saved.notes.as_deref()),
        (Some(91), Some(13), Some("Keep this note"))
    );
    let seats: HashMap<String, &the_gathering::games::Seat> = saved
        .seats
        .iter()
        .map(|seat| (seat.player.name.clone(), seat))
        .collect();
    let bob = seats["Bob"];
    assert_eq!(bob.id, old["Bob"]);
    assert_eq!(bob.seat, 1);
    assert_eq!(
        (
            bob.eliminated_turn,
            bob.eliminated_by_player_id,
            bob.notes.as_deref()
        ),
        (Some(9), Some(ctx.alice.id), Some("Bob note"))
    );
    let alice = seats["Alice"];
    assert_eq!(alice.id, old["Alice"]);
    assert_eq!(
        (
            alice.seat,
            alice.kills,
            alice.mvp_card_name.as_deref(),
            alice.notes.as_deref()
        ),
        (2, Some(0), Some("Swan Song"), Some("Alice note"))
    );
    assert!(!seats.contains_key("Carol"));
    assert!(!old.values().any(|id| *id == seats["Dave"].id));
    let alice_deck = alice.deck.as_ref().unwrap();
    assert_eq!(alice_deck.name, "Angels");
    assert_eq!(alice_deck.commander_name, "Giada");
    let original = app
        .state
        .games
        .get_deck(ctx.alice_deck)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.commander_name, "Kangee");
    let _ = ctx.bob;
}

#[tokio::test]
async fn repeating_a_reviewed_update_skips_with_no_material_changes() {
    let app = TestApp::new().await;
    let ctx = transfer_setup(&app).await;
    let csv = update_csv(&ctx.game, "portable_id", None);
    let first = preview_csv(&app, &csv).await;
    let result = import_csv(&app, &csv, None, first.revision.as_deref())
        .await
        .unwrap();
    assert_eq!(result.updated, Some(1));

    let repeated = preview_csv(&app, &csv).await;
    let review = repeated.review.clone().unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(
        (
            review[0].action,
            review[0].target_id,
            review[0].changes.len()
        ),
        ("skip", Some(ctx.game.id), 0)
    );
    let before = game(&app, ctx.game.id).await;
    let again = import_csv(&app, &csv, None, repeated.revision.as_deref())
        .await
        .unwrap();
    assert_eq!((again.updated, again.skipped), (Some(0), 1));
    assert_eq!(again.game_ids, [ctx.game.id]);
    assert_eq!(game(&app, ctx.game.id).await, before);
}

async fn seat_rows(app: &TestApp) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT json_group_array(json_array(id, game_id, player_id, deck_id, seat, result, kills, eliminated_turn,
                eliminated_by_player_id, mvp_card_id, mvp_card_name, notes, inserted_at, updated_at))
         FROM (SELECT * FROM game_players ORDER BY id)",
    )
    .fetch_one(app.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn adding_only_a_win_condition_leaves_all_seat_records_untouched() {
    let app = TestApp::new().await;
    let ctx = transfer_setup(&app).await;
    sqlx::query("UPDATE game_players SET updated_at = '2020-01-01T00:00:00Z'")
        .execute(app.pool())
        .await
        .unwrap();
    let before = seat_rows(&app).await;
    let game_row = &ctx.game;
    let rows: Vec<String> = game_row
        .seats
        .iter()
        .map(|seat| {
            let deck = seat.deck.as_ref().unwrap();
            format!(
                "backfill,{},{},{},{},{},{},infinite_combo,update,{},{}",
                game_row.played_at,
                seat.player.name,
                deck.name,
                deck.commander_name,
                seat.seat,
                seat.result,
                game_row.source,
                game_row.external_id.as_deref().unwrap()
            )
        })
        .collect();
    let csv = format!(
        "game_id,date,player,deck,commander,seat,result,win_condition,action,source,external_id\n{}",
        rows.join("\n")
    );
    let preview = preview_csv(&app, &csv).await;
    assert!(preview.valid, "{:?}", preview.errors);
    let result = import_csv(&app, &csv, None, preview.revision.as_deref())
        .await
        .unwrap();
    assert_eq!(result.updated, Some(1));
    assert_eq!(seat_rows(&app).await, before);
    assert_eq!(
        game(&app, ctx.game.id)
            .await
            .win_condition
            .map(the_gathering::games::WinCondition::as_str),
        Some("infinite_combo")
    );
}

#[tokio::test]
async fn missing_conflicting_and_duplicate_update_targets_reject_atomically() {
    let app = TestApp::new().await;
    let ctx = transfer_setup(&app).await;
    let missing = update_csv(&ctx.game, "portable_id", None)
        .replace(ctx.game.portable_id.as_deref().unwrap(), "");
    assert_invalid(
        &app,
        &missing,
        "updates require portable_id or source and external_id",
    )
    .await;

    let other = app
        .game(
            json!({
                "played_at": "2026-09-01T12:00:00Z",
                "source": "csv",
                "external_id": "other",
                "seats": [
                    {"player_id": ctx.alice.id, "seat": 1, "result": "win"},
                    {"player_id": ctx.bob.id, "seat": 2, "result": "loss"}
                ]
            }),
            None,
        )
        .await;
    let conflict = update_csv(&ctx.game, "both", other.portable_id.as_deref());
    assert_invalid(
        &app,
        &conflict,
        "Game identities refer to different existing games.",
    )
    .await;

    let duplicate = format!(
        "{}{}",
        update_csv(&ctx.game, "portable_id", None),
        update_rows(&ctx.game, "second", "portable_id", None)
    );
    assert_invalid(
        &app,
        &duplicate,
        "Multiple CSV games target the same existing game.",
    )
    .await;
    assert_eq!(count(&app, "games").await, 2);
    assert_eq!(
        game(&app, ctx.game.id).await.played_at,
        utc("2026-09-10T12:00:00Z")
    );
    assert!(player_named(&app, "Dave").await.is_none());
}

#[tokio::test]
async fn database_and_csv_changes_after_preview_make_the_revision_stale_without_partial_writes() {
    let app = TestApp::new().await;
    let ctx = transfer_setup(&app).await;
    let csv = update_csv(&ctx.game, "portable_id", None);
    let preview = preview_csv(&app, &csv).await;
    app.state
        .games
        .update_game(&ctx.game, &json!({"notes": "Concurrent edit"}))
        .await
        .unwrap();

    assert_invalid_commit(
        &app,
        &csv,
        preview.revision.as_deref(),
        "Preview is stale or missing",
    )
    .await;
    assert_eq!(
        game(&app, ctx.game.id).await.notes.as_deref(),
        Some("Concurrent edit")
    );
    assert!(player_named(&app, "Dave").await.is_none());

    let fresh = preview_csv(&app, &csv).await;
    let modified = csv.replace("2026-09-11", "2026-09-12");
    assert_invalid_commit(
        &app,
        &modified,
        fresh.revision.as_deref(),
        "Preview is stale or missing",
    )
    .await;
    assert_eq!(
        game(&app, ctx.game.id).await.played_at,
        utc("2026-09-10T12:00:00Z")
    );
    assert!(player_named(&app, "Dave").await.is_none());
}

// google_sheet_test.exs

const SHEET_HEADER: &str = "Date\tWinner\tDeck\tDaniel\tDan\tJesse\tWin Con\tOther Decks\tNotes";

fn kill_counts(row: &google_sheet::SheetRow) -> Vec<(String, i64)> {
    row.kill_counts
        .iter()
        .map(|count| (count.player.clone(), count.kills))
        .collect()
}

fn names(row: &google_sheet::SheetRow) -> Vec<&str> {
    row.seats.iter().map(|seat| seat.player.as_str()).collect()
}

#[test]
fn parses_pasted_tsv_dates_notes_aliases_and_exact_kill_attribution() {
    let payload = format!(
        "K I L L S\n{SHEET_HEADER}\n1/2/25\tDaniel\tTidus\t0\t\t\tCombat\tMatt (Chatterfang); Drew (Merieke)\tcomma, slash / kept\n\
         2025-01-03\tDan\tFaramir\t\t\t3\tCombo\tDaniel (Flubs), Jesse (Karlov)\tMisplaced kill\n"
    );
    let rows = google_sheet::parse(&payload).unwrap();
    let [first, second] = rows.as_slice() else {
        panic!("two rows")
    };
    assert_eq!(first.line, 3);
    assert_eq!(first.date.unwrap().to_string(), "2025-01-02");
    assert_eq!(first.notes, "comma, slash / kept");
    let zero = |name: &str| (name.to_owned(), 0);
    assert_eq!(
        kill_counts(first),
        [zero("Daniel"), zero("Dan"), zero("Jesse")]
    );
    assert_eq!(first.seats[0].kills, 0);
    assert_eq!(first.seats[1].kills, 0);
    assert_eq!(second.date.unwrap().to_string(), "2025-01-03");
    assert_eq!(
        kill_counts(second),
        [zero("Daniel"), zero("Dan"), ("Jesse".to_owned(), 3)]
    );
    assert_eq!(
        second
            .seats
            .iter()
            .find(|seat| seat.player == "Jesse")
            .unwrap()
            .kills,
        3
    );
    assert!(!second.errors.iter().any(|error| error.contains("header")));
    assert_ne!(first.key, second.key);
}

#[test]
fn parses_csv_with_quoted_commas_and_all_supported_other_deck_separators() {
    let payload = "Date,Winner,Deck,Drew,Win Con,Other Decks,Notes
9/21/2025,Drew,Gisela,1,Combat,\"Matt (Agatha), Dan (Faramir)\",\"notes, commas/slashes\"
9/22/25,Drew,Gisela,,Combat,Drewson (Karlov) Matt (Tidus),ok
9/23/25,Drew,Gisela,,Combat,Drewson(Gisela); ,ok
";
    let rows = google_sheet::parse(payload).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].notes, "notes, commas/slashes");
    assert_eq!(names(&rows[0]), ["Drew", "Matt", "Dan"]);
    assert_eq!(names(&rows[1]), ["Drew", "Drewson", "Matt"]);
    assert_eq!(names(&rows[2]), ["Drew", "Drewson"]);
}

#[test]
fn reports_row_problems_without_dropping_malformed_residue() {
    let payload = "Date\tWinner\tDeck\tDan\tWin Con\tOther Decks\tNotes
nope\tDaniel\t\t-1\t\tRealty (Kenrith; Landon (Flubs)\t
1/2/25\tDaniel\tTidus\t3\t\tMatt (A); Drew (B)\t
1/2/25\tDaniel\tTidus\t\t\tDaniel (Other)\t
";
    let rows = google_sheet::parse(payload).unwrap();
    let [malformed, overflow, duplicate] = rows.as_slice() else {
        panic!("three rows")
    };
    let has =
        |row: &google_sheet::SheetRow, text: &str| row.errors.iter().any(|e| e.contains(text));
    assert!(has(malformed, "malformed text"));
    assert!(has(malformed, "missing a deck"));
    assert!(has(malformed, "nonnegative integer"));
    assert!(malformed.errors.contains(&"Date is invalid.".to_owned()));
    assert!(has(overflow, "exceed"));
    assert!(
        duplicate
            .errors
            .contains(&"A raw player is listed more than once.".to_owned())
    );
}

#[test]
fn missing_opponents_errors_and_blank_or_na_winners_create_draws() {
    let payload = "Date\tWinner\tDeck\tWin Con\tOther Decks\tNotes
1/1/25\tDrew\tKarlov\t\t\tOnly winner
1/2/25\tN/A\t\t\tMatt (A); Drew (B)\tDraw
1/3/25\t\t\t\tMatt (A); Drew (B)\tDraw
";
    let rows = google_sheet::parse(payload).unwrap();
    assert!(
        rows[0]
            .errors
            .iter()
            .any(|error| error.contains("Other Decks"))
    );
    for row in &rows[1..] {
        assert!(row.seats.iter().all(|seat| seat.result == "draw"));
        assert_eq!(
            row.warnings,
            [
                "No winner: all listed players will be recorded as a draw. Notes do not change results."
            ]
        );
    }
}

#[test]
fn same_date_games_differ_by_complete_row_and_exact_duplicates_get_occurrence_keys() {
    let row = "1/2/25\tDaniel\tTidus\t\tCombat\tMatt (A)\tFine";
    let other = "1/2/25\tMatt\tA\t\tCombat\tDaniel (Tidus)\tFine";
    let rows = google_sheet::parse(&[SHEET_HEADER, row, other, row].join("\n")).unwrap();
    let [first, second, duplicate] = rows.as_slice() else {
        panic!("three rows")
    };
    assert_ne!(first.key, second.key);
    assert_eq!(duplicate.key, format!("{}-2", first.key));
    assert!(
        duplicate
            .warnings
            .iter()
            .any(|w| w.contains("Duplicate row"))
    );
}

#[test]
fn rejects_unparseable_files_and_incomplete_headers() {
    let message = google_sheet::parse("Date,Winner,Deck\n1/2/25,Drew,A").unwrap_err();
    assert!(message.contains("header is missing"));
    assert!(google_sheet::parse("not a sheet").is_err());
}

#[test]
fn plain_pasted_quotes_survive_but_malformed_quoted_exports_are_rejected() {
    let header = "Date\tWinner\tDeck\tWin Con\tOther Decks\tNotes\n";
    let rows = google_sheet::parse(&format!(
        "{header}2/21/26\tDrew\tTifa\tSwing Out\tDan (Voja); Matt (Cloud)\tTifa said \"It's Tifa'ing time\"\n"
    ))
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].notes, "Tifa said \"It's Tifa'ing time\"");
    assert!(rows[0].errors.is_empty());
    assert!(
        google_sheet::parse(&format!(
            "{header}2/21/26\tDrew\tTifa\tSwing Out\tMatt (Cloud)\t\"Unclosed quote\n"
        ))
        .is_err()
    );
}

// mythic_track_test.exs

const DREW_DISCORD: &str = "200000000000000002";

fn mythic_seat(
    name: &str,
    turn_order: Value,
    winner: bool,
    commander: &str,
    extra: &Value,
) -> Value {
    let mut commander_json = json!({
        "scryfallId": format!("sf-{}", commander.to_lowercase()),
        "name": commander,
        "colors": ["R"],
        "decklistUrl": "",
        "deckName": null
    });
    for (key, value) in extra.as_object().unwrap() {
        if key != "discordUserId" {
            commander_json[key] = value.clone();
        }
    }
    json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "player": {"id": uuid::Uuid::new_v4().to_string(), "name": name, "discordUserId": extra.get("discordUserId")},
        "commander": commander_json,
        "commanderPartner": null,
        "turnOrder": turn_order,
        "mulligans": 0,
        "isWinner": winner
    })
}

fn mythic_game(overrides: Value) -> Value {
    let mut game = json!({
        "id": "8f3a0a44-0000-4000-8000-000000000001",
        "name": "",
        "notes": "Close one",
        "createdOn": "2026-03-14T19:30:15.123456",
        "gameStatus": 3,
        "gameType": 1,
        "totalTurns": 9,
        "gameTimeInMinutes": 55,
        "winCondition": 10,
        "players": [
            // Listed out of turn order on purpose: seats must follow turnOrder.
            mythic_seat("Drew", json!(2), false, "Krenko, Mob Boss", &json!({"discordUserId": DREW_DISCORD})),
            mythic_seat("Daniel", json!(1), true, "Tifa Lockhart", &json!({"deckName": "Tifa Punches", "colors": ["G"]})),
            mythic_seat("Kaylyn", Value::Null, false, "Éowyn, Shieldmaiden", &json!({}))
        ]
    });
    for (key, value) in overrides.as_object().unwrap() {
        game[key] = value.clone();
    }
    game
}

fn players_with(update: impl Fn(&mut Value)) -> Value {
    let mut players = mythic_game(json!({}))["players"].clone();
    for player in players.as_array_mut().unwrap() {
        update(player);
    }
    players
}

async fn mythic_preview(app: &TestApp, games: &Value) -> Preview {
    let mut conn = app.pool().acquire().await.unwrap();
    preview::run(&mut conn, Source::MythicTrack, &games.to_string())
        .await
        .unwrap()
}

async fn mythic_preview_raw(app: &TestApp, payload: &str) -> Preview {
    let mut conn = app.pool().acquire().await.unwrap();
    preview::run(&mut conn, Source::MythicTrack, payload)
        .await
        .unwrap()
}

async fn mythic_import(app: &TestApp, games: &Value, user_id: i64) -> ImportResult {
    commit::run(
        &app.state,
        Source::MythicTrack,
        &games.to_string(),
        Some(user_id),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn orders_seats_by_turn_order_derives_results_decks_colours_and_notes() {
    let app = TestApp::new().await;
    let preview = mythic_preview(&app, &json!([mythic_game(json!({}))])).await;
    assert!(preview.valid);
    assert!(preview.warnings.is_empty());
    let parsed = &preview.games[0];
    assert_eq!(parsed.external_id, "8f3a0a44-0000-4000-8000-000000000001");
    assert_eq!(parsed.played_at, utc("2026-03-14T19:30:15Z"));
    assert_eq!((parsed.turns, parsed.duration_minutes), (Some(9), Some(55)));
    assert_eq!(parsed.win_condition.as_deref(), Some("combat_damage"));
    assert_eq!(parsed.notes.as_deref(), Some("Close one"));
    let seats: Vec<(i64, &str, &str, &str)> = parsed
        .seats
        .iter()
        .map(|seat| {
            (
                seat.seat,
                seat.player.as_str(),
                seat.result.as_str(),
                seat.deck.as_str(),
            )
        })
        .collect();
    assert_eq!(
        seats,
        [
            (1, "Daniel", "win", "Tifa Punches"),
            (2, "Drew", "loss", "Krenko, Mob Boss"),
            (3, "Kaylyn", "loss", "Éowyn, Shieldmaiden")
        ]
    );
    let daniel = &parsed.seats[0];
    assert_eq!(
        daniel.commander_card_id.as_deref(),
        Some("sf-tifa lockhart")
    );
    assert_eq!(daniel.color_identity.as_deref(), Some("G"));
    assert_eq!(parsed.seats[1].discord_id.as_deref(), Some(DREW_DISCORD));
    assert_eq!(preview.players.create, ["Daniel", "Drew", "Kaylyn"]);
}

#[tokio::test]
async fn maps_every_mythic_track_win_condition_and_defaults_unrecognized_values_to_unknown() {
    let app = TestApp::new().await;
    let expected = [
        "damage",
        "infinite_combo",
        "mill",
        "poison",
        "alternate_win_con",
        "hard_lock",
        "commander_damage",
        "draw",
        "non_combat_damage",
        "combat_damage",
        "concede",
    ];
    for (key, number) in expected.iter().zip(1..) {
        let preview =
            mythic_preview(&app, &json!([mythic_game(json!({"winCondition": number}))])).await;
        assert_eq!(preview.games[0].win_condition.as_deref(), Some(*key));
    }
    for value in [json!(99), json!(0), json!(12), Value::Null, json!("10")] {
        let preview =
            mythic_preview(&app, &json!([mythic_game(json!({"winCondition": value}))])).await;
        assert_eq!(preview.games[0].win_condition.as_deref(), Some("unknown"));
    }
}

#[tokio::test]
async fn normalizes_naive_midnight_calendar_dates_to_noon_utc_and_preserves_real_timestamps() {
    let app = TestApp::new().await;
    for (created_on, expected) in [
        ("2025-03-17T00:00:00", "2025-03-17T12:00:00Z"),
        ("2025-03-17", "2025-03-17T12:00:00Z"),
        ("2025-03-17T00:00:01", "2025-03-17T00:00:01Z"),
        ("2025-03-17T00:00:00-04:00", "2025-03-17T04:00:00Z"),
    ] {
        let preview = mythic_preview(
            &app,
            &json!([mythic_game(json!({"createdOn": created_on}))]),
        )
        .await;
        assert_eq!(preview.games[0].played_at, utc(expected), "{created_on}");
    }
}

#[tokio::test]
async fn a_game_with_no_winner_is_an_all_player_draw_and_two_winners_is_skipped() {
    let app = TestApp::new().await;
    let draw = mythic_game(json!({"players": players_with(|p| p["isWinner"] = json!(false))}));
    let preview = mythic_preview(&app, &json!([draw])).await;
    assert!(preview.valid);
    let results: Vec<&str> = preview.games[0]
        .seats
        .iter()
        .map(|seat| seat.result.as_str())
        .collect();
    assert_eq!(results, ["draw", "draw", "draw"]);

    let two_winners = mythic_game(json!({
        "id": "8f3a0a44-0000-4000-8000-000000000002",
        "name": "Friday pod",
        "players": players_with(|p| p["isWinner"] = json!(true))
    }));
    let preview = mythic_preview(&app, &json!([mythic_game(json!({})), two_winners])).await;
    // The admin cannot fix the export here, so the good game stays importable.
    assert!(preview.valid);
    assert!(preview.errors.is_empty());
    assert_eq!(preview.warnings.len(), 1);
    assert_eq!(preview.warnings[0].line, 2);
    let message = &preview.warnings[0].message;
    assert!(message.contains("more than one player is marked as the winner"));
    assert!(message.contains("Friday pod, 2026-03-14, players: Drew, Daniel, Kaylyn"));
    assert_eq!(preview.games.len(), 1);
}

#[tokio::test]
async fn a_player_listed_twice_skips_the_game_and_names_the_player() {
    let app = TestApp::new().await;
    let players = mythic_game(json!({}))["players"].clone();
    let mut twice = players.as_array().unwrap().clone();
    let mut daniel = twice[1].clone();
    daniel["turnOrder"] = json!(4);
    twice.push(daniel);
    let preview = mythic_preview(&app, &json!([mythic_game(json!({"players": twice}))])).await;
    assert!(preview.valid);
    assert!(preview.games.is_empty());
    assert_eq!(preview.warnings.len(), 1);
    assert_eq!(preview.warnings[0].line, 1);
    assert!(
        preview.warnings[0]
            .message
            .starts_with("skipped: Daniel is listed twice (")
    );
}

#[tokio::test]
async fn skips_in_progress_games_with_a_warning_and_names_partner_decks() {
    let app = TestApp::new().await;
    let partner = json!({"scryfallId": "sf-bg", "name": "Candlekeep Sage", "colors": ["U"]});
    let mut players = mythic_game(json!({}))["players"].clone();
    players[1]["commanderPartner"] = partner;
    let in_progress =
        mythic_game(json!({"id": "8f3a0a44-0000-4000-8000-000000000003", "gameStatus": 2}));
    let preview = mythic_preview(
        &app,
        &json!([
            mythic_game(json!({"players": players.clone()})),
            in_progress
        ]),
    )
    .await;
    assert!(preview.valid);
    assert_eq!(
        json!(preview.warnings),
        json!([{"line": 2, "message": "skipped: game is in progress (2026-03-14, players: Drew, Daniel, Kaylyn)"}])
    );
    let daniel = &preview.games[0].seats[0];
    assert_eq!(daniel.deck, "Tifa Punches");
    assert_eq!(daniel.partner_name.as_deref(), Some("Candlekeep Sage"));
    assert_eq!(daniel.partner_card_id.as_deref(), Some("sf-bg"));
    assert_eq!(daniel.color_identity.as_deref(), Some("UG"));

    // Without a Mythic Track deck name, a partner deck is "Commander / Partner".
    let mut unnamed = players.clone();
    unnamed[1]["commander"]["deckName"] = json!("");
    let preview = mythic_preview(&app, &json!([mythic_game(json!({"players": unnamed}))])).await;
    assert_eq!(
        preview.games[0].seats[0].deck,
        "Tifa Lockhart / Candlekeep Sage"
    );

    // Mythic Track also writes partners as "A || B (Partners)" in the commander name.
    let mut piped = mythic_game(json!({}))["players"].clone();
    piped[1]["commander"]["name"] =
        json!("Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)");
    piped[1]["commander"]["deckName"] = json!("");
    let preview = mythic_preview(&app, &json!([mythic_game(json!({"players": piped}))])).await;
    let piped_daniel = &preview.games[0].seats[0];
    assert_eq!(piped_daniel.commander, "Frodo, Adventurous Hobbit");
    assert_eq!(
        piped_daniel.partner_name.as_deref(),
        Some("Sam, Loyal Attendant")
    );
    assert_eq!(
        piped_daniel.deck,
        "Frodo, Adventurous Hobbit / Sam, Loyal Attendant"
    );

    let solo = mythic_game(json!({"players": [players[1].clone()]}));
    let preview = mythic_preview(&app, &json!([solo])).await;
    assert_eq!(preview.warnings[0].line, 1);
    assert!(
        preview.warnings[0]
            .message
            .starts_with("skipped: needs between 2 and 6 players, has 1 (")
    );

    let preview = mythic_preview(&app, &json!([mythic_game(json!({"id": ""}))])).await;
    assert_eq!(preview.errors.len(), 1);
    assert_eq!(
        (preview.errors[0].line, preview.errors[0].field.as_str()),
        (1, "id")
    );
}

#[tokio::test]
async fn rejects_payloads_that_are_not_a_game_array() {
    let app = TestApp::new().await;
    for payload in ["{\"nope\": 1}", "not json", "[]"] {
        let preview = mythic_preview_raw(&app, payload).await;
        assert_eq!(preview.errors.len(), 1, "{payload}");
        assert_eq!(
            (preview.errors[0].line, preview.errors[0].field.as_str()),
            (1, "json")
        );
    }
}

#[tokio::test]
async fn imports_with_scryfall_ids_merges_discord_identities_and_skips_the_same_guid() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    // Drew already exists from the Discord bot under a different display name.
    let drew = app
        .state
        .games
        .resolve_player("waxpoetik", Some(DREW_DISCORD), None)
        .await
        .unwrap();
    let other_daniel = app.player("daniel").await;

    let payload = json!([mythic_game(json!({}))]);
    let preview = mythic_preview(&app, &payload).await;
    assert_eq!(preview.players.create, ["Kaylyn"]);
    let mut matched: Vec<i64> = preview.players.matched.iter().map(|p| p.id).collect();
    matched.sort_unstable();
    let mut expected = vec![drew.id, other_daniel.id];
    expected.sort_unstable();
    assert_eq!(matched, expected);

    let result = mythic_import(&app, &payload, user.id).await;
    assert_eq!((result.created, result.skipped), (1, 0));
    let imported = game(&app, result.game_ids[0]).await;
    assert_eq!(imported.source.as_str(), "mythic_track");
    assert_eq!(
        imported.external_id.as_deref(),
        Some("8f3a0a44-0000-4000-8000-000000000001")
    );
    let mut seats = imported.seats.clone();
    seats.sort_by_key(|seat| seat.seat);
    let names: Vec<&str> = seats.iter().map(|seat| seat.player.name.as_str()).collect();
    assert_eq!(names, ["daniel", "waxpoetik", "Kaylyn"]);
    let results: Vec<GameResult> = seats.iter().map(|seat| seat.result).collect();
    assert_eq!(
        results,
        [GameResult::Win, GameResult::Loss, GameResult::Loss]
    );
    let tifa = seats[0].deck.as_ref().unwrap();
    assert_eq!(tifa.name, "Tifa Punches");
    assert_eq!(tifa.commander_card_id.as_deref(), Some("sf-tifa lockhart"));
    assert_eq!(tifa.color_identity, "G");
    assert!(player_by_discord(&app, DREW_DISCORD).await.is_some());

    let again = mythic_import(&app, &payload, user.id).await;
    assert_eq!((again.created, again.skipped), (0, 1));
    assert_eq!(again.game_ids, result.game_ids);
    assert_eq!(count(&app, "games").await, 1);
}

#[tokio::test]
async fn commit_preserves_partner_name_card_id_and_combined_colors() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let mut players = mythic_game(json!({}))["players"].clone();
    players[1]["commanderPartner"] =
        json!({"scryfallId": "sf-candlekeep", "name": "Candlekeep Sage", "colors": ["U"]});
    let result = mythic_import(
        &app,
        &json!([mythic_game(json!({"players": players}))]),
        user.id,
    )
    .await;
    let imported = game(&app, result.game_ids[0]).await;
    let winner = imported.winner().unwrap().deck.as_ref().unwrap();
    assert_eq!(winner.partner_name.as_deref(), Some("Candlekeep Sage"));
    assert_eq!(winner.partner_card_id.as_deref(), Some("sf-candlekeep"));
    assert_eq!(winner.color_identity, "UG");
}

#[tokio::test]
async fn preview_and_import_create_a_distinct_player_for_a_conflicting_discord_identity() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let existing_alice = app
        .player_with(
            json!({"name": "Alice", "discord_id": "discord-alice-a"}),
            None,
        )
        .await;
    let mut players = mythic_game(json!({}))["players"].clone();
    players[0]["player"]["name"] = json!("Alice");
    players[0]["player"]["discordUserId"] = json!("discord-alice-b");
    let payload = json!([mythic_game(json!({"players": players}))]);
    let preview = mythic_preview(&app, &payload).await;
    assert!(preview.players.create.contains(&"Alice (2)".to_owned()));
    assert!(
        !preview
            .players
            .matched
            .iter()
            .any(|p| p.id == existing_alice.id)
    );

    let result = mythic_import(&app, &payload, user.id).await;
    assert_eq!(result.created, 1);
    let imported_alice = player_by_discord(&app, "discord-alice-b").await.unwrap();
    assert_eq!(
        player_named(&app, "Alice (2)").await.unwrap().id,
        imported_alice
    );
    let imported = game(&app, result.game_ids[0]).await;
    assert!(
        imported
            .seats
            .iter()
            .any(|seat| seat.player_id == imported_alice)
    );
    assert!(
        !imported
            .seats
            .iter()
            .any(|seat| seat.player_id == existing_alice.id)
    );
}

#[tokio::test]
async fn matching_discord_identity_wins_when_the_imported_display_name_changed() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let existing = app
        .player_with(
            json!({"name": "Original Discord Name", "discord_id": DREW_DISCORD}),
            None,
        )
        .await;
    let payload = json!([mythic_game(json!({}))]);
    let preview = mythic_preview(&app, &payload).await;
    assert!(preview.players.matched.iter().any(|p| p.id == existing.id));
    assert!(!preview.players.create.contains(&"Drew".to_owned()));
    let result = mythic_import(&app, &payload, user.id).await;
    let imported = game(&app, result.game_ids[0]).await;
    assert!(
        imported
            .seats
            .iter()
            .any(|seat| seat.player_id == existing.id)
    );
}

#[tokio::test]
async fn links_the_first_key_card_to_the_winner_as_mvp_and_keeps_the_rest_in_notes() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let key_cards = json!([
        {"scryfallId": "sf-craterhoof", "name": "Craterhoof Behemoth", "colors": ["G"]},
        {"scryfallId": null, "name": "Finale of Devastation", "colors": ["G"]}
    ]);
    let payload = json!([mythic_game(json!({"keyCards": key_cards}))]);
    let preview = mythic_preview(&app, &payload).await;
    assert!(preview.valid);
    let parsed = &preview.games[0];
    // Daniel (seat 1) won; the losers must not receive an MVP card.
    let mvps: Vec<(&str, Option<&str>, Option<&str>)> = parsed
        .seats
        .iter()
        .map(|seat| {
            (
                seat.result.as_str(),
                seat.mvp_card.as_deref(),
                seat.mvp_card_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        mvps,
        [
            ("win", Some("Craterhoof Behemoth"), Some("sf-craterhoof")),
            ("loss", None, None),
            ("loss", None, None)
        ]
    );
    assert_eq!(
        parsed.notes.as_deref(),
        Some("Close one\nKey cards: Finale of Devastation")
    );

    let result = mythic_import(&app, &payload, user.id).await;
    assert_eq!(result.created, 1);
    let imported = game(&app, result.game_ids[0]).await;
    let winner = imported.winner().unwrap();
    assert_eq!(winner.mvp_card_name.as_deref(), Some("Craterhoof Behemoth"));
    assert_eq!(winner.mvp_card_id.as_deref(), Some("sf-craterhoof"));

    // Without a winner there is no seat to link, so every key card stays in notes.
    let draw = mythic_game(json!({
        "id": "8f3a0a44-0000-4000-8000-000000000009",
        "keyCards": key_cards,
        "players": players_with(|p| p["isWinner"] = json!(false))
    }));
    let preview = mythic_preview(&app, &json!([draw])).await;
    let drawn = &preview.games[0];
    assert!(drawn.seats.iter().all(|seat| seat.mvp_card.is_none()));
    assert_eq!(
        drawn.notes.as_deref(),
        Some("Close one\nKey cards: Craterhoof Behemoth, Finale of Devastation")
    );
}

// portable_transfer_test.exs

struct PortableCtx {
    first: Game,
    second: Game,
    imported: Game,
    alice: Player,
    deck: i64,
}

async fn portable_setup(app: &TestApp) -> PortableCtx {
    let user = app.unique_member().await;
    let alice = app
        .player_with(
            json!({"name": "Alice", "discord_id": "private-discord"}),
            Some(user.id),
        )
        .await;
    let bob = app.player("Bob").await;
    let carol = app.player("Carol").await;
    let archived = app
        .player_with(
            json!({"name": "Retired", "archived_at": "2025-01-01T00:00:00Z"}),
            None,
        )
        .await;
    app.catalog_card(json!({
        "id": "card-identity",
        "oracle_id": "oracle-commander",
        "name": "Karn, Silver Golem",
        "set": "tst",
        "collector_number": "1",
        "type_line": "Legendary Artifact Creature — Golem",
        "rarity": "rare"
    }))
    .await;
    app.printing(
        "chosen-art",
        "commander",
        "Karn, Silver Golem",
        json!({"art_crop": "https://example.com/karn.jpg"}),
    )
    .await;
    let deck = app
        .deck_with(json!({
            "player_id": alice.id,
            "name": "Metal friends",
            "commander_name": "Karn, Silver Golem",
            "commander_card_id": "card-identity",
            "commander_printing_id": "chosen-art",
            "decklist_url": "https://moxfield.com/decks/example",
            "included_for_play": false,
            "archived_at": "2025-02-01T00:00:00Z"
        }))
        .await;
    sqlx::query("UPDATE decks SET skip_count = 4 WHERE id = ?")
        .bind(deck.id)
        .execute(app.pool())
        .await
        .unwrap();
    app.deck(archived.id, "Unused", "Karn, Silver Golem").await;
    let attrs = json!({
        "played_at": "2025-03-17T21:04:00Z",
        "turns": 11,
        "duration_minutes": 83,
        "win_condition": "combat_damage",
        "notes": "Line one\nLine two",
        "seats": [
            {"player_id": bob.id, "seat": 1, "result": "loss", "kills": null, "eliminated_turn": 8,
             "eliminated_by_player_id": alice.id, "notes": "Seat note"},
            {"player_id": alice.id, "deck_id": deck.id, "seat": 2, "result": "win", "kills": 2,
             "mvp_card_name": "Karn, Silver Golem", "mvp_card_id": "card-identity"},
            {"player_id": carol.id, "seat": 3, "result": "loss", "kills": 0}
        ]
    });
    let first = app.game(attrs.clone(), Some(user.id)).await;
    let second = app.game(attrs.clone(), Some(user.id)).await;
    let mut imported_attrs = attrs;
    imported_attrs["source"] = json!("mythic_track");
    imported_attrs["external_id"] = json!("original-game");
    let imported = app.game(imported_attrs, None).await;
    sqlx::query(
        "INSERT INTO sheet_import_receipts (key, game_id) VALUES ('reviewed-sheet-row', ?)",
    )
    .bind(first.id)
    .execute(app.pool())
    .await
    .unwrap();
    PortableCtx {
        first,
        second,
        imported,
        alice,
        deck: deck.id,
    }
}

async fn clear_history(app: &TestApp) {
    for table in [
        "sheet_import_receipts",
        "game_players",
        "games",
        "decks",
        "players",
        "card_printings",
        "cards",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table}")))
            .execute(app.pool())
            .await
            .unwrap();
    }
}

async fn export(app: &TestApp) -> Value {
    portable::export(&app.state).await.unwrap()
}

async fn import_portable(app: &TestApp, json: &str) -> Result<portable::Summary, ImportError> {
    portable::run(&app.state, json, None).await
}

async fn game_by_portable_id(app: &TestApp, portable_id: &str) -> Game {
    let id = sqlx::query_scalar::<_, i64>("SELECT id FROM games WHERE portable_id = ?")
        .bind(portable_id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    game(app, id).await
}

#[tokio::test]
async fn round_trip_preserves_gameplay_unused_records_art_and_receipts_with_different_local_ids() {
    let app = TestApp::new().await;
    let ctx = portable_setup(&app).await;
    let data = export(&app).await;
    let json = data.to_string();
    assert!(!json.contains("private-discord"));
    assert!(!json.contains("user_id"));
    assert!(!json.contains("created_by"));
    assert_eq!(data["games"].as_array().unwrap().len(), 3);
    assert_ne!(ctx.first.portable_id, ctx.second.portable_id);
    clear_history(&app).await;
    app.player("Unrelated").await;

    let summary = portable::preview(&app.state, &json).await.unwrap();
    assert_eq!(
        (
            summary.players.created,
            summary.decks.created,
            summary.games.created
        ),
        (4, 2, 3)
    );
    assert_eq!(count(&app, "players").await, 1);
    assert_eq!(count(&app, "games").await, 0);
    assert_eq!(count(&app, "cards").await, 0);
    let summary = import_portable(&app, &json).await.unwrap();
    assert_eq!(summary.games.created, 3);

    let alice = player_named(&app, "Alice").await.unwrap();
    assert_ne!(alice.id, ctx.alice.id);
    assert_eq!(alice.user_id, None);
    assert_eq!(alice.discord_id, None);

    let saved = game_by_portable_id(&app, ctx.first.portable_id.as_deref().unwrap()).await;
    assert_eq!(saved.played_at, utc("2025-03-17T21:04:00Z"));
    assert_eq!(
        (
            saved.duration_minutes,
            saved.turns,
            saved
                .win_condition
                .map(the_gathering::games::WinCondition::as_str),
            saved.notes.as_deref()
        ),
        (
            Some(83),
            Some(11),
            Some("combat_damage"),
            Some("Line one\nLine two")
        )
    );
    let mut seats = saved.seats.clone();
    seats.sort_by_key(|seat| seat.seat);
    let summary_seats: Vec<(&str, GameResult, Option<i64>)> = seats
        .iter()
        .map(|seat| (seat.player.name.as_str(), seat.result, seat.kills))
        .collect();
    assert_eq!(
        summary_seats,
        [
            ("Bob", GameResult::Loss, None),
            ("Alice", GameResult::Win, Some(2)),
            ("Carol", GameResult::Loss, Some(0))
        ]
    );
    assert_eq!(seats[0].eliminated_by_player_id, Some(alice.id));
    assert_eq!(seats[0].eliminated_turn, Some(8));
    assert_eq!(seats[0].notes.as_deref(), Some("Seat note"));
    assert_eq!(seats[1].mvp_card_id.as_deref(), Some("card-identity"));
    let deck = seats[1].deck.as_ref().unwrap();
    assert_eq!(
        (
            deck.name.as_str(),
            deck.commander_printing_id.as_deref(),
            deck.skip_count,
            deck.included_for_play
        ),
        ("Metal friends", Some("chosen-art"), 4, false)
    );
    assert_eq!(deck.archived_at, Some(utc("2025-02-01T00:00:00Z")));
    assert_eq!(
        deck.decklist_source
            .map(the_gathering::games::DecklistSource::as_str),
        Some("moxfield")
    );
    assert_eq!(
        player_named(&app, "Retired").await.unwrap().archived_at,
        Some(utc("2025-01-01T00:00:00Z"))
    );
    let receipt = sqlx::query_scalar::<_, i64>(
        "SELECT game_id FROM sheet_import_receipts WHERE key = 'reviewed-sheet-row'",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(receipt, saved.id);
    let printing = the_gathering::catalog::Catalog {
        pool: app.pool().clone(),
    }
    .get_printing("chosen-art")
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        printing.image_uris["art_crop"],
        "https://example.com/karn.jpg"
    );
    assert_eq!(
        game_by_portable_id(&app, ctx.imported.portable_id.as_deref().unwrap())
            .await
            .external_id
            .as_deref(),
        Some("original-game")
    );

    app.state
        .games
        .update_game(&saved, &json!({"notes": "Edited on destination"}))
        .await
        .unwrap();
    let summary = import_portable(&app, &json).await.unwrap();
    assert_eq!((summary.games.created, summary.games.reused), (0, 3));
    let reexport = export(&app).await;
    let summary = import_portable(&app, &reexport.to_string()).await.unwrap();
    assert_eq!((summary.games.created, summary.games.reused), (0, 3));
    assert_eq!(
        game(&app, saved.id).await.notes.as_deref(),
        Some("Edited on destination")
    );
    assert_eq!(count(&app, "games").await, 3);
}

#[tokio::test]
async fn malformed_and_unsupported_files_reject_without_writes() {
    let app = TestApp::new().await;
    portable_setup(&app).await;
    let data = export(&app).await;
    clear_history(&app).await;
    let with = |key: &str, value: Value| {
        let mut changed = data.clone();
        changed[key] = value;
        changed.to_string()
    };
    let mut doubled = data["players"].as_array().unwrap().clone();
    doubled.extend(data["players"].as_array().unwrap().clone());
    for invalid in [
        "bad".to_owned(),
        "[]".to_owned(),
        with("version", json!(99)),
        with("games", json!([{"seats": null}])),
        with("players", Value::Array(doubled)),
    ] {
        assert!(portable::preview(&app.state, &invalid).await.is_err());
        assert!(import_portable(&app, &invalid).await.is_err());
    }
    assert_eq!(count(&app, "players").await, 0);
}

#[tokio::test]
async fn source_identities_recognize_independent_imports_and_conflicting_identities_block() {
    let app = TestApp::new().await;
    let ctx = portable_setup(&app).await;
    let data = export(&app).await;
    let mut independent = data.clone();
    independent["games"][2]["portable_id"] = json!(uuid::Uuid::new_v4().to_string());
    let summary = import_portable(&app, &independent.to_string())
        .await
        .unwrap();
    assert_eq!((summary.games.created, summary.games.reused), (0, 3));
    assert_eq!(
        game(&app, ctx.imported.id).await.portable_id,
        ctx.imported.portable_id
    );

    let mut conflict = data.clone();
    conflict["games"][0]["source"] = json!("mythic_track");
    conflict["games"][0]["external_id"] = json!("original-game");
    match import_portable(&app, &conflict.to_string()).await {
        Err(ImportError::Message(message)) => {
            assert_eq!(
                message,
                "Game identities refer to different existing games."
            );
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(count(&app, "games").await, 3);
}

#[tokio::test]
async fn null_deck_selection_settings_fail_validation_rather_than_database_constraints() {
    let app = TestApp::new().await;
    portable_setup(&app).await;
    let data = export(&app).await;
    clear_history(&app).await;
    for field in ["skip_count", "included_for_play"] {
        let mut invalid = data.clone();
        invalid["decks"][0][field] = Value::Null;
        assert!(import_portable(&app, &invalid.to_string()).await.is_err());
        assert_eq!(count(&app, "players").await, 0);
    }
}

#[tokio::test]
async fn bad_references_invalid_kills_and_deck_ownership_roll_back_the_entire_file() {
    let app = TestApp::new().await;
    portable_setup(&app).await;
    let data = export(&app).await;
    clear_history(&app).await;
    let first_deck = data["decks"][0]["id"].clone();
    for changes in [
        json!({"player_id": 999_999}),
        json!({"kills": -1}),
        json!({"deck_id": first_deck}),
    ] {
        let mut bad = data.clone();
        for (key, value) in changes.as_object().unwrap() {
            bad["games"][2]["seats"][0][key] = value.clone();
        }
        assert!(import_portable(&app, &bad.to_string()).await.is_err());
        assert_eq!(count(&app, "games").await, 0);
        assert_eq!(count(&app, "players").await, 0);
        assert_eq!(count(&app, "card_printings").await, 0);
    }
}

#[tokio::test]
async fn same_name_commander_conflict_blocks_instead_of_replacing_destination_metadata() {
    let app = TestApp::new().await;
    let ctx = portable_setup(&app).await;
    let data = export(&app).await;
    let deck = app.state.games.get_deck(ctx.deck).await.unwrap().unwrap();
    app.state
        .games
        .update_deck(
            &deck,
            &json!({"commander_name": "Other Commander", "commander_card_id": null}),
        )
        .await
        .unwrap();
    match import_portable(&app, &data.to_string()).await {
        Err(ImportError::Message(message)) => assert!(message.contains("different commanders")),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(
        app.state
            .games
            .get_deck(ctx.deck)
            .await
            .unwrap()
            .unwrap()
            .commander_name,
        "Other Commander"
    );
}

// sheet_reconciliation_test.exs

const RECON_HEADER: &str = "Date\tWinner\tDeck\tDan\tMatt\tJesse\tWin Con\tOther Decks\tNotes\n";
const RECON_ROW: &str = "3/17/25\tDaniel\tEdgar Markov\t1\t1\t\tSwing Out\tReality (Kenrith); Matt (Sergeant John Benton)\tCorrected history\n";

struct ReconCtx {
    players: HashMap<&'static str, Player>,
    game: Game,
    deck: i64,
}

async fn recon_setup(app: &TestApp) -> ReconCtx {
    let mut players = HashMap::new();
    for name in ["Daniel", "Reality", "Matt", "Jesse", "Drew", "Landon"] {
        players.insert(name, app.player(name).await);
    }
    let deck = app
        .deck(players["Daniel"].id, "Cleaned-up vampires", "Edgar Markov")
        .await;
    // Deliberately different turn order, deck names, results, notes, and kill counts.
    let created = app
        .game(
            json!({
                "played_at": "2025-03-18T01:30:00Z",
                "source": "mythic_track",
                "external_id": "original",
                "notes": "Old notes",
                "turns": 8,
                "duration_minutes": 90,
                "seats": [
                    {"player_id": players["Matt"].id, "seat": 1, "result": "win", "kills": 2, "notes": "Seat note"},
                    {"player_id": players["Daniel"].id, "deck_id": deck.id, "seat": 2, "result": "loss",
                     "mvp_card_name": "Sol Ring"},
                    {"player_id": players["Reality"].id, "seat": 3, "result": "loss", "kills": 0}
                ]
            }),
            None,
        )
        .await;
    ReconCtx {
        game: game(app, created.id).await,
        players,
        deck: deck.id,
    }
}

fn input(ctx: &ReconCtx) -> Value {
    json!({"text": format!("{RECON_HEADER}{RECON_ROW}"), "players": {"Dan": ctx.players["Daniel"].id}})
}

fn with_text(mut params: Value, text: String) -> Value {
    params["text"] = json!(text);
    params
}

async fn preview_sheet(app: &TestApp, params: &Value) -> SheetPreview {
    let mut conn = app.pool().acquire().await.unwrap();
    sheet_preview::run(&mut conn, params).await.unwrap()
}

async fn import_sheet(
    app: &TestApp,
    params: &Value,
    revision: &str,
) -> Result<sheet_commit::SheetResult, ImportError> {
    sheet_commit::run(&app.state, params, Some(revision), None).await
}

async fn select_all(app: &TestApp, mut params: Value, action: Value) -> Value {
    let preview = preview_sheet(app, &params).await;
    let actions: Map<String, Value> = preview
        .rows
        .iter()
        .map(|row| (row.key.clone(), action.clone()))
        .collect();
    params["actions"] = Value::Object(actions);
    params
}

async fn new_decks(app: &TestApp, mut params: Value) -> Value {
    let preview = preview_sheet(app, &params).await;
    let decks: Map<String, Value> = preview
        .rows
        .iter()
        .flat_map(|row| &row.seats)
        .map(|seat| (seat.deck_key.clone(), json!("new")))
        .collect();
    params["decks"] = Value::Object(decks);
    params
}

fn first(preview: &SheetPreview) -> &ResolvedRow {
    &preview.rows[0]
}

async fn receipts(app: &TestApp) -> i64 {
    count(app, "sheet_import_receipts").await
}

#[tokio::test]
async fn updates_in_place_preserves_cleaned_identity_and_fields_and_remembers_reconciled_rows() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let params = input(&ctx);
    let initial = preview_sheet(&app, &params).await;
    assert_eq!(first(&initial).action, Choice::Id(ctx.game.id));
    assert!(initial.valid);
    assert_eq!(first(&initial).status, "changed");
    assert!(first(&initial).match_reason.contains("nearby date"));
    assert_eq!(
        json!(first(&initial).changes),
        json!([
            {"field": "result", "player": "Daniel", "before": "loss", "after": "win"},
            {"field": "kills", "player": "Daniel", "before": null, "after": 1},
            {"field": "result", "player": "Matt", "before": "win", "after": "loss"},
            {"field": "kills", "player": "Matt", "before": 2, "after": 1},
            {"field": "notes", "player": null, "before": "Old notes", "after": "Win con: Swing Out\nCorrected history"}
        ])
    );

    let preview = preview_sheet(&app, &params).await;
    assert!(preview.valid, "{:?}", first(&preview).errors);
    let result = import_sheet(&app, &params, &preview.revision)
        .await
        .unwrap();
    assert_eq!((result.updated, result.created), (1, 0));

    let saved = game(&app, ctx.game.id).await;
    assert_eq!(
        (
            saved.source.as_str(),
            saved.external_id.as_deref(),
            saved.played_at,
            saved.turns,
            saved.duration_minutes
        ),
        (
            "mythic_track",
            Some("original"),
            utc("2025-03-18T01:30:00Z"),
            Some(8),
            Some(90)
        )
    );
    assert_eq!(
        saved.notes.as_deref(),
        Some("Win con: Swing Out\nCorrected history")
    );
    let ids = |game: &Game| -> Vec<(i64, i64, Option<i64>, i64)> {
        game.seats
            .iter()
            .map(|seat| (seat.id, seat.player_id, seat.deck_id, seat.seat))
            .collect()
    };
    assert_eq!(ids(&saved), ids(&ctx.game));
    let outcome: Vec<(&str, GameResult, Option<i64>)> = saved
        .seats
        .iter()
        .map(|seat| (seat.player.name.as_str(), seat.result, seat.kills))
        .collect();
    assert_eq!(
        outcome,
        [
            ("Matt", GameResult::Loss, Some(1)),
            ("Daniel", GameResult::Win, Some(1)),
            ("Reality", GameResult::Loss, Some(0))
        ]
    );
    assert_eq!(saved.seats[0].notes.as_deref(), Some("Seat note"));
    assert_eq!(saved.seats[1].mvp_card_name.as_deref(), Some("Sol Ring"));
    let repeated = preview_sheet(&app, &params).await;
    assert_eq!(first(&repeated).imported_id, Some(ctx.game.id));
    assert_eq!(first(&repeated).action, Choice::Text("skip".into()));
    assert_eq!(count(&app, "games").await, 1);
}

#[tokio::test]
async fn blank_kills_and_explicit_zero_replace_counts_while_blank_notes_are_preserved() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let text = format!(
        "{RECON_HEADER}3/17/25\tDaniel\tEdgar\t0\t\t\t\tReality (Kenrith); Matt (Benton)\t\n"
    );
    let params = select_all(&app, with_text(input(&ctx), text), json!(ctx.game.id)).await;
    let preview = preview_sheet(&app, &params).await;
    import_sheet(&app, &params, &preview.revision)
        .await
        .unwrap();
    let saved = game(&app, ctx.game.id).await;
    assert_eq!(saved.notes.as_deref(), Some("Old notes"));
    let kills: Vec<Option<i64>> = saved.seats.iter().map(|seat| seat.kills).collect();
    assert_eq!(kills, [Some(0), Some(0), Some(0)]);
}

#[tokio::test]
async fn deck_diffs_reflect_committed_mappings_and_do_not_invent_changes_when_creation_reuses_a_deck()
 {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let key = json!(["Daniel", "Edgar Markov"]).to_string();
    let mut params = input(&ctx);
    params["decks"] = json!({key.clone(): "new"});
    let preview = preview_sheet(&app, &params).await;
    assert!(!first(&preview).changes.iter().any(|c| c.field == "deck"));
    assert_eq!(first(&preview).seats[0].deck_id, Some(Choice::Id(ctx.deck)));

    let replacement = app
        .deck(ctx.players["Daniel"].id, "Replacement", "Voja")
        .await;
    let mut params = input(&ctx);
    params["decks"] = json!({key: replacement.id});
    let preview = preview_sheet(&app, &params).await;
    let changes = json!(first(&preview).changes);
    assert!(changes.as_array().unwrap().contains(&json!(
        {"field": "deck", "player": "Daniel", "before": "Cleaned-up vampires", "after": "Replacement"}
    )));
    import_sheet(&app, &params, &preview.revision)
        .await
        .unwrap();
    let saved = game(&app, ctx.game.id).await;
    let daniel = saved
        .seats
        .iter()
        .find(|seat| seat.player_id == ctx.players["Daniel"].id)
        .unwrap();
    assert_eq!(daniel.deck_id, Some(replacement.id));
}

#[tokio::test]
async fn unchanged_values_are_skipped_despite_sheet_nicknames_and_different_turn_order() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let daniel = ctx.players["Daniel"].id;
    let reality = ctx.players["Reality"].id;
    let seats: Vec<Value> = ctx
        .game
        .seats
        .iter()
        .map(|seat| {
            json!({
                "id": seat.id,
                "player_id": seat.player_id,
                "seat": seat.seat,
                "deck_id": seat.deck_id,
                "result": if seat.player_id == daniel { "win" } else { "loss" },
                "kills": i64::from(seat.player_id != reality)
            })
        })
        .collect();
    app.state
        .games
        .update_game(
            &ctx.game,
            &json!({"seats": seats, "notes": "Win con: Swing Out\nCorrected history"}),
        )
        .await
        .unwrap();
    let preview = preview_sheet(&app, &input(&ctx)).await;
    assert_eq!(preview.rows.len(), 1);
    let row = first(&preview);
    assert_eq!(row.target.as_ref().unwrap().id, ctx.game.id);
    assert!(row.changes.is_empty());
    assert_eq!(row.status, "unchanged");
    assert_eq!(row.action, Choice::Text("skip".into()));
    assert!(!preview.valid);
}

#[tokio::test]
async fn multiple_games_use_deck_evidence_never_the_recorded_winner_to_break_ties() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let other_deck = app
        .deck(
            ctx.players["Daniel"].id,
            "Other deck",
            "Voja, Jaws of the Conclave",
        )
        .await;
    let other = app
        .game(
            json!({
                "played_at": ctx.game.played_at,
                "seats": [
                    {"player_id": ctx.players["Daniel"].id, "deck_id": other_deck.id, "seat": 1, "result": "win"},
                    {"player_id": ctx.players["Matt"].id, "seat": 2, "result": "loss"},
                    {"player_id": ctx.players["Reality"].id, "seat": 3, "result": "loss"}
                ]
            }),
            None,
        )
        .await;
    let preview = preview_sheet(&app, &input(&ctx)).await;
    assert_eq!(first(&preview).action, Choice::Id(ctx.game.id));
    assert!(first(&preview).match_reason.contains("decks"));

    let params = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}{}",
            RECON_ROW.replace("Edgar Markov", "Voja")
        ),
    );
    let preview = preview_sheet(&app, &params).await;
    assert_eq!(first(&preview).action, Choice::Id(other.id));

    let params = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}{}",
            RECON_ROW.replace("Edgar Markov", "Nickname")
        ),
    );
    let preview = preview_sheet(&app, &params).await;
    let row = first(&preview);
    assert!(row.target.is_none());
    assert_eq!(row.status, "review");
    assert_eq!(row.action, Choice::Text("skip".into()));
    assert!(row.match_reason.contains("Multiple games"));
}

#[tokio::test]
async fn unmapped_players_and_invalid_rows_never_auto_select_a_game() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let params = with_text(
        input(&ctx),
        format!("{RECON_HEADER}{}", RECON_ROW.replace("Reality", "Unknown")),
    );
    let preview = preview_sheet(&app, &params).await;
    assert_eq!(first(&preview).action, Choice::Text("skip".into()));
    assert!(first(&preview).target.is_none());

    let params = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}{}",
            RECON_ROW.replace("\t1\t1\t", "\t5\t1\t")
        ),
    );
    let preview = preview_sheet(&app, &params).await;
    let row = first(&preview);
    assert_eq!(row.target.as_ref().unwrap().id, ctx.game.id);
    assert_eq!(row.status, "review");
    assert_eq!(row.action, Choice::Text("skip".into()));
}

#[tokio::test]
async fn skipping_a_matched_row_preserves_its_comparison_but_prevents_writes() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let params = select_all(&app, input(&ctx), json!("skip")).await;
    let preview = preview_sheet(&app, &params).await;
    assert_eq!(first(&preview).target.as_ref().unwrap().id, ctx.game.id);
    assert!(!first(&preview).changes.is_empty());
    assert!(!preview.valid);
    assert!(
        import_sheet(&app, &params, &preview.revision)
            .await
            .is_err()
    );
    assert_eq!(
        game(&app, ctx.game.id).await.notes.as_deref(),
        Some("Old notes")
    );
}

#[tokio::test]
async fn competing_sheet_rows_are_left_for_review_and_cannot_update_one_game() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let params = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}{RECON_ROW}{}",
            RECON_ROW.replace("Corrected history", "Second game")
        ),
    );
    let preview = preview_sheet(&app, &params).await;
    assert!(
        preview
            .rows
            .iter()
            .all(|row| row.action == Choice::Text("skip".into()))
    );
    let params = select_all(&app, params, json!(ctx.game.id)).await;
    let invalid = preview_sheet(&app, &params).await;
    assert!(!invalid.valid);
    assert!(invalid.rows.iter().all(|row| {
        row.errors
            .iter()
            .any(|error| error.contains("Two sheet rows"))
    }));
    assert!(
        import_sheet(&app, &params, &invalid.revision)
            .await
            .is_err()
    );
    assert_eq!(
        game(&app, ctx.game.id).await.notes.as_deref(),
        Some("Old notes")
    );
}

#[tokio::test]
async fn stale_notes_and_changed_input_invalidate_the_preview() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let params = select_all(&app, input(&ctx), json!(ctx.game.id)).await;
    let preview = preview_sheet(&app, &params).await;
    let changed = with_text(
        params.clone(),
        format!(
            "{RECON_HEADER}{}",
            RECON_ROW.replace("Corrected", "Changed")
        ),
    );
    assert!(
        import_sheet(&app, &changed, &preview.revision)
            .await
            .is_err()
    );
    app.state
        .games
        .update_game(&ctx.game, &json!({"notes": "Edited after preview"}))
        .await
        .unwrap();
    assert!(
        import_sheet(&app, &params, &preview.revision)
            .await
            .is_err()
    );
    assert_eq!(
        game(&app, ctx.game.id).await.notes.as_deref(),
        Some("Edited after preview")
    );
    assert_eq!(receipts(&app).await, 0);
}

#[tokio::test]
async fn invalid_selected_rows_block_the_batch_skipped_invalid_rows_do_not() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let mut params = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}{RECON_ROW}3/7/25\tMatt\tPantyBlink\t\t3\t\tCombo\t\tMissing opponents\n"
        ),
    );
    let preview = preview_sheet(&app, &params).await;
    let (valid_key, invalid_key) = (preview.rows[0].key.clone(), preview.rows[1].key.clone());
    params["actions"] = json!({valid_key: ctx.game.id, invalid_key.clone(): "create"});
    let blocked = preview_sheet(&app, &params).await;
    assert!(!blocked.valid);
    assert!(
        import_sheet(&app, &params, &blocked.revision)
            .await
            .is_err()
    );
    assert_eq!(
        game(&app, ctx.game.id).await.notes.as_deref(),
        Some("Old notes")
    );
    params["actions"][invalid_key] = json!("skip");
    let ready = preview_sheet(&app, &params).await;
    let result = import_sheet(&app, &params, &ready.revision).await.unwrap();
    assert_eq!((result.updated, result.skipped), (1, 1));
}

#[tokio::test]
async fn approved_jesse_correction_creates_a_missing_game_with_explicit_deck_mapping() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let text = format!(
        "{RECON_HEADER}9/4/25\tJesse\tSoul of Windgrace\t\t\t3\tSwing Out\tDrew (Merieke); Matt (Cloud); Landon (Archelos)\tJesse 3 kills - craterhoof game ender\n"
    );
    let params = select_all(&app, with_text(input(&ctx), text), json!("create")).await;
    let params = new_decks(&app, params).await;
    let preview = preview_sheet(&app, &params).await;
    assert!(preview.valid, "{:?}", first(&preview).errors);
    let result = import_sheet(&app, &params, &preview.revision)
        .await
        .unwrap();
    assert_eq!(result.created, 1);
    let created = game(&app, result.game_ids[0]).await;
    assert_eq!(created.seats[0].player.name, "Jesse");
    assert_eq!(created.seats[0].kills, Some(3));
    assert!(created.seats[1..].iter().all(|seat| seat.kills == Some(0)));
    assert_eq!(
        created.seats[0].deck.as_ref().unwrap().name,
        "Soul of Windgrace"
    );
    let replay = preview_sheet(&app, &params).await;
    assert_eq!(first(&replay).action, Choice::Text("skip".into()));
}

#[tokio::test]
async fn october_draw_preserves_the_funny_note_rather_than_inferring_a_loss() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let text = format!(
        "{RECON_HEADER}10/2/25\tN/A\tN/A\t\t\t\tN/A\tDrew (Karlov); Matt (Gylwain); Reality (Kenrith); Dan (Sokrates) Landon(Oona)\t4-way Tie due to Divine Intervention. Woo. Matt lost tho.\n"
    );
    let params = select_all(&app, with_text(input(&ctx), text), json!("create")).await;
    let params = new_decks(&app, params).await;
    let preview = preview_sheet(&app, &params).await;
    assert!(preview.valid, "{:?}", first(&preview).errors);
    let result = import_sheet(&app, &params, &preview.revision)
        .await
        .unwrap();
    let created = game(&app, result.game_ids[0]).await;
    assert_eq!(created.seats.len(), 5);
    assert!(
        created
            .seats
            .iter()
            .all(|seat| seat.result == GameResult::Draw)
    );
    assert_eq!(
        created.notes.as_deref(),
        Some("4-way Tie due to Divine Intervention. Woo. Matt lost tho.")
    );
}

#[tokio::test]
async fn rejects_alias_collisions_misplaced_kills_and_foreign_owned_decks() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let collision = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}{}",
            RECON_ROW.replace("Reality (Kenrith)", "Dan (Tyvar)")
        ),
    );
    let collision = select_all(&app, collision, json!(ctx.game.id)).await;
    let preview = preview_sheet(&app, &collision).await;
    assert!(
        first(&preview)
            .errors
            .iter()
            .any(|error| error.contains("same player twice"))
    );

    let misplaced = with_text(
        input(&ctx),
        format!(
            "{RECON_HEADER}9/4/25\tJesse\tWindgrace\t3\t\t\tCombat\tMatt (Cloud); Drew (A); Landon (B)\t\n"
        ),
    );
    let misplaced = select_all(&app, misplaced, json!("create")).await;
    let preview = preview_sheet(&app, &misplaced).await;
    assert!(
        first(&preview)
            .errors
            .iter()
            .any(|error| error.contains("not seated"))
    );

    let mut params = select_all(&app, input(&ctx), json!(ctx.game.id)).await;
    params["decks"] = json!({json!(["Matt", "Sergeant John Benton"]).to_string(): ctx.deck});
    let preview = preview_sheet(&app, &params).await;
    assert!(!preview.valid);
    assert!(
        import_sheet(&app, &params, &preview.revision)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_persistence_failure_rolls_back_earlier_updates_and_new_identities() {
    let app = TestApp::new().await;
    let ctx = recon_setup(&app).await;
    let long_name = "X".repeat(101);
    let bad = format!("3/20/25\tNew player\t{long_name}\t\t\t\tCombat\tMatt (Cloud)\t\n");
    let mut params = with_text(input(&ctx), format!("{RECON_HEADER}{RECON_ROW}{bad}"));
    let preview = preview_sheet(&app, &params).await;
    let (first_key, second_key) = (preview.rows[0].key.clone(), preview.rows[1].key.clone());
    params["actions"] = json!({first_key: ctx.game.id, second_key: "create"});
    params["players"] = json!({"Dan": ctx.players["Daniel"].id, "New player": "new"});
    let params = new_decks(&app, params).await;
    let preview = preview_sheet(&app, &params).await;
    assert!(
        preview.valid,
        "{:?}",
        preview.rows.iter().map(|r| &r.errors).collect::<Vec<_>>()
    );
    assert!(matches!(
        import_sheet(&app, &params, &preview.revision).await,
        Err(ImportError::Invalid(_))
    ));
    assert_eq!(
        game(&app, ctx.game.id).await.notes.as_deref(),
        Some("Old notes")
    );
    assert!(player_named(&app, "New player").await.is_none());
    assert_eq!(receipts(&app).await, 0);
}
