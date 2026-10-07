//! Self-update: version, channel, GitHub checks, and update requests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use serde_json::json;
use support::{TestApp, capture_logs};
use the_gathering::config::SelfUpdateConfig;
use the_gathering::self_update::{Channel, Method, RequestError};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NIGHTLY_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// An app whose `priv/VERSION` holds `version` and whose GitHub API is `github`.
async fn app(
    dir: &tempfile::TempDir,
    version: Option<&str>,
    github: &MockServer,
    self_update: SelfUpdateConfig,
) -> TestApp {
    if let Some(version) = version {
        std::fs::write(dir.path().join("VERSION"), format!("{version}\n")).unwrap();
    }
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

fn request_file(dir: &tempfile::TempDir, name: &str) -> SelfUpdateConfig {
    SelfUpdateConfig {
        request_file: Some(dir.path().join(name).display().to_string()),
        ..SelfUpdateConfig::default()
    }
}

fn watchtower(token: &str, url: Option<String>) -> SelfUpdateConfig {
    SelfUpdateConfig {
        watchtower_token: Some(token.into()),
        watchtower_url: url,
        ..SelfUpdateConfig::default()
    }
}

#[tokio::test]
async fn a_development_build_has_no_version_channel_or_updater_and_skips_the_check() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&github)
        .await;
    let app = app(&dir, None, &github, SelfUpdateConfig::default()).await;

    let status = app.state.self_update.status().await;
    assert_eq!(status.version, None);
    assert_eq!(status.channel, None);
    assert_eq!(status.method, None);
    assert!(!status.pending);
    assert_eq!(status.latest, None);
    assert_eq!(status.update_available, None);
    assert_eq!(status.check_error, None);
    github.verify().await;
}

#[tokio::test]
async fn compares_a_tagged_release_with_the_latest_github_release_by_version() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/cfbender/the-gathering/releases/latest"))
        .and(header("accept", "application/vnd.github+json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v0.10.0",
            "html_url": "https://github.com/cfbender/the-gathering/releases/tag/v0.10.0"
        })))
        .mount(&github)
        .await;
    let app = app(&dir, Some("v0.9.0"), &github, request_file(&dir, "request")).await;

    let status = app.state.self_update.status().await;
    assert_eq!(status.version.as_deref(), Some("v0.9.0"));
    assert_eq!(status.channel, Some(Channel::Release));
    assert_eq!(status.method, Some(Method::Systemd));
    assert_eq!(status.update_available, Some(true));
    let latest = status.latest.unwrap();
    assert_eq!(latest.version, "v0.10.0");
    assert_eq!(
        latest.url,
        "https://github.com/cfbender/the-gathering/releases/tag/v0.10.0"
    );
}

#[tokio::test]
async fn a_nightly_build_follows_the_nightly_tags_commit() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/cfbender/the-gathering/git/ref/tags/nightly"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"object": {"sha": NIGHTLY_SHA}})),
        )
        .mount(&github)
        .await;
    let app = app(
        &dir,
        Some("nightly-0123456"),
        &github,
        watchtower("secret", None),
    )
    .await;

    let status = app.state.self_update.status().await;
    assert_eq!(status.channel, Some(Channel::Nightly));
    assert_eq!(status.method, Some(Method::Watchtower));
    assert_eq!(status.update_available, Some(false));
    assert_eq!(status.latest.unwrap().version, "nightly-0123456");
}

/// A preview build (a branch pre-release installed with `update preview`) follows the preview
/// tag's commit, not nightly or the latest release.
#[tokio::test]
async fn a_preview_build_follows_the_preview_tags_commit() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/cfbender/the-gathering/git/ref/tags/preview"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"object": {"sha": "fedcba9876543210fedcba9876543210fedcba98"}}),
            ),
        )
        .expect(1)
        .mount(&github)
        .await;
    let first = app(
        &dir,
        Some("preview-0123456"),
        &github,
        watchtower("secret", None),
    )
    .await;

    let status = first.state.self_update.status().await;
    assert_eq!(status.channel, Some(Channel::Preview));
    assert_eq!(status.check_error, None);
    assert_eq!(status.update_available, Some(true));
    let latest = status.latest.unwrap();
    assert_eq!(latest.version, "preview-fedcba9");
    assert_eq!(
        latest.url,
        "https://github.com/cfbender/the-gathering/releases/tag/preview"
    );

    // Once the preview tag points at the running commit, it is up to date.
    let current = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/cfbender/the-gathering/git/ref/tags/preview"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"object": {"sha": "0123456789abcdef0123456789abcdef01234567"}}),
            ),
        )
        .mount(&github)
        .await;
    let second = app(
        &current,
        Some("preview-0123456"),
        &github,
        watchtower("secret", None),
    )
    .await;
    assert_eq!(
        second.state.self_update.status().await.update_available,
        Some(false)
    );
}

