//! Ported from `test/the_gathering_web/controllers/api/admin_discord_pending_controller_test.exs`
//! and `discord_result_draft_controller_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use serde_json::{Value, json};
use support::discord::{command, interaction, player, report};
use support::{TestApp, utc};
use the_gathering::accounts::User;
use the_gathering::config::DiscordBotConfig;
use the_gathering::db::UtcDateTime;
use the_gathering::discord::api::{ButtonStyle, Component, ResponseKind};
use the_gathering::discord::interaction::OptionValue;
use the_gathering::discord::web_draft::{self, OpenError};
use the_gathering::discord::{self, Actor, GameReport, log_command};
use the_gathering::games::{Game, GameResult};

fn pending_report() -> GameReport {
    let mut report = report(
        utc("2026-09-20T14:00:00Z"),
        vec![
            player("111", "Aria", Some("Alela")),
            player("222", "Bryn", None),
        ],
    );
    report.raw.insert("message_id".into(), json!("555"));
    report
}

async fn count(app: &TestApp, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(app.pool())
        .await
        .unwrap()
}

async fn exists(app: &TestApp, table: &str, id: &str) -> bool {
    let found: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table} WHERE id = ?"
    )))
    .bind(id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    found > 0
}

async fn discord_game(app: &TestApp) -> Option<Game> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM games WHERE source = 'discord' AND external_id = 'spellbot:SB12345'",
    )
    .fetch_optional(app.pool())
    .await
    .unwrap();
    match id {
        Some(id) => app.state.games.get_game(id).await.unwrap(),
        None => None,
    }
}

// AdminDiscordPendingController

async fn admin_app() -> TestApp {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    app
}

#[tokio::test]
async fn admin_lists_and_resolves_a_pending_discord_game() {
    let app = admin_app().await;
    let pending = discord::stage_report(app.pool(), &pending_report())
        .await
        .unwrap();
    let body = app.get("/api/admin/discord/pending").await.assert_json(200);
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["id"], pending.id);
    assert_eq!(data[0]["external_id"], "spellbot:SB12345");
    assert_eq!(data[0]["channel_id"], "444");
    assert_eq!(data[0]["guild_id"], "333");
    assert_eq!(data[0]["played_at"], "2026-09-20T14:00:00Z");
    let names: Vec<&str> = data[0]["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|player| player["display_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Aria", "Bryn"]);
    assert_eq!(
        data[0]["players"][0],
        json!({"discord_id": "111", "display_name": "Aria", "commander_name": "Alela"})
    );

    let response = app
        .patch(
            &format!("/api/admin/discord/pending/{}", pending.id),
            json!({"winner_discord_id": "222"}),
        )
        .await;
    assert_eq!(response.status, 204);
    assert!(!exists(&app, "pending_discord_games", &pending.id.to_string()).await);
    let game = discord_game(&app).await.unwrap();
    let seat = game
        .seats
        .iter()
        .find(|seat| seat.player.discord_id.as_deref() == Some("222"))
        .unwrap();
    assert_eq!(seat.result, GameResult::Win);
}

#[tokio::test]
async fn admin_can_discard_a_pending_game() {
    let app = admin_app().await;
    let pending = discord::stage_report(app.pool(), &pending_report())
        .await
        .unwrap();
    let response = app
        .delete(&format!("/api/admin/discord/pending/{}", pending.id))
        .await;
    assert_eq!(response.status, 204);
    assert!(!exists(&app, "pending_discord_games", &pending.id.to_string()).await);
    app.delete(&format!("/api/admin/discord/pending/{}", pending.id))
        .await
        .assert_json(404);
}

#[tokio::test]
async fn a_failed_admin_resolution_leaves_the_pending_game_intact() {
    let app = admin_app().await;
    let mut invalid = pending_report();
    invalid.players.truncate(1);
    let pending = discord::stage_report(app.pool(), &invalid).await.unwrap();
    let body = app
        .patch(
            &format!("/api/admin/discord/pending/{}", pending.id),
            json!({"winner_discord_id": "111"}),
        )
        .await
        .assert_json(400);
    assert_eq!(body, json!({"errors": {"detail": "Bad Request"}}));
    assert!(exists(&app, "pending_discord_games", &pending.id.to_string()).await);
    assert!(discord_game(&app).await.is_none());
    app.patch(
        &format!("/api/admin/discord/pending/{}", pending.id),
        json!({}),
    )
    .await
    .assert_json(400);
}

