//! Ported from `test/the_gathering/discord/won_command_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::HashMap;

use serde_json::json;
use support::TestApp;
use support::discord::{
    Call, RecordingApi, command, component, interaction, modal, player, report, string,
};
use the_gathering::config::DiscordBotConfig;
use the_gathering::db::UtcDateTime;
use the_gathering::discord::api::{Component, InteractionResponse, ResponseKind};
use the_gathering::discord::draft::{self, WonDraftData};
use the_gathering::discord::interaction::Interaction;
use the_gathering::discord::{self, GameReport, won};
use the_gathering::games::{Game, GameResult, Seat, WinCondition};

async fn app() -> TestApp {
    TestApp::with_config(|config| {
        config.discord_bot = Some(DiscordBotConfig {
            token: "test-token".into(),
            guild_id: Some("333".into()),
            spellbot_user_id: "725510263251402832".into(),
        });
    })
    .await
}

async fn stage(app: &TestApp, count: usize) -> GameReport {
    let played_at = UtcDateTime::now().plus(time::Duration::minutes(-90));
    let players = (1..=count)
        .map(|n| player(&(110 + n).to_string(), &format!("Player {n}"), None))
        .collect();
    let report = report(played_at, players);
    discord::stage_report(app.pool(), &report).await.unwrap();
    report
}

async fn card(app: &TestApp, id: &str, name: &str, commander: bool, colors: &[&str]) {
    app.card(id, name, colors, json!({}), commander).await;
}

async fn handle(app: &TestApp, event: &Interaction) -> InteractionResponse {
    won::handle(&app.state, event).await
}

async fn open_as(app: &TestApp, user: &str) -> String {
    let response = handle(app, &interaction("444", user, command("won", vec![]))).await;
    assert_eq!(response.kind, ResponseKind::Modal, "{}", response.content());
    let custom_id = &response.modal_data().unwrap().custom_id;
    let rest = custom_id.strip_prefix("won:").unwrap();
    let (id, action) = rest.split_once(':').unwrap();
    assert_eq!(action, "details");
    id.to_owned()
}

async fn open(app: &TestApp) -> String {
    open_as(app, "111").await
}

async fn click_as(
    app: &TestApp,
    id: &str,
    action: &str,
    value: Option<&str>,
    user: &str,
) -> InteractionResponse {
    let event = interaction("444", user, component(&format!("won:{id}:{action}"), value));
    handle(app, &event).await
}

async fn click(app: &TestApp, id: &str, action: &str, value: Option<&str>) -> InteractionResponse {
    click_as(app, id, action, value, "111").await
}

async fn submit_from(
    app: &TestApp,
    id: &str,
    action: &str,
    fields: &[(&str, &str)],
    from_message: bool,
) -> InteractionResponse {
    let mut event = interaction("444", "111", modal(&format!("won:{id}:{action}"), fields));
    if from_message {
        event.message_id = Some("999".into());
    }
    handle(app, &event).await
}

async fn submit(
    app: &TestApp,
    id: &str,
    action: &str,
    fields: &[(&str, &str)],
) -> InteractionResponse {
    submit_from(app, id, action, fields, false).await
}

async fn count(app: &TestApp, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(app.pool())
        .await
        .unwrap()
}

async fn draft_data(app: &TestApp, id: &str) -> Option<WonDraftData> {
    let mut conn = app.pool().acquire().await.unwrap();
    draft::get(&mut conn, id)
        .await
        .unwrap()
        .map(|draft| serde_json::from_str(&draft.data).unwrap())
}

async fn the_game(app: &TestApp) -> Game {
    let id: i64 = sqlx::query_scalar("SELECT id FROM games")
        .fetch_one(app.pool())
        .await
        .unwrap();
    app.state.games.get_game(id).await.unwrap().unwrap()
}

fn seats_by_discord(game: &Game) -> HashMap<String, Seat> {
    game.seats
        .iter()
        .map(|seat| (seat.player.discord_id.clone().unwrap(), seat.clone()))
        .collect()
}

fn rows(response: &InteractionResponse) -> Vec<Component> {
    match (response.modal_data(), response.message_data()) {
        (Some(modal), _) => modal.components.clone(),
        (_, Some(message)) => message.rows().to_vec(),
        _ => Vec::new(),
    }
}

