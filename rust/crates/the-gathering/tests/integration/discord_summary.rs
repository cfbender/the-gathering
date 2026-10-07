//! The Discord `/summary` command and its image upload (the authenticated PNG preview
//! test lives in `games_api.rs`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use std::time::Duration;

use serde_json::{Value, json};
use support::TestApp;
use support::discord::{Call, RecordingApi, command, interaction, string};
use the_gathering::accounts::User;
use the_gathering::config::DiscordBotConfig;
use the_gathering::discord::api::{
    AllowedMentions, AttachmentMeta, DiscordApi, DiscordError, FileUpload, InteractionResponse,
    InteractionTarget, MessagePayload, ResponseKind, SentMessage,
};
use the_gathering::discord::interaction::Interaction;
use the_gathering::discord::rest::RestApi;
use the_gathering::discord::summary::{self, Rejection};
use the_gathering::games::{Game, GamesError, Player};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Ctx {
    app: TestApp,
    user: User,
    attrs: Value,
    interaction: Interaction,
}

async fn setup() -> Ctx {
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
    let alice: Player = app.player("Alice").await;
    let bob: Player = app.player("Bob").await;
    let attrs = json!({
        "played_at": "2026-09-20T20:00:00Z",
        "win_condition": "combat_damage",
        "notes": "A close finish",
        "seats": [
            {"player_id": alice.id, "seat": 1, "result": "loss", "kills": 0},
            {"player_id": bob.id, "seat": 2, "result": "win", "kills": 1},
        ],
    });
    let mut interaction = interaction("1", "551122", command("summary", vec![]));
    interaction.guild_id = Some("9090".into());
    Ctx {
        app,
        user,
        attrs,
        interaction,
    }
}

async fn game(ctx: &Ctx, attrs: &Value) -> Game {
    ctx.app.game(attrs.clone(), None).await
}

#[tokio::test]
async fn latest_uses_played_at_then_id_not_creation_order_and_loads_seats() {
    let ctx = setup().await;
    let games = &ctx.app.state.games;
    assert!(matches!(
        games.find_summary_game("").await,
        Err(GamesError::NotFound)
    ));
    let first = game(&ctx, &ctx.attrs).await;
    let latest = game(&ctx, &ctx.attrs).await;
    let mut older_attrs = ctx.attrs.clone();
    older_attrs["played_at"] = json!("2025-01-01T00:00:00Z");
    let older = game(&ctx, &older_attrs).await;
    assert!(older.id > latest.id);
    let found = games.find_summary_game("").await.unwrap();
    assert_eq!(found.id, latest.id);
    let mut seats = found.seats.clone();
    seats.sort_by_key(|seat| seat.seat);
    let names: Vec<&str> = seats.iter().map(|seat| seat.player.name.as_str()).collect();
    assert_eq!(names, ["Alice", "Bob"]);
    assert_eq!(
        games
            .find_summary_game(&first.id.to_string())
            .await
            .unwrap()
            .id,
        first.id
    );
}

#[tokio::test]
async fn spellbot_ids_are_explicit_source_scoped_and_case_insensitive() {
    let ctx = setup().await;
    let games = &ctx.app.state.games;
    let local = game(&ctx, &ctx.attrs).await;
    let discord = games
        .find_or_create_game_by_external_id(
            "discord",
            &format!("spellbot:SB{}", local.id),
            &ctx.attrs,
        )
        .await
        .unwrap();
    games
        .find_or_create_game_by_external_id("csv", "spellbot:SB7654", &ctx.attrs)
        .await
        .unwrap();
    assert_eq!(
        games
            .find_summary_game(&format!(" #sb{} ", local.id))
            .await
            .unwrap()
            .id,
        discord.id
    );
    assert_eq!(
        games
            .find_summary_game(&local.id.to_string())
            .await
            .unwrap()
            .id,
        local.id
    );
    assert!(matches!(
        games.find_summary_game("SB7654").await,
        Err(GamesError::NotFound)
    ));
    for invalid in ["../../etc/passwd", "123abc", "-1", &"9".repeat(100)] {
        assert!(
            matches!(
                games.find_summary_game(invalid).await,
                Err(GamesError::BadRequest)
            ),
            "{invalid}"
        );
    }
}

