//! The personal API keys API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use serde_json::json;
use support::TestApp;
use the_gathering::crypto;

/// `setup :register_and_log_in_user`.
async fn signed_in() -> (TestApp, the_gathering::accounts::User) {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    app.log_in(&user).await;
    (app, user)
}

#[tokio::test]
async fn creates_a_key_reveals_the_secret_once_and_stores_only_its_digest() {
    let (app, user) = signed_in().await;
    let response = app
        .post("/api/session/api-keys", json!({"name": "  Stats script  "}))
        .await;
    let created = response.assert_json(201)["data"].clone();
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    assert_eq!(created["name"], "Stats script");
    let token = created["token"].as_str().unwrap();
    let prefix = created["prefix"].as_str().unwrap();
    assert!(token.starts_with("tg_"));
    assert!(token.starts_with(prefix));

    let (owner, digest): (i64, Vec<u8>) =
        sqlx::query_as("SELECT user_id, token_hash FROM api_keys WHERE id = ?")
            .bind(created["id"].as_i64().unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(owner, user.id);
    assert_eq!(digest, crypto::sha256(token.as_bytes()));

    let listed = app.get("/api/session/api-keys").await.assert_json(200)["data"].clone();
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], created["id"]);
    assert_eq!(listed[0]["name"], "Stats script");
    assert_eq!(listed[0]["last_used_at"], json!(null));
    assert!(listed[0].get("token").is_none());
}

#[tokio::test]
async fn requires_a_name() {
    let (app, _user) = signed_in().await;
    let body = app
        .post("/api/session/api-keys", json!({"name": " "}))
        .await
        .assert_json(422);
    assert!(!body["errors"]["name"].as_array().unwrap().is_empty());
    let body = app
        .post("/api/session/api-keys", json!({}))
        .await
        .assert_json(422);
    assert_eq!(body["errors"]["name"], json!(["can't be blank"]));
    app.post("/api/session/api-keys", json!({"name": 5}))
        .await
        .assert_json(400);
}

#[tokio::test]
async fn lists_and_revokes_only_the_signed_in_users_keys() {
    let (app, user) = signed_in().await;
    let other = app.unique_member().await;
    let accounts = &app.state.accounts;
    let (_token, own) = accounts
        .create_api_key(user.id, &support::input(json!({"name": "mine"})))
        .await
        .unwrap();
    let (other_token, other_key) = accounts
        .create_api_key(other.id, &support::input(json!({"name": "theirs"})))
        .await
        .unwrap();

    let listed = app.get("/api/session/api-keys").await.assert_json(200);
    assert_eq!(listed["data"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"][0]["name"], "mine");

    app.delete(&format!("/api/session/api-keys/{}", other_key.id))
        .await
        .assert_json(404);
    assert!(
        accounts
            .authenticate_api_key(&other_token)
            .await
            .unwrap()
            .is_some()
    );

    let response = app
        .delete(&format!("/api/session/api-keys/{}", own.id))
        .await;
    assert_eq!(response.status.as_u16(), 204);
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM api_keys WHERE id = ?")
        .bind(own.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    app.delete("/api/session/api-keys/not-an-id")
        .await
        .assert_json(400);
}

#[tokio::test]
async fn requires_a_signed_in_session() {
    let app = TestApp::new().await;
    app.get("/api/session/api-keys").await.assert_json(401);
}