#[tokio::test]
async fn six_player_modal_flow_saves_only_on_confirmation_with_correctly_attributed_data() {
    let app = app().await;
    stage(&app, 6).await;
    card(&app, "sol-ring", "Sol Ring", false, &[]).await;
    let id = open(&app).await;
    assert_eq!(count(&app, "games").await, 0);
    let duration = draft_data(&app, &id).await.unwrap().duration.unwrap();
    assert!(["90", "91"].contains(&duration.as_str()), "{duration}");

    let review = submit(
        &app,
        &id,
        "details",
        &[
            ("turns", "7"),
            ("duration", "95"),
            ("mvp", "sol ring"),
            ("notes", "Close finish\n@everyone"),
        ],
    )
    .await;
    assert_eq!(review.kind, ResponseKind::ChannelMessage);
    let data = review.message_data().unwrap();
    assert_eq!(data.flags, Some(64));
    assert_eq!(
        serde_json::to_value(&data.allowed_mentions).unwrap(),
        json!({"parse": []})
    );
    assert!(data.rows().len() <= 5);
    click(&app, &id, "winner", Some("112")).await;
    click(&app, &id, "condition", Some("poison")).await;

    let first = click(&app, &id, "kills0", None).await;
    assert_eq!(first.kind, ResponseKind::Modal);
    assert_eq!(rows(&first).len(), 5);
    let second = click(&app, &id, "kills1", None).await;
    assert_eq!(second.kind, ResponseKind::Modal);
    let second_rows = rows(&second);
    assert_eq!(second_rows.len(), 1);
    assert_eq!(second_rows[0].children()[0].custom_id(), Some("kills_116"));

    submit(
        &app,
        &id,
        "kills0",
        &[
            ("kills_111", "0"),
            ("kills_112", "3"),
            ("kills_113", "1"),
            ("kills_114", ""),
            ("kills_115", "0"),
        ],
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("every kills page")
    );
    assert_eq!(count(&app, "games").await, 0);
    submit(&app, &id, "kills1", &[("kills_116", "1")]).await;
    assert_eq!(count(&app, "games").await, 0);
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded SB12345")
    );

    let game = the_game(&app).await;
    assert_eq!(game.turns, Some(7));
    assert_eq!(game.duration_minutes, Some(95));
    assert_eq!(game.win_condition, Some(WinCondition::Poison));
    assert_eq!(game.notes.as_deref(), Some("Close finish\n@everyone"));
    let seats = seats_by_discord(&game);
    assert_eq!(seats["112"].result, GameResult::Win);
    assert_eq!(seats["112"].mvp_card_id.as_deref(), Some("sol-ring"));
    assert_eq!(seats["112"].mvp_card_name.as_deref(), Some("Sol Ring"));
    for (discord_id, seat) in &seats {
        if discord_id != "112" {
            assert_eq!(seat.result, GameResult::Loss);
            assert_eq!(seat.mvp_card_id, None);
        }
    }
    let kills: HashMap<&str, Option<i64>> = seats
        .iter()
        .map(|(id, seat)| (id.as_str(), seat.kills))
        .collect();
    assert_eq!(
        kills,
        HashMap::from([
            ("111", Some(0)),
            ("112", Some(3)),
            ("113", Some(1)),
            ("114", None),
            ("115", Some(0)),
            ("116", Some(1)),
        ])
    );
    assert!(
        discord::get_pending_by_external_id(app.pool(), "spellbot:SB12345")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&app, "discord_result_drafts").await, 0);
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("expired")
    );
    assert_eq!(count(&app, "games").await, 1);
}