#[tokio::test]
async fn active_discord_linked_members_only_guild_only_with_configured_guild_enforced() {
    let ctx = setup().await;
    let recorded = game(&ctx, &ctx.attrs).await;
    let state = &ctx.app.state;
    assert_eq!(
        summary::prepare(state, &ctx.interaction).await.unwrap().id,
        recorded.id
    );

    let mut dm = ctx.interaction.clone();
    dm.guild_id = None;
    assert!(matches!(
        summary::prepare(state, &dm).await,
        Err(Rejection::Forbidden)
    ));
    let mut foreign = ctx.interaction.clone();
    foreign.guild_id = Some("8080".into());
    assert!(matches!(
        summary::prepare(state, &foreign).await,
        Err(Rejection::Forbidden)
    ));
    let mut stranger = ctx.interaction.clone();
    stranger.user.as_mut().unwrap().id = "998877".into();
    assert!(matches!(
        summary::prepare(state, &stranger).await,
        Err(Rejection::Forbidden)
    ));

    state
        .accounts
        .update_user(&ctx.user, &json!({"disabled_at": "2026-01-01T00:00:00Z"}))
        .await
        .unwrap();
    assert!(matches!(
        summary::prepare(state, &ctx.interaction).await,
        Err(Rejection::Forbidden)
    ));
}

#[tokio::test]
async fn defers_publicly_before_uploading_png_includes_alt_text_link_and_mention_suppression() {
    let ctx = setup().await;
    let recorded = game(&ctx, &ctx.attrs).await;
    let api = RecordingApi::new();
    summary::respond(&ctx.app.state, api.as_ref(), &ctx.interaction)
        .await
        .unwrap();
    let Call::Response(ack) = api.next() else {
        panic!("expected the acknowledgement first")
    };
    assert_eq!(ack, InteractionResponse::deferred(false));
    let Call::EditResponse(response) = api.next() else {
        panic!("expected the upload")
    };
    assert_eq!(response.allowed_mentions, Some(AllowedMentions::none()));
    assert!(
        response
            .content
            .as_deref()
            .unwrap()
            .contains(&format!("/games/{}", recorded.id))
    );
    let attachments = response.attachments.clone().unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].id, 0);
    assert!(attachments[0].description.contains("Winner: Bob"));
    assert_eq!(response.files.len(), 1);
    assert_eq!(response.files[0].name, attachments[0].filename);
    assert!(
        response.files[0]
            .body
            .starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
    );
    assert!(api.is_idle());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM games")
        .fetch_one(ctx.app.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn bad_ids_and_missing_games_return_private_errors_without_deferral() {
    let ctx = setup().await;
    for (options, expected) in [
        (vec![], "No recorded game"),
        (vec![("game", string("oops"))], "Use a Gathering"),
    ] {
        let mut event = ctx.interaction.clone();
        event.data = command("summary", options);
        let api = RecordingApi::new();
        summary::respond(&ctx.app.state, api.as_ref(), &event)
            .await
            .unwrap();
        let calls = api.take();
        assert_eq!(calls.len(), 1);
        let Call::Response(response) = &calls[0] else {
            panic!("{calls:?}")
        };
        assert_eq!(response.kind, ResponseKind::ChannelMessage);
        assert_eq!(response.message_data().unwrap().flags, Some(64));
        assert!(response.content().contains(expected));
    }
}

// SummaryUpload: the REST client's interaction webhook edit.

fn target() -> InteractionTarget {
    InteractionTarget {
        id: "1".into(),
        application_id: "123".into(),
        token: "test-token".into(),
    }
}

fn client(server: &MockServer, timeout: Duration) -> RestApi {
    RestApi::with_base("bot-token", &format!("{}/api/v10", server.uri()), timeout).unwrap()
}

const WEBHOOK: &str = "/api/v10/webhooks/123/test-token/messages/@original";

/// Splits a multipart body into `(headers, content)` parts.
fn multipart_parts(content_type: &str, body: &[u8]) -> Vec<(String, Vec<u8>)> {
    let boundary = content_type
        .split("boundary=")
        .nth(1)
        .unwrap()
        .trim_matches('"');
    let delimiter = format!("--{boundary}");
    let delimiter = delimiter.as_bytes();
    let mut parts = Vec::new();
    let mut rest = body;
    while let Some(start) = find(rest, delimiter) {
        rest = &rest[start + delimiter.len()..];
        if rest.starts_with(b"--") {
            break;
        }
        let end = find(rest, delimiter).unwrap();
        let part = &rest[2..end - 2]; // strip the leading and trailing CRLF
        let split = find(part, b"\r\n\r\n").unwrap();
        parts.push((
            String::from_utf8_lossy(&part[..split]).into_owned(),
            part[split + 4..].to_vec(),
        ));
        rest = &rest[end..];
    }
    parts
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[tokio::test]
async fn patches_the_original_response_with_matching_png_and_attachment_metadata() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(WEBHOOK))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "456"})))
        .expect(1)
        .mount(&server)
        .await;
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];
    for _ in 0..192_000 {
        png.extend_from_slice(&[0, 255]);
    }
    let response = MessagePayload {
        content: Some("Game #219".into()),
        allowed_mentions: Some(AllowedMentions::none()),
        attachments: Some(vec![AttachmentMeta {
            id: 0,
            filename: "game-219.png".into(),
            description: "Winner: Alice".into(),
        }]),
        files: vec![FileUpload {
            name: "game-219.png".into(),
            body: png.clone(),
        }],
        ..MessagePayload::default()
    };
    let sent = client(&server, Duration::from_secs(15))
        .edit_response(&target(), &response)
        .await
        .unwrap();
    assert_eq!(sent, SentMessage { id: "456".into() });

    let requests = server.received_requests().await.unwrap();
    let request = &requests[0];
    assert!(request.headers.get("authorization").is_none());
    let content_type = request
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.starts_with("multipart/form-data"));
    let parts = multipart_parts(content_type, &request.body);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].0.contains("name=\"payload_json\""));
    assert_eq!(
        serde_json::from_slice::<Value>(&parts[0].1).unwrap(),
        json!({
            "content": "Game #219",
            "allowed_mentions": {"parse": []},
            "attachments": [{"id": 0, "filename": "game-219.png", "description": "Winner: Alice"}],
        })
    );
    assert!(parts[1].0.contains("name=\"files[0]\""));
    assert!(parts[1].0.contains("filename=\"game-219.png\""));
    assert!(
        parts[1]
            .0
            .to_lowercase()
            .contains("content-type: image/png")
    );
    assert_eq!(parts[1].1, png);
}

