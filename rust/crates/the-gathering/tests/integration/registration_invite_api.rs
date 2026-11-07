//! The registration invitation API: per-playgroup invite links.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use axum::http::Method;
use serde_json::{Value, json};
use support::{PASSWORD, PLAYGROUP, TestApp, capture_logs};
use the_gathering::accounts::registration_invite_hash;
use the_gathering::crypto;

fn admin_path(playgroup: i64) -> String {
    format!("/api/admin/playgroups/{playgroup}/registration-invite")
}

async fn stored_hash(app: &TestApp, playgroup: i64) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT registration_invite_hash FROM playgroups WHERE id = ?")
        .bind(playgroup)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn creation_and_rotation_require_an_administrator_with_recent_authentication() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    let path = admin_path(PLAYGROUP);

    for method in [Method::GET, Method::POST] {
        app.clear_cookies();
        app.request(method.clone(), &path, None)
            .await
            .assert_json(401);
        app.log_in(&member).await;
        app.request(method.clone(), &path, None)
            .await
            .assert_json(403);
        app.log_in(&admin).await;
        app.expire_sudo(11 * 60).await;
        let body = app
            .request(method.clone(), &path, None)
            .await
            .assert_json(403);
        assert_eq!(body["errors"]["code"], "sudo_required");
    }

    assert!(stored_hash(&app, PLAYGROUP).await.is_none());
    app.log_in(&admin).await;
    assert_eq!(
        app.get(&path).await.assert_json(200),
        json!({"data": {"enabled": false}})
    );
    let response = app.request(Method::POST, &path, None).await;
    let first = response.assert_json(200)["data"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.header("cache-control"), Some("no-store"));
    assert_eq!(first.len(), 43);
    assert_eq!(
        stored_hash(&app, PLAYGROUP).await,
        Some(crypto::sha256(first.as_bytes()))
    );

    let second = app
        .request(Method::POST, &path, None)
        .await
        .assert_json(200)["data"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(first, second);
    let playgroups = &app.state.playgroups;
    assert!(
        playgroups
            .for_invite_hash(registration_invite_hash(&first).as_deref())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        playgroups
            .for_invite_hash(registration_invite_hash(&second).as_deref())
            .await
            .unwrap()
            .map(|playgroup| playgroup.id),
        Some(PLAYGROUP)
    );
    assert_eq!(
        app.get(&path).await.assert_json(200),
        json!({"data": {"enabled": true}})
    );
    app.get(&admin_path(999)).await.assert_json(404);
    app.request(Method::POST, &admin_path(999), None)
        .await
        .assert_json(404);
}

#[tokio::test]
async fn each_playgroup_has_its_own_invitation() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let crew = app.playgroup("Thursday crew", None, &admin).await;
    let main_token = app.rotate_invite(PLAYGROUP).await;
    let crew_token = app.rotate_invite(crew).await;
    let playgroups = &app.state.playgroups;
    assert_eq!(
        playgroups
            .for_invite_hash(registration_invite_hash(&main_token).as_deref())
            .await
            .unwrap()
            .map(|playgroup| playgroup.id),
        Some(PLAYGROUP)
    );
    assert_eq!(
        playgroups
            .for_invite_hash(registration_invite_hash(&crew_token).as_deref())
            .await
            .unwrap()
            .map(|playgroup| playgroup.id),
        Some(crew)
    );

    // Rotating one playgroup's invitation leaves the other's working.
    app.rotate_invite(crew).await;
    assert_eq!(
        app.post("/api/registration-invite", json!({"token": crew_token}))
            .await
            .assert_json(200)["data"]["valid"],
        false
    );
    assert_eq!(
        app.post("/api/registration-invite", json!({"token": main_token}))
            .await
            .assert_json(200)["data"],
        json!({"valid": true, "playgroup": {"id": PLAYGROUP, "name": "Main"}, "joined": false})
    );
}

#[tokio::test]
async fn public_settings_and_logs_do_not_reveal_secrets_and_invalid_tokens_clear_pending_invitations()
 {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let token = app.rotate_invite(PLAYGROUP).await;
    let hash = registration_invite_hash(&token).unwrap();

    let body = app.get("/api/registration").await.assert_json(200);
    assert_eq!(body["data"]["allowed"], false);
    assert_eq!(body["data"]["bootstrap"], false);
    assert_eq!(body["data"]["discord_configured"], true);

    let (guard, logs) = capture_logs();
    let response = app
        .post("/api/registration-invite", json!({"token": token}))
        .await;
    assert_eq!(response.header("cache-control"), Some("no-store"));
    let accepted = json!({"data": {"valid": true, "playgroup": {"id": PLAYGROUP, "name": "Main"}, "joined": false}});
    assert_eq!(response.assert_json(200), accepted);
    assert_eq!(app.session().registration_invite_hash, Some(hash.clone()));
    assert_eq!(
        app.get("/api/registration-invite").await.assert_json(200),
        accepted
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
    let rejected_body = json!({"data": {"valid": false, "playgroup": null, "joined": false}});
    for value in invalid {
        let rejected = app
            .post("/api/registration-invite", json!({"token": value}))
            .await;
        assert_eq!(rejected.assert_json(200), rejected_body, "{value}");
        assert!(app.session().registration_invite_hash.is_none());
    }

    app.rotate_invite(PLAYGROUP).await;
    assert_eq!(
        app.get("/api/registration-invite").await.assert_json(200),
        rejected_body
    );
    app.clear_cookies();
    assert_eq!(
        app.post("/api/registration-invite", json!({"token": token}))
            .await
            .assert_json(200),
        rejected_body
    );
    drop(guard);

    let logs = logs.contents();
    assert!(!logs.contains(&token), "{logs}");
    assert!(!logs.contains(&hex));
    assert!(!logs.contains(&format!("{hash:?}")));

    let stored = stored_hash(&app, PLAYGROUP).await.unwrap();
    let playgroup = app.state.playgroups.get(PLAYGROUP).await.unwrap().unwrap();
    assert!(playgroup.invite_enabled);
    assert!(!format!("{playgroup:?}").contains(&format!("{stored:?}")));
    app.log_in(&admin).await;
    let text = app
        .get("/api/admin/playgroups")
        .await
        .assert_json(200)
        .to_string();
    assert!(!text.contains(&hex));
    assert!(!text.contains("registration_invite_hash"));
    assert!(text.contains("\"invite_enabled\":true"));
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
    let token = app.rotate_invite(PLAYGROUP).await;
    let hash = stored_hash(&app, PLAYGROUP).await;

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
        app.patch(
            &format!("/api/admin/playgroups/{PLAYGROUP}"),
            json!({"registration_invite_hash": "attacker", "invite_enabled": false})
        )
        .await
        .assert_json(200)["data"]["invite_enabled"],
        true
    );
    assert_eq!(stored_hash(&app, PLAYGROUP).await, hash);

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