#[tokio::test]
async fn cancelling_or_dismissing_a_modal_leaves_the_game_pending() {
    let app = app().await;
    stage(&app, 3).await;
    let id = open(&app).await;
    assert_eq!(count(&app, "games").await, 0);
    assert!(
        click(&app, &id, "cancel", None)
            .await
            .content()
            .contains("cancelled")
    );
    assert!(draft_data(&app, &id).await.is_none());
    assert!(
        discord::get_pending_by_external_id(app.pool(), "spellbot:SB12345")
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(count(&app, "games").await, 0);
}

#[tokio::test]
async fn ambiguous_mvp_is_selected_privately_and_persisted_on_the_winner_not_the_reporter() {
    let app = app().await;
    stage(&app, 2).await;
    card(&app, "ring", "Sol Ring", false, &[]).await;
    card(&app, "talisman", "Sol Talisman", false, &[]).await;
    let id = open(&app).await;
    let review = submit(
        &app,
        &id,
        "details",
        &[
            ("turns", ""),
            ("duration", ""),
            ("mvp", "Sol"),
            ("notes", ""),
        ],
    )
    .await;
    assert!(review.content().contains("Choose a matching MVP"));
    assert_eq!(rows(&review).len(), 5);
    assert!(
        click(&app, &id, "mvp", Some("forged-id"))
            .await
            .content()
            .contains("matching MVP")
    );
    assert!(
        click(&app, &id, "mvp", Some("talisman"))
            .await
            .content()
            .contains("MVP: Sol Talisman")
    );
    click(&app, &id, "winner", Some("112")).await;
    submit(
        &app,
        &id,
        "kills0",
        &[("kills_111", "0"), ("kills_112", "1")],
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded")
    );
    let game = the_game(&app).await;
    assert_eq!(game.turns, None);
    assert_eq!(game.duration_minutes, None);
    let winner = game.winner().unwrap();
    assert_eq!(winner.player.discord_id.as_deref(), Some("112"));
    assert_eq!(winner.mvp_card_name.as_deref(), Some("Sol Talisman"));
}

#[tokio::test]
async fn invalid_numbers_unrecognized_mvp_and_forged_choices_never_consume_the_pending_game() {
    let app = app().await;
    stage(&app, 2).await;
    let id = open(&app).await;
    let details = |turns: &'static str, mvp: &'static str| {
        [
            ("turns", turns),
            ("duration", "90"),
            ("mvp", mvp),
            ("notes", "Keep this note"),
        ]
    };
    submit(&app, &id, "details", &details("-1", "no such card")).await;
    submit(
        &app,
        &id,
        "kills0",
        &[("kills_111", "0"), ("kills_112", "1")],
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Turns must be")
    );
    assert!(
        click(&app, &id, "winner", Some("999"))
            .await
            .content()
            .contains("this game's players")
    );
    assert!(
        click(&app, &id, "condition", Some("made_up"))
            .await
            .content()
            .contains("valid win condition")
    );
    assert!(
        click(&app, &id, "condition", Some("draw"))
            .await
            .content()
            .contains("valid win condition")
    );

    submit(&app, &id, "details", &details("8", "no such card")).await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("MVP card not found")
    );

    submit(&app, &id, "details", &details("8", "")).await;
    submit(
        &app,
        &id,
        "kills0",
        &[("kills_111", "6"), ("kills_112", "1")],
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("kills must be")
    );
    assert_eq!(count(&app, "games").await, 0);
    assert_eq!(
        draft_data(&app, &id).await.unwrap().notes.as_deref(),
        Some("Keep this note")
    );
    submit(
        &app,
        &id,
        "kills0",
        &[("kills_111", "0"), ("kills_112", "1")],
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded")
    );
}

#[tokio::test]
async fn draft_is_bound_to_the_reporter_guild_and_channel_and_expires_or_invalidates_on_roster_changes()
 {
    let app = app().await;
    let staged = stage(&app, 3).await;
    let id = open(&app).await;

    let stranger = handle(&app, &interaction("444", "999", command("won", vec![]))).await;
    assert!(stranger.content().contains("participant"));
    assert!(
        click_as(&app, &id, "save", None, "112")
            .await
            .content()
            .contains("not yours")
    );

    for change in 0..3 {
        let mut event = interaction("444", "111", component(&format!("won:{id}:save"), None));
        match change {
            0 => event.guild_id = None,
            1 => event.guild_id = Some("999".into()),
            _ => event.channel_id = Some("999".into()),
        }
        assert!(handle(&app, &event).await.content().contains("not yours"));
    }

    let expired = UtcDateTime::now().plus(time::Duration::seconds(-1));
    sqlx::query("UPDATE discord_result_drafts SET expires_at = ? WHERE id = ?")
        .bind(expired)
        .bind(&id)
        .execute(app.pool())
        .await
        .unwrap();
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("expired")
    );

    let second = open(&app).await;
    let mut reversed = staged.clone();
    reversed.players.reverse();
    discord::stage_report(app.pool(), &reversed).await.unwrap();
    assert!(
        click(&app, &second, "save", None)
            .await
            .content()
            .contains("changed")
    );
    assert_eq!(count(&app, "games").await, 0);
}

