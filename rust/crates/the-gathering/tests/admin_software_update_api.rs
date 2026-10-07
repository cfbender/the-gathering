//! The admin software update API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::Method;
use serde_json::{Value, json};
use support::{TestApp, capture_logs};
use the_gathering::config::SelfUpdateConfig;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PATH: &str = "/api/admin/software-update";

async fn github() -> MockServer {
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/cfbender/the-gathering/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v9.9.9",
            "html_url": "https://github.com/cfbender/the-gathering/releases/tag/v9.9.9"
        })))
        .mount(&github)
        .await;
    github
}

async fn app(
    dir: &tempfile::TempDir,
    github: &MockServer,
    self_update: SelfUpdateConfig,
) -> TestApp {
    std::fs::write(dir.path().join("VERSION"), "v0.1.0\n").unwrap();
    let priv_dir = dir.path().to_path_buf();
    let github_api = format!("{}/repos/cfbender/the-gathering", github.uri());
    TestApp::with_config(move |config| {
        config.priv_dir = priv_dir;
        config.self_update = SelfUpdateConfig {
            github_api,
            ..self_update
        };
    })
    .await
}

fn systemd(dir: &tempfile::TempDir) -> SelfUpdateConfig {
    SelfUpdateConfig {
        request_file: Some(dir.path().join("update-request").display().to_string()),
        ..SelfUpdateConfig::default()
    }
}

async fn request(app: &TestApp, method: &Method) -> support::TestResponse {
    app.request(method.clone(), PATH, None).await
}

#[tokio::test]
async fn status_and_requests_need_an_administrator_with_recent_authentication() {
    let dir = tempfile::tempdir().unwrap();
    let github = github().await;
    let app = app(&dir, &github, systemd(&dir)).await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;

    for method in [Method::GET, Method::POST] {
        app.clear_cookies();
        request(&app, &method).await.assert_json(401);
        app.log_in(&member).await;
        request(&app, &method).await.assert_json(403);
        app.log_in(&admin).await;
        app.expire_sudo(11 * 60).await;
        let body = request(&app, &method).await.assert_json(403);
        assert_eq!(body["errors"]["code"], "sudo_required");
    }

    assert!(!dir.path().join("update-request").exists());
}

#[tokio::test]
async fn reports_the_version_and_hands_the_update_to_systemd() {
    let dir = tempfile::tempdir().unwrap();
    let github = github().await;
    let app = app(&dir, &github, systemd(&dir)).await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;

    let response = app.get(PATH).await;
    assert_eq!(
        response.assert_json(200),
        json!({"data": {
            "version": "v0.1.0",
            "channel": "release",
            "method": "systemd",
            "pending": false,
            "requested_at": null,
            "update_available": true,
            "check_error": null,
            "latest": {
                "version": "v9.9.9",
                "url": "https://github.com/cfbender/the-gathering/releases/tag/v9.9.9"
            }
        }})
    );
    assert_eq!(response.header("cache-control"), Some("no-store"));

    let response = app.request(Method::POST, PATH, None).await;
    let body = response.assert_json(202);
    assert_eq!(response.header("cache-control"), Some("no-store"));
    assert_eq!(body["data"]["pending"], true);
    let requested_at = body["data"]["requested_at"].as_str().unwrap();
    let parsed =
        time::OffsetDateTime::parse(requested_at, &time::format_description::well_known::Rfc3339)
            .unwrap();
    assert_eq!(parsed.offset(), time::UtcOffset::UTC);
    assert!(dir.path().join("update-request").exists());
}

#[tokio::test]
async fn refuses_when_no_updater_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    let github = github().await;
    let app = app(&dir, &github, SelfUpdateConfig::default()).await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;

    assert_eq!(
        app.get(PATH).await.assert_json(200)["data"]["method"],
        Value::Null
    );
    assert_eq!(
        app.request(Method::POST, PATH, None).await.assert_json(400),
        json!({"errors": {"detail": "Bad Request"}})
    );
}

#[tokio::test]
async fn maps_watchtower_outcomes_to_409_and_502() {
    let dir = tempfile::tempdir().unwrap();
    let github = github().await;
    let updater = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/update"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"error": "another update"})))
        .mount(&updater)
        .await;
    let app = app(
        &dir,
        &github,
        SelfUpdateConfig {
            watchtower_token: Some("secret".into()),
            watchtower_url: Some(updater.uri()),
            ..SelfUpdateConfig::default()
        },
    )
    .await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;

    app.request(Method::POST, PATH, None).await.assert_json(409);

    drop(updater);
    let (_guard, _logs) = capture_logs();
    app.request(Method::POST, PATH, None).await.assert_json(502);
}
