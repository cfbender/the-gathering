//! The registration invitation API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use axum::http::Method;
use serde_json::{Value, json};
use support::{PASSWORD, TestApp, capture_logs};
use the_gathering::accounts::registration_invite_hash;
use the_gathering::crypto;

const ADMIN_PATH: &str = "/api/admin/registration-invite";

#[tokio::test]
async fn creation_and_rotation_require_an_administrator_with_recent_authentication() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;

    for method in [Method::GET, Method::POST] {
        app.clear_cookies();
        app.request(method.clone(), ADMIN_PATH, None)
            .await
            .assert_json(401);
        app.log_in(&member).await;
        app.request(method.clone(), ADMIN_PATH, None)
            .await
            .assert_json(403);
        app.log_in(&admin).await;
        app.expire_sudo(11 * 60).await;
        let body = app
            .request(method.clone(), ADMIN_PATH, None)
            .await
            .assert_json(403);
        assert_eq!(body["errors"]["code"], "sudo_required");
    }

    let accounts = &app.state.accounts;
    assert!(
        accounts
            .get_settings()
            .await
            .unwrap()
            .registration_invite_hash
            .is_none()
    );
    app.log_in(&admin).await;
    let response = app.request(Method::POST, ADMIN_PATH, None).await;
    let first = response.assert_json(200)["data"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.header("cache-control"), Some("no-store"));
    assert_eq!(first.len(), 43);
    assert_eq!(
        accounts
            .get_settings()
            .await
            .unwrap()
            .registration_invite_hash,
        Some(crypto::sha256(first.as_bytes()))
    );

    let second = app
        .request(Method::POST, ADMIN_PATH, None)
        .await
        .assert_json(200)["data"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(first, second);
    assert!(
        !accounts
            .valid_registration_invite_hash(registration_invite_hash(&first).as_deref())
            .await
            .unwrap()
    );
    assert!(
        accounts
            .valid_registration_invite_hash(registration_invite_hash(&second).as_deref())
            .await
            .unwrap()
    );
    assert_eq!(
        app.get(ADMIN_PATH).await.assert_json(200),
        json!({"data": {"enabled": true}})
    );
}

#[tokio::test]
async fn public_settings_and_logs_do_not_reveal_secrets_and_invalid_tokens_clear_pending_invitations()
 {
    let app = TestApp::new().await;
    app.unique_admin().await;
    let accounts = &app.state.accounts;
    let token = accounts.rotate_registration_invite().await.unwrap();
    let hash = registration_invite_hash(&token).unwrap();

    let body = app.get("/api/registration").await.assert_json(200);
    assert_eq!(body["data"]["allowed"], false);
    assert_eq!(body["data"]["bootstrap"], false);
    assert_eq!(body["data"]["discord_configured"], true);

    let (guard, logs) = capture_logs();
    let response = app
        .post("/api/registration-invite", json!({"token": token}))
        .await;
    assert_eq!(response.assert_json(200), json!({"data": {"valid": true}}));
    assert_eq!(app.session().registration_invite_hash, Some(hash.clone()));
    assert_eq!(
        app.get("/api/registration-invite").await.assert_json(200),
        json!({"data": {"valid": true}})
    );

    let hex: String = hash.iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    for value in [Value::Null, json!({"token": token})] {
        app.post("/api/registration-invite", json!({"token": value}))
            .await
            .assert_json(400);
    }
    let invalid = [
        json!(""),
        json!("invalid"),
        json!("x".repeat(43)),
        json!(hex),
    ];
    for value in invalid {
        let rejected = app
            .post("/api/registration-invite", json!({"token": value}))
            .await;
        assert_eq!(
            rejected.assert_json(200),
            json!({"data": {"valid": false}}),
            "{value}"
        );
        assert!(app.session().registration_invite_hash.is_none());
    }

    accounts.rotate_registration_invite().await.unwrap();
    assert_eq!(
        app.get("/api/registration-invite").await.assert_json(200),
        json!({"data": {"valid": false}})
    );
    app.clear_cookies();
    assert_eq!(
        app.post("/api/registration-invite", json!({"token": token}))
            .await
            .assert_json(200),
        json!({"data": {"valid": false}})
    );
    drop(guard);

    let logs = logs.contents();
    assert!(!logs.contains(&token), "{logs}");
    assert!(!logs.contains(&hex));
    assert!(!logs.contains(&format!("{hash:?}")));

    let settings = accounts.get_settings().await.unwrap();
    let stored = settings.registration_invite_hash.clone().unwrap();
    assert!(!format!("{settings:?}").contains(&format!("{stored:?}")));
}

#[tokio::test]
async fn a_missing_token_is_a_bad_request() {
    let app = TestApp::new().await;
    app.post("/api/registration-invite", json!({}))
        .await
        .assert_json(400);
}

#[tokio::test]
async fn invite_fields_cannot_be_assigned_through_settings_or_password_registration() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let accounts = &app.state.accounts;
    let token = accounts.rotate_registration_invite().await.unwrap();
    let hash = accounts
        .get_settings()
        .await
        .unwrap()
        .registration_invite_hash;

    app.log_in(&admin).await;
    assert_eq!(
        app.patch(
            "/api/admin/settings",
            json!({"registration_invite_hash": "attacker"})
        )
        .await
        .assert_json(200),
        json!({"data": {"registration_enabled": false, "detailed_stats_from": null}})
    );
    assert_eq!(
        accounts
            .get_settings()
            .await
            .unwrap()
            .registration_invite_hash,
        hash
    );

    app.clear_cookies();
    app.post("/api/registration-invite", json!({"token": token}))
        .await
        .assert_json(200);
    app.post(
        "/api/users",
        json!({"username": "newcomer", "display_name": "Newcomer", "password": PASSWORD}),
    )
    .await
    .assert_json(403);
}