#[tokio::test]
async fn a_second_participants_old_draft_and_restaged_completed_games_cannot_overwrite_the_result()
{
    let app = app().await;
    let staged = stage(&app, 2).await;
    let first = open(&app).await;
    let second = open_as(&app, "112").await;
    submit(
        &app,
        &first,
        "details",
        &[
            ("turns", "5"),
            ("duration", "60"),
            ("mvp", ""),
            ("notes", "Original"),
        ],
    )
    .await;
    submit(
        &app,
        &first,
        "kills0",
        &[("kills_111", "1"), ("kills_112", "0")],
    )
    .await;
    click(&app, &first, "save", None).await;
    assert!(
        click_as(&app, &second, "save", None, "112")
            .await
            .content()
            .contains("expired")
    );
    discord::stage_report(app.pool(), &staged).await.unwrap();
    let event = interaction(
        "444",
        "111",
        command("won", vec![("game", string("SB12345"))]),
    );
    assert!(
        handle(&app, &event)
            .await
            .content()
            .contains("already been recorded")
    );
    assert_eq!(the_game(&app).await.notes.as_deref(), Some("Original"));
}

#[tokio::test]
async fn editing_through_a_button_updates_the_private_review_and_preserves_the_full_note() {
    let app = app().await;
    stage(&app, 2).await;
    let id = open(&app).await;
    let notes = "A".repeat(4000);
    let review = submit(
        &app,
        &id,
        "details",
        &[
            ("turns", "8"),
            ("duration", "91"),
            ("mvp", ""),
            ("notes", &notes),
        ],
    )
    .await;
    assert!(review.content().chars().count() < 2000);
    assert_eq!(
        draft_data(&app, &id).await.unwrap().notes.as_deref(),
        Some(notes.as_str())
    );
    let details = click(&app, &id, "details", None).await;
    assert_eq!(details.kind, ResponseKind::Modal);
    let detail_rows = rows(&details);
    let Component::TextInput(input) = &detail_rows[3].children()[0] else {
        panic!("not a text input")
    };
    assert_eq!(input.value.as_deref(), Some(notes.as_str()));

    let review = submit_from(
        &app,
        &id,
        "kills0",
        &[("kills_111", "1"), ("kills_112", "0")],
        true,
    )
    .await;
    assert_eq!(review.kind, ResponseKind::UpdateMessage);
    let data = review.message_data().unwrap();
    assert_eq!(data.flags, None);
    assert!(data.allowed_mentions.is_some());
    assert!(review.content().contains("Player 1: 1 · Player 2: 0"));
    let kills1 = format!("won:{id}:kills1");
    assert!(
        !data
            .row_components()
            .iter()
            .flat_map(|component| std::iter::once(*component).chain(component.children()))
            .any(|component| component.custom_id() == Some(kills1.as_str()))
    );
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded")
    );
    assert_eq!(the_game(&app).await.notes.as_deref(), Some(notes.as_str()));
}

#[tokio::test]
async fn http_204_acknowledgement_is_accepted_for_opening_the_modal() {
    let app = app().await;
    stage(&app, 2).await;
    let api = RecordingApi::new();
    won::respond(
        &app.state,
        api.as_ref(),
        &interaction("444", "111", command("won", vec![])),
    )
    .await;
    let Call::Response(response) = api.next() else {
        panic!("expected a response")
    };
    assert_eq!(response.kind, ResponseKind::Modal);
    let rows = rows(&response);
    assert_eq!(rows.len(), 4);
    for row in &rows {
        assert!(matches!(row, Component::ActionRow(_)));
        assert!(matches!(row.children(), [Component::TextInput(_)]));
    }
}

