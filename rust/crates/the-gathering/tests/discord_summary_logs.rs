//! The `/summary` command's log lines, in one test so a global subscriber captures them
//! without racing other tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use serde_json::json;
use support::TestApp;
use support::discord::{Call, LogCapture, Op, RecordingApi, command, interaction};
use the_gathering::config::DiscordBotConfig;
use the_gathering::discord::api::{DiscordError, ResponseKind};
use the_gathering::discord::summary;

#[tokio::test]
async fn acknowledgement_and_upload_failures_log_the_stage_and_numeric_codes_only() {
    let logs = LogCapture::global();
    let app = TestApp::with_config(|config| {
        config.discord_bot = Some(DiscordBotConfig {
            token: "test-token".into(),
            guild_id: Some("9090".into()),
            spellbot_user_id: "725510263251402832".into(),
        });
    })
    .await;
    let user = app.unique_member().await;
    sqlx::query("UPDATE users SET discord_id = '551122' WHERE id = ?")
        .bind(user.id)
        .execute(app.pool())
        .await
        .unwrap();
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    app.game(
        json!({
            "played_at": "2026-09-20T20:00:00Z",
            "seats": [
                {"player_id": alice.id, "seat": 1, "result": "loss"},
                {"player_id": bob.id, "seat": 2, "result": "win"},
            ],
        }),
        None,
    )
    .await;
    let mut event = interaction("1", "551122", command("summary", vec![]));
    event.guild_id = Some("9090".into());

    // A failed acknowledgement is not retried and never uploads.
    logs.clear();
    let api = RecordingApi::new();
    api.fail(&[Op::Response]);
    let result = summary::respond(&app.state, api.as_ref(), &event).await;
    assert_eq!(result, Err(DiscordError::Network));
    assert!(
        logs.text()
            .contains("Discord /summary acknowledge failed: network")
    );
    let calls = api.take();
    assert_eq!(calls.len(), 1);
    assert!(
        matches!(&calls[0], Call::Response(response) if response.kind == ResponseKind::DeferredChannelMessage)
    );

    // Upload failures log the stage and numeric codes without exposing response data.
    logs.clear();
    let error = DiscordError::Http {
        status: 403,
        code: Some(50_013),
    };
    let api = RecordingApi::new();
    api.set_edit_response_result(Err(error.clone()));
    event.token = "private interaction token".into();
    let result = summary::respond(&app.state, api.as_ref(), &event).await;
    assert_eq!(result, Err(error));
    let log = logs.text();
    assert!(
        log.contains("Discord /summary acknowledge completed"),
        "{log}"
    );
    assert!(log.contains("Discord /summary rendered game"), "{log}");
    assert!(log.contains("Discord /summary upload started"));
    assert!(log.contains("Discord /summary upload failed: HTTP 403, Discord code 50013"));
    assert!(!log.contains("private interaction token"));
    let calls = api.take();
    assert_eq!(calls.len(), 2);
    assert!(matches!(calls[1], Call::EditResponse(_)));
}
