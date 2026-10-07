//! SpellBot game tracking, recording reported games, and commander card choices.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use serde_json::json;
use sqlx::SqliteConnection;
use support::discord::{command, interaction, player, report, string};
use support::{TestApp, utc};
use the_gathering::db::UtcDateTime;
use the_gathering::discord::api::ResponseKind;
use the_gathering::discord::card_choice::{self, Mode};
use the_gathering::discord::tracker::Tracker;
use the_gathering::discord::{self, GameReport, GamesSink, ResolveError, Sink, SinkError, won};
use the_gathering::games::GameResult;

#[derive(Default)]
struct TestSink(Mutex<Vec<GameReport>>);

impl TestSink {
    fn take(&self) -> Vec<GameReport> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

impl Sink for TestSink {
    fn handle_report<'a>(
        &'a self,
        _conn: &'a mut SqliteConnection,
        report: &'a GameReport,
    ) -> BoxFuture<'a, Result<(), SinkError>> {
        self.0.lock().unwrap().push(report.clone());
        Box::pin(async { Ok(()) })
    }
}

struct FailingSink;

impl Sink for FailingSink {
    fn handle_report<'a>(
        &'a self,
        conn: &'a mut SqliteConnection,
        report: &'a GameReport,
    ) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            if report.winner_discord_ids.is_empty() {
                return Ok(());
            }
            the_gathering::games::player::create_player(
                conn,
                &json!({"name": "Rolled Back"}),
                None,
            )
            .await
            .unwrap();
            Err(SinkError::Other("forced_failure".into()))
        })
    }
}

fn tracker_report() -> GameReport {
    report(
        utc("2025-06-15T15:08:43Z"),
        vec![player("111", "Aria", None), player("222", "Bryn", None)],
    )
}

async fn count(app: &TestApp, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(app.pool())
        .await
        .unwrap()
}

async fn pending(app: &TestApp, external_id: &str) -> Option<discord::PendingGame> {
    discord::get_pending_by_external_id(app.pool(), external_id)
        .await
        .unwrap()
}

struct Ctx {
    app: TestApp,
    sink: Arc<TestSink>,
    tracker: Tracker,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let sink = Arc::new(TestSink::default());
    let tracker = Tracker::new(app.pool().clone(), sink.clone());
    Ctx { app, sink, tracker }
}

#[tokio::test]
async fn dispatches_observations_and_completed_winner_reports_to_the_sink() {
    let ctx = setup().await;
    let observed = tracker_report();
    ctx.tracker.observe(&observed).await.unwrap();
    assert_eq!(ctx.sink.take(), std::slice::from_ref(&observed));

    let completed = ctx.tracker.record_winner("12345", "111").await.unwrap();
    assert_eq!(completed.winner_discord_ids, ["111"]);
    assert_eq!(completed.raw["winner_reported_by"], "111");
    assert_eq!(ctx.sink.take(), [completed]);
    assert!(pending(&ctx.app, &observed.external_id).await.is_none());
}

#[tokio::test]
async fn staged_reports_survive_a_tracker_restart() {
    let ctx = setup().await;
    let observed = tracker_report();
    ctx.tracker.observe(&observed).await.unwrap();
    assert_eq!(ctx.sink.take().len(), 1);

    let restarted = Tracker::new(ctx.app.pool().clone(), ctx.sink.clone());
    let completed = restarted.record_winner("SB12345", "111").await.unwrap();
    assert_eq!(completed.external_id, observed.external_id);
    assert_eq!(ctx.sink.take(), [completed]);
}

#[tokio::test]
async fn replaying_a_report_updates_its_staged_normalized_data() {
    let ctx = setup().await;
    ctx.tracker.observe(&tracker_report()).await.unwrap();
    let replay = GameReport {
        played_at: utc("2025-06-16T12:00:00Z"),
        players: vec![
            player("111", "Aria Updated", Some("Alela")),
            player("222", "Bryn", None),
        ],
        ..tracker_report()
    };
    ctx.tracker.observe(&replay).await.unwrap();
    assert_eq!(ctx.sink.take().last(), Some(&replay));
    let staged = pending(&ctx.app, &replay.external_id).await.unwrap();
    assert_eq!(staged.played_at, replay.played_at);
    assert_eq!(staged.report().players, replay.players);
}