#[tokio::test]
async fn rendering_errors_can_still_replace_the_deferred_response_with_plain_text() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(WEBHOOK))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "456"})))
        .mount(&server)
        .await;
    let response = MessagePayload {
        content: Some("Render failed".into()),
        allowed_mentions: Some(AllowedMentions::none()),
        ..MessagePayload::default()
    };
    client(&server, Duration::from_secs(15))
        .edit_response(&target(), &response)
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("content-type").unwrap(),
        "application/json"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
        json!({"content": "Render failed", "allowed_mentions": {"parse": []}})
    );
}

#[tokio::test]
async fn timeouts_propagate_without_retries_and_network_waits_have_finite_limits() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let result = client(&server, Duration::from_millis(200))
        .edit_response(
            &target(),
            &MessagePayload {
                content: Some("test".into()),
                ..MessagePayload::default()
            },
        )
        .await;
    assert_eq!(result, Err(DiscordError::Timeout));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn discord_errors_and_redirects_return_once_rather_than_retrying_or_forwarding_the_token() {
    for status in [302_u16, 403, 429, 500] {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("location", "https://example.com/should-not-follow")
                    .set_body_json(json!({"code": 50_013, "retry_after": 0.01})),
            )
            .mount(&server)
            .await;
        let result = client(&server, Duration::from_secs(15))
            .edit_response(
                &target(),
                &MessagePayload {
                    content: Some("test".into()),
                    ..MessagePayload::default()
                },
            )
            .await;
        assert_eq!(
            result,
            Err(DiscordError::Http {
                status,
                code: Some(50_013)
            })
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn bot_routes_authenticate_and_wait_out_rate_limits() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/222/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"retry_after": 0.01})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/222/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "999"})))
        .mount(&server)
        .await;
    let payload = MessagePayload {
        content: Some("hello".into()),
        nonce: Some("newgame:1".into()),
        enforce_nonce: Some(true),
        ..MessagePayload::default()
    };
    let sent = client(&server, Duration::from_secs(15))
        .create_message("222", &payload)
        .await
        .unwrap();
    assert_eq!(sent.id, "999");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].headers.get("authorization").unwrap(),
        "Bot bot-token"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[1].body).unwrap(),
        json!({"content": "hello", "nonce": "newgame:1", "enforce_nonce": true})
    );
}