#[tokio::test]
async fn partial_commanders_and_punctuation_free_mvp_names_resolve_and_save_on_the_correct_seats() {
    let app = app().await;
    stage(&app, 6).await;
    card(&app, "will", "Jeska's Will", false, &[]).await;
    card(&app, "lumra", "Lumra, Bellow of the Woods", true, &["G"]).await;
    card(
        &app,
        "ardenn",
        "Ardenn, Intrepid Archaeologist",
        true,
        &["W"],
    )
    .await;
    card(&app, "kediss", "Kediss, Emberclaw Familiar", true, &["R"]).await;
    let player = app
        .state
        .games
        .resolve_player("Player 1", Some("111"), None)
        .await
        .unwrap();
    let deck = app
        .deck_with(json!({
            "player_id": player.id,
            "name": "My voltron deck",
            "commander_name": "Kediss, Emberclaw Familiar",
            "partner_name": "Ardenn, Intrepid Archaeologist",
        }))
        .await;
    let id = open(&app).await;
    let review = submit(
        &app,
        &id,
        "details",
        &[
            ("mvp", "Jeskas will"),
            ("turns", ""),
            ("duration", ""),
            ("notes", ""),
        ],
    )
    .await;
    assert!(review.content().contains("MVP: Jeska's Will"));
    assert!(
        click(&app, &id, "commanders", None)
            .await
            .content()
            .contains("Player 6: Not recorded")
    );

    let commander_modal = click(&app, &id, "player", Some("116")).await;
    assert_eq!(commander_modal.kind, ResponseKind::Modal);
    assert_eq!(
        commander_modal.modal_data().unwrap().custom_id,
        format!("won:{id}:commander_116")
    );
    assert_eq!(rows(&commander_modal).len(), 2);
    let panel = submit_from(
        &app,
        &id,
        "commander_116",
        &[("commander", "lumra"), ("partner", "")],
        true,
    )
    .await;
    assert_eq!(panel.kind, ResponseKind::UpdateMessage);
    assert_eq!(panel.message_data().unwrap().flags, None);
    assert!(
        panel
            .content()
            .contains("Player 6: Lumra, Bellow of the Woods")
    );
    assert!(rows(&panel).len() <= 5);
    submit_from(
        &app,
        &id,
        "commander_111",
        &[("commander", "ardenn"), ("partner", "kediss")],
        true,
    )
    .await;
    assert_eq!(count(&app, "games").await, 0);
    assert!(
        click(&app, &id, "review", None)
            .await
            .content()
            .contains("Commander entries: 2")
    );
    submit(
        &app,
        &id,
        "kills0",
        &[
            ("kills_111", "0"),
            ("kills_112", "0"),
            ("kills_113", "0"),
            ("kills_114", "0"),
            ("kills_115", "0"),
        ],
    )
    .await;
    submit(&app, &id, "kills1", &[("kills_116", "0")]).await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded")
    );
    let seats = seats_by_discord(&the_game(&app).await);
    assert_eq!(seats["111"].deck_id, Some(deck.id));
    assert_eq!(seats["111"].mvp_card_name.as_deref(), Some("Jeska's Will"));
    let lumra = seats["116"].deck.clone().unwrap();
    assert_eq!(lumra.commander_card_id.as_deref(), Some("lumra"));
    assert_eq!(lumra.color_identity, "G");
    assert_eq!(seats["112"].deck_id, None);
}