#[tokio::test]
async fn pending_routes_require_an_administrator() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    app.get("/api/admin/discord/pending").await.assert_json(403);
}

// DiscordResultDraftController

struct Ctx {
    app: TestApp,
    user: User,
    pending_id: i64,
}

fn draft_report() -> GameReport {
    report(
        utc("2026-09-20T14:00:00Z"),
        vec![player("111", "Aria", None), player("222", "Bryn", None)],
    )
}

async fn setup() -> Ctx {
    let app = TestApp::with_config(|config| {
        config.discord_bot = Some(DiscordBotConfig {
            token: "test-token".into(),
            guild_id: Some("333".into()),
            spellbot_user_id: "725510263251402832".into(),
        });
    })
    .await;
    let user = app.unique_member().await;
    sqlx::query("UPDATE users SET discord_id = '999' WHERE id = ?")
        .bind(user.id)
        .execute(app.pool())
        .await
        .unwrap();
    let user = app.reload(&user).await.unwrap();
    let pending = discord::stage_report(app.pool(), &draft_report())
        .await
        .unwrap();
    app.log_in(&user).await;
    Ctx {
        app,
        user,
        pending_id: pending.id,
    }
}

fn actor() -> Actor {
    Actor {
        discord_id: "999".into(),
        guild_id: "333".into(),
        channel_id: "444".into(),
    }
}

fn payload() -> Value {
    json!({
        "played_at": "2026-09-20T14:00:00Z",
        "turns": 8,
        "duration_minutes": 72,
        "win_condition": "combat_damage",
        "notes": "Web log note",
        "seats": [
            {
                "discord_id": "222",
                "result": "win",
                "kills": 1,
                "deck": {
                    "name": "Raccoon",
                    "commander_name": "Bello, Bard of the Brambles",
                    "color_identity": "RG"
                }
            },
            {"discord_id": "111", "result": "loss", "kills": 0}
        ]
    })
}