async fn age_pending(app: &TestApp, days: i64) {
    let stale = UtcDateTime::now().plus(time::Duration::days(-days));
    sqlx::query("UPDATE pending_discord_games SET updated_at = ?")
        .bind(stale)
        .execute(app.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn listing_pending_reports_does_not_prune_expired_rows() {
    let ctx = setup().await;
    ctx.tracker.observe(&tracker_report()).await.unwrap();
    age_pending(&ctx.app, 31).await;
    assert_eq!(
        discord::list_pending(ctx.app.pool()).await.unwrap().len(),
        1
    );
    assert!(pending(&ctx.app, "spellbot:SB12345").await.is_some());
    discord::prune_pending(ctx.app.pool(), UtcDateTime::now())
        .await
        .unwrap();
    assert!(
        discord::list_pending(ctx.app.pool())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn pruning_removes_only_reports_older_than_30_days() {
    let ctx = setup().await;
    ctx.tracker.observe(&tracker_report()).await.unwrap();
    age_pending(&ctx.app, 31).await;
    let fresh = GameReport {
        external_id: "spellbot:SB20000".into(),
        ..tracker_report()
    };
    let fresh_pending = discord::stage_report(ctx.app.pool(), &fresh).await.unwrap();
    discord::prune_pending(ctx.app.pool(), UtcDateTime::now())
        .await
        .unwrap();
    assert!(pending(&ctx.app, "spellbot:SB12345").await.is_none());
    assert_eq!(
        pending(&ctx.app, &fresh.external_id).await.unwrap().id,
        fresh_pending.id
    );
}

#[tokio::test]
async fn does_not_let_a_non_player_report_themselves_as_winner() {
    let ctx = setup().await;
    ctx.tracker.observe(&tracker_report()).await.unwrap();
    ctx.sink.take();
    assert!(matches!(
        ctx.tracker.record_winner("SB12345", "999").await,
        Err(ResolveError::NotAPlayer)
    ));
    assert!(ctx.sink.take().is_empty());
}

#[tokio::test]
async fn a_failed_bot_resolution_rolls_back_writes_and_leaves_the_pending_game_intact() {
    let app = TestApp::new().await;
    let tracker = Tracker::new(app.pool().clone(), Arc::new(FailingSink));
    tracker.observe(&tracker_report()).await.unwrap();
    let error = tracker.record_winner("SB12345", "111").await.unwrap_err();
    assert!(
        matches!(&error, ResolveError::SinkFailed(SinkError::Other(reason)) if reason == "forced_failure"),
        "{error:?}"
    );
    assert!(pending(&app, "spellbot:SB12345").await.is_some());
    let rolled_back: i64 =
        sqlx::query_scalar("SELECT count(*) FROM players WHERE name = 'Rolled Back'")
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(rolled_back, 0);
}

#[tokio::test]
async fn without_a_game_id_completes_the_most_recently_started_game_in_the_channel() {
    let ctx = setup().await;
    let older = tracker_report();
    let other_channel = GameReport {
        external_id: "spellbot:SB30000".into(),
        channel_id: "555".into(),
        ..tracker_report()
    };
    let newer = GameReport {
        external_id: "spellbot:SB20000".into(),
        played_at: utc("2025-06-15T18:00:00Z"),
        ..tracker_report()
    };
    // Observe the newest game first so recency comes from played_at, not order.
    for observed in [&newer, &other_channel, &older] {
        ctx.tracker.observe(observed).await.unwrap();
    }
    ctx.sink.take();
    let completed = ctx
        .tracker
        .record_latest_winner("444", "111")
        .await
        .unwrap();
    assert_eq!(completed.external_id, "spellbot:SB20000");
    let dispatched = ctx.sink.take();
    assert_eq!(dispatched[0].external_id, "spellbot:SB20000");
    assert_eq!(dispatched[0].winner_discord_ids, ["111"]);
    assert!(matches!(
        ctx.tracker.record_latest_winner("444", "999").await,
        Err(ResolveError::NotAPlayer)
    ));
    assert!(matches!(
        ctx.tracker.record_latest_winner("666", "111").await,
        Err(ResolveError::NoGameInChannel)
    ));
}

#[tokio::test]
async fn without_a_game_id_skips_a_re_staged_game_that_already_has_a_winner() {
    let app = TestApp::new().await;
    let tracker = Tracker::new(app.pool().clone(), Arc::new(GamesSink));
    let older = tracker_report();
    let newer = GameReport {
        external_id: "spellbot:SB20000".into(),
        played_at: utc("2025-06-15T18:00:00Z"),
        ..tracker_report()
    };
    tracker.observe(&older).await.unwrap();
    tracker.observe(&newer).await.unwrap();
    let completed = tracker.record_winner("SB20000", "111").await.unwrap();
    assert_eq!(completed.external_id, "spellbot:SB20000");
    tracker.observe(&newer).await.unwrap();
    assert!(pending(&app, &newer.external_id).await.is_some());
    let listed = discord::list_pending(app.pool()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].external_id, older.external_id);
    let completed = tracker.record_latest_winner("444", "111").await.unwrap();
    assert_eq!(completed.external_id, older.external_id);
}

#[tokio::test]
async fn without_a_game_id_reports_no_game_when_every_staged_game_already_has_a_winner() {
    let app = TestApp::new().await;
    let tracker = Tracker::new(app.pool().clone(), Arc::new(GamesSink));
    let observed = tracker_report();
    tracker.observe(&observed).await.unwrap();
    tracker.record_winner("SB12345", "111").await.unwrap();
    tracker.observe(&observed).await.unwrap();
    assert!(pending(&app, &observed.external_id).await.is_some());
    assert!(discord::list_pending(app.pool()).await.unwrap().is_empty());
    assert!(matches!(
        tracker.record_latest_winner("444", "111").await,
        Err(ResolveError::NoGameInChannel)
    ));
}

#[tokio::test]
async fn slash_command_without_options_uses_the_invoking_channel() {
    let ctx = setup().await;
    ctx.tracker.observe(&tracker_report()).await.unwrap();
    ctx.sink.take();
    let event = interaction("444", "222", command("won", vec![]));
    let response = won::handle(&ctx.app.state, &event).await;
    assert_eq!(response.kind, ResponseKind::Modal);
    assert_eq!(response.modal_data().unwrap().title, "Game details");
    assert!(ctx.sink.take().is_empty());
    assert!(pending(&ctx.app, "spellbot:SB12345").await.is_some());

    let elsewhere = interaction("999", "222", command("won", vec![]));
    let response = won::handle(&ctx.app.state, &elsewhere).await;
    assert!(
        response
            .content()
            .contains("haven't seen an unfinished SpellBot game")
    );
}

#[tokio::test]
async fn slash_command_opens_a_modal_and_gives_ephemeral_errors() {
    let ctx = setup().await;
    ctx.tracker.observe(&tracker_report()).await.unwrap();
    ctx.sink.take();
    let event = interaction(
        "444",
        "111",
        command("won", vec![("game", string("SB12345"))]),
    );
    assert_eq!(
        won::handle(&ctx.app.state, &event).await.kind,
        ResponseKind::Modal
    );
    assert!(ctx.sink.take().is_empty());

    let unknown = interaction(
        "444",
        "111",
        command("won", vec![("game", string("SB99999"))]),
    );
    let response = won::handle(&ctx.app.state, &unknown).await;
    assert_eq!(response.message_data().unwrap().flags, Some(64));
    assert!(response.content().contains("haven't seen"));
}

// Sink.Games

fn sink_report(winners: &[&str]) -> GameReport {
    let mut report = report(
        utc("2026-09-19T18:00:00Z"),
        vec![
            player("111", "Aria", Some("Alela, Artful Provocateur")),
            player("222", "Bryn", None),
        ],
    );
    report.winner_discord_ids = winners.iter().map(|id| (*id).to_owned()).collect();
    report.raw.insert("message_id".into(), json!("555"));
    report
}

async fn handle(app: &TestApp, report: &GameReport) -> Result<(), SinkError> {
    let mut conn = app.pool().acquire().await.unwrap();
    GamesSink.handle_report(&mut conn, report).await
}

async fn discord_game(app: &TestApp) -> the_gathering::games::Game {
    let id: i64 = sqlx::query_scalar(
        "SELECT id FROM games WHERE source = 'discord' AND external_id = 'spellbot:SB12345'",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    app.state.games.get_game(id).await.unwrap().unwrap()
}

#[tokio::test]
async fn creates_players_decks_and_a_completed_game() {
    let app = TestApp::new().await;
    handle(&app, &sink_report(&["111"])).await.unwrap();
    assert_eq!(count(&app, "players").await, 2);
    assert_eq!(count(&app, "decks").await, 1);
    assert_eq!(count(&app, "games").await, 1);
    let game = discord_game(&app).await;
    let mut seats = game.seats.clone();
    seats.sort_by_key(|seat| seat.seat);
    assert_eq!(seats[0].player.discord_id.as_deref(), Some("111"));
    assert_eq!(seats[0].result, GameResult::Win);
    assert_eq!(seats[1].player.discord_id.as_deref(), Some("222"));
    assert_eq!(seats[1].result, GameResult::Loss);
    let deck = seats[0].deck.clone().unwrap();
    assert_eq!(deck.player_id, seats[0].player_id);
    assert_eq!(deck.name, "Alela, Artful Provocateur");
    assert_eq!(deck.commander_name, "Alela, Artful Provocateur");
    assert_eq!(deck.commander_card_id, None);
    assert_eq!(deck.color_identity, "");
}

#[tokio::test]
async fn replay_updates_seat_order_and_winner_without_creating_duplicates() {
    let app = TestApp::new().await;
    handle(&app, &sink_report(&["111"])).await.unwrap();
    let mut replay = sink_report(&["222"]);
    replay.players.reverse();
    handle(&app, &replay).await.unwrap();
    assert_eq!(count(&app, "players").await, 2);
    assert_eq!(count(&app, "games").await, 1);
    let mut seats = discord_game(&app).await.seats;
    seats.sort_by_key(|seat| seat.seat);
    assert_eq!(seats[0].player.discord_id.as_deref(), Some("222"));
    assert_eq!(seats[0].result, GameResult::Win);
    assert_eq!(seats[1].player.discord_id.as_deref(), Some("111"));
    assert_eq!(seats[1].result, GameResult::Loss);
}

#[tokio::test]
async fn distinct_discord_users_with_the_same_display_name_get_distinct_players() {
    let app = TestApp::new().await;
    let mut same = sink_report(&["111"]);
    for player in &mut same.players {
        player.display_name = "Shared Name".into();
    }
    handle(&app, &same).await.unwrap();
    let names: Vec<(String, String)> =
        sqlx::query_as("SELECT discord_id, name FROM players ORDER BY discord_id")
            .fetch_all(app.pool())
            .await
            .unwrap();
    assert_eq!(
        names,
        [
            ("111".to_owned(), "Shared Name".to_owned()),
            ("222".to_owned(), "Shared Name (2)".to_owned())
        ]
    );
    assert_eq!(count(&app, "games").await, 1);
}

#[tokio::test]
async fn won_updates_the_winner_of_an_already_created_game() {
    let app = TestApp::new().await;
    handle(&app, &sink_report(&["111"])).await.unwrap();
    let tracker = Tracker::new(app.pool().clone(), Arc::new(GamesSink));
    tracker.observe(&sink_report(&[])).await.unwrap();
    assert!(pending(&app, "spellbot:SB12345").await.is_some());
    tracker.record_winner("SB12345", "222").await.unwrap();
    assert_eq!(count(&app, "games").await, 1);
    assert!(pending(&app, "spellbot:SB12345").await.is_none());
    let seats = discord_game(&app).await.seats;
    let result = |id: &str| {
        seats
            .iter()
            .find(|seat| seat.player.discord_id.as_deref() == Some(id))
            .unwrap()
            .result
    };
    assert_eq!(result("222"), GameResult::Win);
    assert_eq!(result("111"), GameResult::Loss);
}

#[tokio::test]
async fn winnerless_reports_stay_pending_and_invalid_reports_return_errors() {
    let app = TestApp::new().await;
    handle(&app, &sink_report(&[])).await.unwrap();
    assert_eq!(count(&app, "games").await, 0);
    assert_eq!(count(&app, "players").await, 0);

    let mut invalid = sink_report(&["999"]);
    invalid.players.truncate(1);
    assert!(matches!(
        handle(&app, &invalid).await,
        Err(SinkError::InvalidPlayerCount)
    ));
    assert_eq!(count(&app, "games").await, 0);

    let tracker = Tracker::new(app.pool().clone(), Arc::new(GamesSink));
    assert!(matches!(
        tracker.observe(&invalid).await,
        Err(discord::tracker::ObserveError::Sink(
            SinkError::InvalidPlayerCount
        ))
    ));
    assert!(pending(&app, &invalid.external_id).await.is_none());
}

// CardChoice

async fn insert_card(app: &TestApp, id: &str, name: &str) {
    app.catalog_card(json!({
        "id": id,
        "oracle_id": format!("oracle-{id}"),
        "name": name,
        "collector_number": id,
        "type_line": "Legendary Creature — Human",
        "rarity": "rare",
    }))
    .await;
}

async fn resolve(app: &TestApp, name: &str) -> discord::draft::CardChoice {
    let mut conn = app.pool().acquire().await.unwrap();
    card_choice::resolve(&mut conn, Some(name), "commander", Mode::All)
        .await
        .unwrap()
}

#[tokio::test]
async fn resolves_a_unique_whole_leading_name_among_more_than_25_broad_matches() {
    let app = TestApp::new().await;
    insert_card(&app, "bello", "Bello, Bard of the Brambles").await;
    for number in 1..=25 {
        insert_card(
            &app,
            &format!("decoy-{number}"),
            &format!("Bellowing Decoy {number}"),
        )
        .await;
    }
    let choice = resolve(&app, "Bello").await;
    assert_eq!(choice.id.as_deref(), Some("bello"));
    assert_eq!(choice.name, "Bello, Bard of the Brambles");
    assert_eq!(choice.error, None);
}

#[tokio::test]
async fn keeps_multiple_whole_leading_names_ambiguous_while_resolving_front_face_shorthand() {
    let app = TestApp::new().await;
    insert_card(
        &app,
        "sephiroth-fabled",
        "Sephiroth, Fabled SOLDIER // Sephiroth, One-Winged Angel",
    )
    .await;
    insert_card(&app, "sephiroth-heir", "Sephiroth, Planet's Heir").await;
    insert_card(&app, "terra", "Terra, Magical Adept // Esper Terra").await;

    let choice = resolve(&app, "Sephiroth").await;
    assert_eq!(choice.id, None);
    assert_eq!(choice.candidates.len(), 2);
    assert_eq!(
        choice.error.as_deref(),
        Some("Choose a matching commander card below.")
    );
    assert_eq!(
        resolve(&app, "sephiroth fabled soldier")
            .await
            .id
            .as_deref(),
        Some("sephiroth-fabled")
    );
    assert_eq!(
        resolve(&app, "Terra magical adept").await.id.as_deref(),
        Some("terra")
    );
}