#[tokio::test]
async fn ambiguous_commander_and_partner_choices_are_validated_then_create_a_catalog_linked_pair() {
    let app = app().await;
    stage(&app, 2).await;
    card(&app, "akroma1", "Akroma, Angel of Wrath", true, &["W"]).await;
    card(&app, "akroma2", "Akroma, Vision of Ixidor", true, &["W"]).await;
    card(
        &app,
        "sakashima1",
        "Sakashima of a Thousand Faces",
        true,
        &["U"],
    )
    .await;
    card(&app, "sakashima2", "Sakashima the Impostor", true, &["U"]).await;
    let id = open(&app).await;
    submit(
        &app,
        &id,
        "details",
        &[("mvp", ""), ("turns", ""), ("duration", ""), ("notes", "")],
    )
    .await;
    submit(
        &app,
        &id,
        "kills0",
        &[("kills_111", "1"), ("kills_112", "0")],
    )
    .await;
    let panel = submit_from(
        &app,
        &id,
        "commander_112",
        &[("commander", "akroma"), ("partner", "sakashima")],
        true,
    )
    .await;
    assert_eq!(rows(&panel).len(), 4);
    assert!(panel.content().contains("Choose a matching Commander"));
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Player 2: Choose")
    );
    assert!(
        click(&app, &id, "player", Some("999"))
            .await
            .content()
            .contains("Select a player")
    );
    assert!(
        submit(&app, &id, "commander_999", &[("commander", "akroma")])
            .await
            .content()
            .contains("Select a player")
    );
    assert!(
        click_as(&app, &id, "commanders", None, "112")
            .await
            .content()
            .contains("not yours")
    );
    assert!(
        click(&app, &id, "commander_choice_112", Some("sakashima1"))
            .await
            .content()
            .contains("matching cards")
    );
    assert!(
        click(&app, &id, "commander_choice_112", Some("akroma2"))
            .await
            .content()
            .contains("Akroma, Vision")
    );
    assert!(
        click(&app, &id, "partner_choice_112", Some("sakashima1"))
            .await
            .content()
            .contains("Sakashima of a Thousand Faces")
    );
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded")
    );
    let seats = seats_by_discord(&the_game(&app).await);
    let deck = seats["112"].deck.clone().unwrap();
    assert_eq!(deck.commander_card_id.as_deref(), Some("akroma2"));
    assert_eq!(deck.partner_card_id.as_deref(), Some("sakashima1"));
    assert_eq!(deck.color_identity, "WU");
    assert_eq!(seats["112"].result, GameResult::Loss);
}

#[tokio::test]
async fn unresolved_or_invalid_commanders_block_saving_and_can_be_cleared_without_losing_other_details()
 {
    let app = app().await;
    stage(&app, 2).await;
    card(&app, "ring", "Sol Ring", false, &[]).await;
    card(&app, "lumra", "Lumra, Bellow of the Woods", true, &[]).await;
    let id = open(&app).await;
    submit(
        &app,
        &id,
        "details",
        &[
            ("mvp", "ring"),
            ("turns", "9"),
            ("duration", "75"),
            ("notes", "Keep me"),
        ],
    )
    .await;
    submit(
        &app,
        &id,
        "kills0",
        &[("kills_111", "1"), ("kills_112", "0")],
    )
    .await;
    submit_from(
        &app,
        &id,
        "commander_111",
        &[("commander", "Sol Ring"), ("partner", "")],
        true,
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Commander card not found")
    );
    submit_from(
        &app,
        &id,
        "commander_111",
        &[("commander", ""), ("partner", "lumra")],
        true,
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("before adding a partner")
    );
    submit_from(
        &app,
        &id,
        "commander_111",
        &[("commander", "lumra"), ("partner", "lumra")],
        true,
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("different cards")
    );
    assert_eq!(count(&app, "games").await, 0);
    submit_from(
        &app,
        &id,
        "commander_111",
        &[("commander", ""), ("partner", "")],
        true,
    )
    .await;
    assert!(
        click(&app, &id, "save", None)
            .await
            .content()
            .contains("Recorded")
    );
    let game = the_game(&app).await;
    assert_eq!(game.notes.as_deref(), Some("Keep me"));
    assert_eq!(game.turns, Some(9));
    assert!(game.seats.iter().all(|seat| seat.deck_id.is_none()));
}

#[tokio::test]
async fn malformed_custom_ids_and_actions_are_rejected() {
    let app = app().await;
    stage(&app, 2).await;
    let id = open(&app).await;
    assert!(
        click(&app, "x:y", "save", None)
            .await
            .content()
            .contains("Invalid result form")
    );
    assert!(
        click(&app, &id, "bogus", None)
            .await
            .content()
            .contains("Invalid result action")
    );
    assert!(
        submit(&app, &id, "winner", &[])
            .await
            .content()
            .contains("Invalid result action")
    );
}