async fn open(ctx: &Ctx) -> String {
    web_draft::open(&ctx.app.state, "", None, &actor())
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn a_non_player_can_get_a_private_link_with_an_optional_winner_mention_preview_is_read_only()
{
    let ctx = setup().await;
    let event = interaction(
        "444",
        "999",
        command("log", vec![("winner", OptionValue::User("222".into()))]),
    );
    let response = log_command::handle(&ctx.app.state, &event).await;
    assert_eq!(response.kind, ResponseKind::ChannelMessage);
    let data = response.message_data().unwrap();
    assert_eq!(data.flags, Some(64));
    assert_eq!(
        serde_json::to_value(&data.allowed_mentions).unwrap(),
        json!({"parse": []})
    );
    let [Component::ActionRow(row)] = data.rows() else {
        panic!("{data:?}")
    };
    let [Component::Button(button)] = row.components.as_slice() else {
        panic!("{row:?}")
    };
    assert_eq!(button.style, ButtonStyle::Link);
    let url = url::Url::parse(button.url.as_deref().unwrap()).unwrap();
    assert_eq!(url.path(), "/games/new");
    let id = url
        .query_pairs()
        .find(|(key, _)| key == "discord")
        .unwrap()
        .1
        .into_owned();
    let players_before = count(&ctx.app, "players").await;

    let body = ctx
        .app
        .get(&format!("/api/discord/result-drafts/{id}"))
        .await
        .assert_json(200);
    let data = &body["data"];
    assert_eq!(data["winner_discord_id"], "222");
    let seats: Vec<&str> = data["seats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seat| seat["discord_id"].as_str().unwrap())
        .collect();
    assert_eq!(seats, ["111", "222"]);
    assert_eq!(
        data["seats"][0],
        json!({"discord_id": "111", "player_id": null, "player_name": "Aria"})
    );
    assert_eq!(data["played_at"], "2026-09-20T14:00:00Z");
    assert_eq!(data["external_id"], "spellbot:SB12345");
    assert!(data["duration_minutes"].as_i64().unwrap() >= 1);
    assert_eq!(count(&ctx.app, "players").await, players_before);
    assert_eq!(count(&ctx.app, "games").await, 0);

    assert!(matches!(
        web_draft::open(&ctx.app.state, "", Some("999"), &actor()).await,
        Err(OpenError::InvalidWinner)
    ));
    for guild in ["other", ""] {
        let actor = Actor {
            guild_id: guild.into(),
            ..actor()
        };
        assert!(matches!(
            web_draft::open(&ctx.app.state, "SB12345", None, &actor).await,
            Err(OpenError::Forbidden)
        ));
    }
}

#[tokio::test]
async fn log_command_explains_missing_games_and_bad_winners() {
    let ctx = setup().await;
    let missing = interaction(
        "444",
        "999",
        command("log", vec![("game", OptionValue::String("SB4040".into()))]),
    );
    assert!(
        log_command::handle(&ctx.app.state, &missing)
            .await
            .content()
            .contains("No unfinished game found")
    );
    let bad_winner = interaction(
        "444",
        "999",
        command("log", vec![("winner", OptionValue::User("1".into()))]),
    );
    assert!(
        log_command::handle(&ctx.app.state, &bad_winner)
            .await
            .content()
            .contains("Choose a winner")
    );
    let mut foreign = interaction("444", "999", command("log", vec![]));
    foreign.guild_id = Some("1".into());
    assert!(
        log_command::handle(&ctx.app.state, &foreign)
            .await
            .content()
            .contains("Use /log in the game's server")
    );
}

#[tokio::test]
async fn saving_creates_the_discord_identities_and_game_atomically_ignores_forged_provenance_and_consumes_drafts()
 {
    let ctx = setup().await;
    let draft = open(&ctx).await;
    let second = open(&ctx).await;
    let mut forged = payload();
    forged["source"] = json!("manual");
    forged["external_id"] = json!("forged");
    forged["created_by_user_id"] = json!(-1);
    let body = ctx
        .app
        .post(
            &format!("/api/discord/result-drafts/{draft}"),
            json!({"game": forged}),
        )
        .await
        .assert_json(201);
    let game = ctx
        .app
        .state
        .games
        .get_game(body["data"]["id"].as_i64().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(game.source.as_str(), "discord");
    assert_eq!(game.external_id.as_deref(), Some("spellbot:SB12345"));
    assert_eq!(game.created_by_user_id, Some(ctx.user.id));
    assert_eq!(game.notes.as_deref(), Some("Web log note"));
    let seats: Vec<(Option<&str>, GameResult, Option<i64>)> = game
        .seats
        .iter()
        .map(|seat| (seat.player.discord_id.as_deref(), seat.result, seat.kills))
        .collect();
    assert_eq!(
        seats,
        [
            (Some("222"), GameResult::Win, Some(1)),
            (Some("111"), GameResult::Loss, Some(0))
        ]
    );
    assert_eq!(
        game.seats[0].deck.as_ref().unwrap().commander_name,
        "Bello, Bard of the Brambles"
    );
    assert!(
        !exists(
            &ctx.app,
            "pending_discord_games",
            &ctx.pending_id.to_string()
        )
        .await
    );
    assert!(!exists(&ctx.app, "discord_result_drafts", &draft).await);
    ctx.app
        .post(
            &format!("/api/discord/result-drafts/{second}"),
            json!({"game": payload()}),
        )
        .await
        .assert_json(404);
    assert_eq!(count(&ctx.app, "games").await, 1);
}

#[tokio::test]
async fn invalid_game_data_rolls_back_new_players_and_decks_and_preserves_pending_state() {
    let ctx = setup().await;
    let draft = open(&ctx).await;
    let players_before = count(&ctx.app, "players").await;
    let decks_before = count(&ctx.app, "decks").await;
    let mut invalid = payload();
    invalid["turns"] = json!(-1);
    ctx.app
        .post(
            &format!("/api/discord/result-drafts/{draft}"),
            json!({"game": invalid}),
        )
        .await
        .assert_json(422);
    assert_eq!(count(&ctx.app, "players").await, players_before);
    assert_eq!(count(&ctx.app, "decks").await, decks_before);
    assert_eq!(count(&ctx.app, "games").await, 0);
    assert!(
        exists(
            &ctx.app,
            "pending_discord_games",
            &ctx.pending_id.to_string()
        )
        .await
    );
    assert!(exists(&ctx.app, "discord_result_drafts", &draft).await);
    ctx.app
        .post(
            &format!("/api/discord/result-drafts/{draft}"),
            json!({"game": payload()}),
        )
        .await
        .assert_json(201);
}

#[tokio::test]
async fn rejects_changed_duplicate_or_missing_roster_identities_and_another_players_deck() {
    let ctx = setup().await;
    let draft = open(&ctx).await;
    let seats = payload()["seats"].clone();
    let (first, last) = (seats[0].clone(), seats[1].clone());
    let mut renamed = first.clone();
    renamed["discord_id"] = json!("999");
    for seats in [
        json!([first]),
        json!([first, first]),
        json!([renamed, last]),
        json!([null, last]),
    ] {
        let mut attrs = payload();
        attrs["seats"] = seats;
        ctx.app
            .post(
                &format!("/api/discord/result-drafts/{draft}"),
                json!({"game": attrs}),
            )
            .await
            .assert_json(400);
    }
    let other = ctx
        .app
        .state
        .games
        .resolve_player("Other", Some("777"), None)
        .await
        .unwrap();
    let deck = ctx
        .app
        .deck_with(
            json!({"player_id": other.id, "name": "Other's deck", "commander_name": "Bello"}),
        )
        .await;
    let mut with_deck = first.clone();
    with_deck["deck_id"] = json!(deck.id);
    let mut attrs = payload();
    attrs["seats"] = json!([with_deck, last]);
    ctx.app
        .post(
            &format!("/api/discord/result-drafts/{draft}"),
            json!({"game": attrs}),
        )
        .await
        .assert_json(400);
    ctx.app
        .post(
            &format!("/api/discord/result-drafts/{draft}"),
            json!({"nope": true}),
        )
        .await
        .assert_json(400);
    assert_eq!(count(&ctx.app, "games").await, 0);
}

#[tokio::test]
async fn links_require_authentication_and_owner_identity_and_expire_or_invalidate_on_roster_change()
{
    let ctx = setup().await;
    let draft = open(&ctx).await;
    let path = format!("/api/discord/result-drafts/{draft}");
    ctx.app.clear_cookies();
    ctx.app.get(&path).await.assert_json(401);
    let other = ctx.app.unique_member().await;
    ctx.app.log_in(&other).await;
    ctx.app.get(&path).await.assert_json(404);
    ctx.app
        .post(&path, json!({"game": payload()}))
        .await
        .assert_json(404);
    let disabled = User {
        disabled_at: Some(UtcDateTime::now()),
        ..ctx.user.clone()
    };
    assert!(
        web_draft::preview(&ctx.app.state, &draft, &disabled)
            .await
            .unwrap()
            .is_none()
    );
    ctx.app.log_in(&ctx.user).await;
    sqlx::query(
        "UPDATE discord_result_drafts SET expires_at = '2020-01-01T00:00:00Z' WHERE id = ?",
    )
    .bind(&draft)
    .execute(ctx.app.pool())
    .await
    .unwrap();
    ctx.app.get(&path).await.assert_json(404);
    let draft = open(&ctx).await;
    let mut reversed = draft_report();
    reversed.players.reverse();
    discord::stage_report(ctx.app.pool(), &reversed)
        .await
        .unwrap();
    ctx.app
        .post(
            &format!("/api/discord/result-drafts/{draft}"),
            json!({"game": payload()}),
        )
        .await
        .assert_json(404);
    ctx.app
        .get("/api/discord/result-drafts/not-a-uuid")
        .await
        .assert_json(404);
}
