//! Webcam table config and room list APIs.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;
use crate::webcam_support;

use std::time::Duration;

use serde_json::{Value, json};
use support::TestApp;
use the_gathering::config::{Config, WindowLimit};
use the_gathering::crypto;
use the_gathering::web::channels;
use webcam_support::{PEER_A, PEER_B, Server, room_id};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn signed_in(adjust: impl FnOnce(&mut Config)) -> (TestApp, the_gathering::accounts::User) {
    let app = TestApp::with_config(adjust).await;
    let user = app.member("member").await;
    app.log_in(&user).await;
    (app, user)
}

#[tokio::test]
async fn returns_authenticated_ice_configuration() {
    let (app, _) = signed_in(|config| {
        config.webcam_table.stun_urls = vec!["stun:stun.example:3478".into()];
        config.webcam_table.turn_urls = vec!["turns:turn.example:5349".into()];
        config.webcam_table.turn_username = Some("table".into());
        config.webcam_table.turn_credential = Some("secret".into());
    })
    .await;
    let response = app.get("/api/webcam-table/config").await;
    let body = response.assert_json(200);
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    let data = &body["data"];
    assert_eq!(
        (&data["max_players"], &data["minimum_height"]),
        (&json!(10), &json!(1080))
    );
    assert!(data["socket_token"].is_string());
    assert_eq!(data["sfu"], json!({ "transport": "direct" }));
    assert_eq!(
        data["ice_servers"],
        json!([
            { "urls": ["stun:stun.example:3478"] },
            { "urls": ["turns:turn.example:5349"], "username": "table", "credential": "secret" },
        ])
    );
}

#[tokio::test]
async fn issues_an_encrypted_socket_token_that_connects_as_the_signed_in_user() {
    let (app, user) = signed_in(|_| {}).await;
    let body = app.get("/api/webcam-table/config").await.assert_json(200);
    let socket_token = body["data"]["socket_token"].as_str().unwrap().to_owned();
    let session_token = app.session().user_token.unwrap();
    assert!(!socket_token.contains(&crypto::url_encode64_unpadded(&session_token)));
    let (connected, token) = channels::authenticate(&app.state, &socket_token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(connected.id, user.id);
    assert_eq!(token, session_token);
}

async fn with_cloudflare(response: ResponseTemplate) -> (MockServer, TestApp) {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/v1/turn/keys/key123/credentials/generate-ice-servers",
        ))
        .and(header("authorization", "Bearer token-abc"))
        .and(body_json(json!({ "ttl": 1234 })))
        .respond_with(response)
        .expect(1)
        .mount(&mock)
        .await;
    let uri = mock.uri();
    let (app, _) = signed_in(move |config| {
        config.webcam_table.stun_urls = vec!["stun:stun.cloudflare.com:3478".into()];
        config.cloudflare_turn.key_id = Some("key123".into());
        config.cloudflare_turn.api_token = Some("token-abc".into());
        config.cloudflare_turn.ttl_seconds = 1234;
        config.cloudflare_turn.api_base = uri;
    })
    .await;
    (mock, app)
}

#[tokio::test]
async fn appends_minted_credentials_keeping_one_udp_and_one_tls_relay_url_and_dropping_static_urls()
{
    // Cloudflare's documented response: primary and alternate ports for every transport.
    let (_mock, app) = with_cloudflare(ResponseTemplate::new(201).set_body_json(json!({
        "iceServers": [
            { "urls": ["stun:stun.cloudflare.com:3478"] },
            {
                "urls": [
                    "turn:turn.cloudflare.com:3478?transport=udp",
                    "turn:turn.cloudflare.com:443?transport=udp",
                    "turn:turn.cloudflare.com:3478?transport=tcp",
                    "turn:turn.cloudflare.com:80?transport=tcp",
                    "turns:turn.cloudflare.com:5349?transport=tcp",
                    "turns:turn.cloudflare.com:443?transport=tcp",
                ],
                "username": "short-lived-user",
                "credential": "short-lived-secret",
            },
        ]
    })))
    .await;
    // Four URLs in total: Firefox warns that five or more slow ICE discovery.
    let body = app.get("/api/webcam-table/config").await.assert_json(200);
    assert_eq!(
        body["data"]["ice_servers"],
        json!([
            { "urls": ["stun:stun.cloudflare.com:3478"] },
            {
                "urls": ["turn:turn.cloudflare.com:3478?transport=udp", "turns:turn.cloudflare.com:443?transport=tcp"],
                "username": "short-lived-user",
                "credential": "short-lived-secret",
            },
        ])
    );
}

#[tokio::test]
async fn passes_an_unfamiliar_relay_url_set_through_rather_than_dropping_the_relay() {
    let (_mock, app) = with_cloudflare(ResponseTemplate::new(201).set_body_json(json!({
        "iceServers": [{ "urls": ["turn:relay.example:9000?transport=udp"], "username": "u", "credential": "c" }]
    })))
    .await;
    let body = app.get("/api/webcam-table/config").await.assert_json(200);
    let servers = body["data"]["ice_servers"].as_array().unwrap();
    assert_eq!(servers.len(), 2);
    assert_eq!(
        servers[0],
        json!({ "urls": ["stun:stun.cloudflare.com:3478"] })
    );
    assert_eq!(
        servers[1]["urls"],
        json!(["turn:relay.example:9000?transport=udp"])
    );
}

#[tokio::test]
async fn falls_back_to_the_static_servers_when_cloudflare_rejects_the_key() {
    let (_mock, app) = with_cloudflare(
        ResponseTemplate::new(401)
            .set_body_json(json!({ "success": false, "errors": [{ "message": "Unauthorized" }] })),
    )
    .await;
    let body = app.get("/api/webcam-table/config").await.assert_json(200);
    assert_eq!(
        body["data"]["ice_servers"],
        json!([{ "urls": ["stun:stun.cloudflare.com:3478"] }])
    );
}