#[tokio::test]
async fn caches_the_github_answer_and_reports_failures_without_raising() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&github)
        .await;
    let app = app(&dir, Some("v0.9.0"), &github, SelfUpdateConfig::default()).await;

    let status = app.state.self_update.status().await;
    assert_eq!(status.latest, None);
    assert_eq!(status.update_available, None);
    let error = status.check_error.unwrap();
    assert!(error.contains("rate limit"), "{error}");
    let again = app.state.self_update.status().await;
    assert_eq!(again.check_error.as_deref(), Some(error.as_str()));
    github.verify().await;
}

#[tokio::test]
async fn other_github_failures_report_the_status() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(403))
        .mount(&github)
        .await;
    let app = app(&dir, Some("v0.9.0"), &github, SelfUpdateConfig::default()).await;
    assert_eq!(
        app.state.self_update.status().await.check_error.as_deref(),
        Some("GitHub answered with status 403.")
    );
}

#[tokio::test]
async fn update_available_orders_versions_and_accepts_nightly_commit_lengths() {
    use the_gathering::self_update::update_available;
    assert!(update_available(Channel::Release, "v0.9.0", "v0.10.0"));
    assert!(!update_available(Channel::Release, "v0.10.0", "v0.9.0"));
    assert!(!update_available(Channel::Release, "v1.2.3", "v1.2.3"));
    assert!(!update_available(
        Channel::Nightly,
        "nightly-0123456",
        "nightly-0123456789ab"
    ));
    assert!(update_available(
        Channel::Nightly,
        "nightly-0123456",
        "nightly-fedcba9"
    ));
}

#[tokio::test]
async fn without_an_updater_it_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    let app = app(&dir, Some("v0.9.0"), &github, SelfUpdateConfig::default()).await;
    assert_eq!(
        app.state.self_update.request_update().await.unwrap_err(),
        RequestError::Unsupported
    );
}

#[tokio::test]
async fn with_systemd_it_creates_the_request_file_and_reports_the_update_as_pending() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    let app = app(
        &dir,
        Some("v0.9.0"),
        &github,
        request_file(&dir, "update-request"),
    )
    .await;
    let file = dir.path().join("update-request");

    let status = app.state.self_update.request_update().await.unwrap();
    assert!(status.pending);
    assert!(status.requested_at.is_some());
    assert!(file.exists());
    assert!(app.state.self_update.status().await.pending);

    // systemd removes the file when `update` finishes. Even without the restart a successful
    // update brings, that ends the pending state, so a failed run does not lock the button.
    std::fs::remove_file(&file).unwrap();
    let status = app.state.self_update.status().await;
    assert!(!status.pending);
    assert!(status.requested_at.is_some());
}

#[tokio::test]
async fn with_systemd_an_unwritable_request_file_is_reported_not_raised() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    let app = app(
        &dir,
        Some("v0.9.0"),
        &github,
        request_file(&dir, "missing/dir/update-request"),
    )
    .await;
    let (guard, logs) = capture_logs();
    assert_eq!(
        app.state.self_update.request_update().await.unwrap_err(),
        RequestError::UpdaterUnavailable
    );
    drop(guard);
    assert!(
        logs.contents()
            .contains("Could not write the update request file")
    );
}

#[tokio::test]
async fn with_watchtower_it_posts_an_async_update_for_this_image_only() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    let updater = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/update"))
        .and(query_param("image", "ghcr.io/cfbender/the-gathering"))
        .and(query_param("async", "true"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({})))
        .expect(1)
        .mount(&updater)
        .await;
    let app = app(
        &dir,
        Some("nightly-0123456"),
        &github,
        watchtower("secret", Some(updater.uri())),
    )
    .await;

    assert!(
        app.state
            .self_update
            .request_update()
            .await
            .unwrap()
            .pending
    );
    updater.verify().await;
}

#[tokio::test]
async fn watchtower_already_updating_is_distinguished_from_watchtower_being_down() {
    let dir = tempfile::tempdir().unwrap();
    let github = MockServer::start().await;
    // A server of its own (not pooled), so dropping it closes the port.
    let updater = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"error": "another update"})))
        .mount(&updater)
        .await;
    let app = app(
        &dir,
        Some("v0.9.0"),
        &github,
        watchtower("secret", Some(updater.uri())),
    )
    .await;

    assert_eq!(
        app.state.self_update.request_update().await.unwrap_err(),
        RequestError::UpdateInProgress
    );

    drop(updater);
    let (guard, logs) = capture_logs();
    assert_eq!(
        app.state.self_update.request_update().await.unwrap_err(),
        RequestError::UpdaterUnavailable
    );
    drop(guard);
    assert!(logs.contents().contains("could not be reached"));
    assert!(!app.state.self_update.status().await.pending);
}