#[tokio::test]
async fn relay_only_mode_tells_browsers_media_goes_through_turn() {
    let (app, _) = signed_in(|config| {
        config.sfu.relay_only = true;
        config.cloudflare_turn.key_id = Some("key123".into());
        config.cloudflare_turn.api_token = Some("token-abc".into());
        config.cloudflare_turn.api_base = "http://127.0.0.1:9".into();
    })
    .await;
    let body = app.get("/api/webcam-table/config").await.assert_json(200);
    assert_eq!(body["data"]["sfu"], json!({ "transport": "relay" }));

    // Relay-only without a Cloudflare key keeps listening directly.
    let (app, _) = signed_in(|config| config.sfu.relay_only = true).await;
    let body = app.get("/api/webcam-table/config").await.assert_json(200);
    assert_eq!(body["data"]["sfu"], json!({ "transport": "direct" }));
}

#[tokio::test]
async fn limits_credential_minting_per_account() {
    let (app, _) = signed_in(|config| {
        config.rate_limits.turn_credentials = WindowLimit {
            limit: 1,
            scale: Duration::from_secs(300),
        };
    })
    .await;
    app.get("/api/webcam-table/config").await.assert_json(200);
    let limited = app.get("/api/webcam-table/config").await;
    assert_eq!(
        limited.assert_json(429),
        json!({ "errors": { "detail": "Too Many Requests" } })
    );
    assert!(limited.header("retry-after").is_some());

    let other = app.member("other").await;
    app.log_in(&other).await;
    app.get("/api/webcam-table/config").await.assert_json(200);
}

#[tokio::test]
async fn config_requires_authentication() {
    let app = TestApp::new().await;
    app.get("/api/webcam-table/config").await.assert_json(401);
}

async fn rooms(server: &Server) -> Value {
    let response = server.app.get("/api/webcam-table/rooms").await;
    let body = response.assert_json(200);
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    body
}

#[tokio::test]
async fn lists_live_rooms_with_their_seated_players_in_join_order() {
    let server = Server::start().await;
    let viewer = server.user().await;
    server.app.log_in(&viewer).await;
    assert_eq!(rooms(&server).await, json!({ "data": [] }));

    let room = room_id();
    let alice = server.join_player(&room, PEER_A, "Alice").await;
    let bob = server.join_player(&room, PEER_B, "Bob").await;
    let body = rooms(&server).await;
    let listed = body["data"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (&listed[0]["id"], &listed[0]["full"]),
        (&json!(room), &json!(false))
    );
    assert!(listed[0]["started_at"].is_i64());
    assert_eq!(
        listed[0]["players"],
        json!([{ "id": alice.player_id(), "name": "Alice" }, { "id": bob.player_id(), "name": "Bob" }])
    );
}

#[tokio::test]
async fn keeps_an_empty_room_listed_until_it_is_closed_as_idle() {
    let server = Server::start().await;
    let viewer = server.user().await;
    server.app.log_in(&viewer).await;
    let room = room_id();
    let alice = server.join_player(&room, PEER_A, "Alice").await;
    let player = alice.player_id();
    let mut lobby = server.state().pubsub.listen("webcam_tables");
    assert_eq!(rooms(&server).await["data"][0]["id"], room);

    alice.close().await;
    // The join diff may arrive after we subscribe, so skip diffs until the leave shows up.
    loop {
        let message = lobby.recv().await.unwrap();
        if message.event == "presence_diff" && message.payload["leaves"].get(PEER_A).is_some() {
            break;
        }
    }
    let body = rooms(&server).await;
    assert_eq!(
        body["data"],
        json!([{ "id": room, "started_at": body["data"][0]["started_at"], "full": false, "players": [] }])
    );

    server.wait_departed(&room, player).await;
    assert_eq!(
        server.state().webcam_tables.close_idle_rooms(0).await,
        std::slice::from_ref(&room)
    );
    assert_eq!(rooms(&server).await, json!({ "data": [] }));
}

#[tokio::test]
async fn rooms_require_a_signed_in_user() {
    let app = TestApp::new().await;
    app.get("/api/webcam-table/rooms").await.assert_json(401);
}

#[tokio::test]
async fn relay_only_sfu_servers_come_from_cloudflare_with_credentials() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/turn/keys/key123/credentials/generate-ice-servers"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "iceServers": [
                { "urls": ["stun:stun.cloudflare.com:3478"] },
                {
                    "urls": ["turn:turn.cloudflare.com:3478?transport=udp", "turn:turn.cloudflare.com:80?transport=tcp"],
                    "username": "u",
                    "credential": "c",
                },
            ]
        })))
        .mount(&mock)
        .await;
    let config = the_gathering::config::CloudflareTurnConfig {
        key_id: Some("key123".into()),
        api_token: Some("token-abc".into()),
        ttl_seconds: 60,
        api_base: mock.uri(),
    };
    let servers =
        the_gathering::cloudflare_turn::relay_servers(&reqwest::Client::new(), &config).await;
    assert_eq!(
        servers,
        [the_gathering_sfu::IceServer {
            urls: vec!["turn:turn.cloudflare.com:3478?transport=udp".into()],
            username: Some("u".into()),
            credential: Some("c".into()),
        }]
    );

    // Unavailable credentials leave the SFU without relays rather than failing.
    let down = the_gathering::config::CloudflareTurnConfig {
        api_base: "http://127.0.0.1:9".into(),
        ..config
    };
    assert!(
        the_gathering::cloudflare_turn::relay_servers(&reqwest::Client::new(), &down)
            .await
            .is_empty()
    );
}
