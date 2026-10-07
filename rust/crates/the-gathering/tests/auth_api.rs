//! Ported from `test/the_gathering_web/controllers/api/auth_controller_test.exs`.

mod support;

use serde_json::json;
use support::{PASSWORD, TestApp};
use the_gathering::crypto;
use the_gathering::db::UtcDateTime;

async fn create_user(app: &TestApp, username: &str, role: &str) -> the_gathering::accounts::User {
    let mut chars = username.chars();
    let display: String = chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
    app.state
        .accounts
        .create_user(&json!({"username": username, "display_name": display, "password": PASSWORD, "role": role}))
        .await
        .unwrap()
}

async fn log_in(app: &TestApp, username: &str) -> support::TestResponse {
    app.post("/api/session", json!({"username": username, "password": PASSWORD})).await
}

#[tokio::test]
async fn registration_signs_in_the_first_user_without_exposing_password_data() {
    let app = TestApp::new().await;
    let response = app
        .post(
            "/api/users",
            json!({"user": {"username": "Owner", "display_name": "Server Owner", "password": PASSWORD}}),
        )
        .await;
    let body = response.assert_json(201);
    assert_eq!(body["data"]["username"], "owner");
    assert_eq!(body["data"]["role"], "admin");
    let owner = app.state.accounts.get_user_by_username("owner").await.unwrap().unwrap();
    assert!(!response.text().contains("hashed_password"));
    assert!(!response.text().contains(owner.hashed_password.as_deref().unwrap()));
    let token = app.session().get_bytes("user_token").expect("token");
    assert!(app.session().get("user_id").is_none());
    assert_eq!(
        app.session().get_string("live_socket_id").unwrap(),
        format!("users_sessions:{}", crypto::url_encode64(&token))
    );
    assert!(app.state.accounts.get_user_by_session_token(&token).await.unwrap().is_some());
    assert!(response.header("x-csrf-token").is_some());

    let body = app.get("/api/session").await.assert_json(200);
    assert_eq!(body["data"]["display_name"], "Server Owner");
}

#[tokio::test]
async fn registration_is_forbidden_after_the_first_user_by_default() {
    let app = TestApp::new().await;
    create_user(&app, "owner", "admin").await;
    let response = app.post("/api/users", json!({"user": {"username": "member", "password": PASSWORD}})).await;
    assert_eq!(response.assert_json(403), json!({"errors": {"detail": "Forbidden"}}));
}

#[tokio::test]
async fn a_member_gets_403_on_admin_routes() {
    let app = TestApp::new().await;
    let member = create_user(&app, "member", "member").await;
    app.log_in(&member).await;
    assert_eq!(app.get("/api/admin/users").await.assert_json(403), json!({"errors": {"detail": "Forbidden"}}));
}

#[tokio::test]
async fn logout_clears_the_session() {
    let app = TestApp::new().await;
    create_user(&app, "owner", "admin").await;
    let body = log_in(&app, "OWNER").await.assert_json(200);
    assert_eq!(body["data"]["username"], "owner");
    let token = app.session().get_bytes("user_token").unwrap();

    let response = app.delete("/api/session").await;
    assert_eq!(response.status.as_u16(), 204);
    assert!(app.state.accounts.get_user_by_session_token(&token).await.unwrap().is_none());
    assert_eq!(app.get("/api/session").await.assert_json(401), json!({"errors": {"detail": "Unauthorized"}}));
}

#[tokio::test]
async fn session_renewal_revokes_only_the_superseded_current_device_token() {
    let app = TestApp::new().await;
    let user = create_user(&app, "owner", "admin").await;
    let other_device = app.state.accounts.generate_user_session_token(&user).await.unwrap();
    log_in(&app, "owner").await.assert_json(200);
    let old_token = app.session().get_bytes("user_token").unwrap();
    let eight_days_ago = UtcDateTime::now().add(time::Duration::days(-8));
    sqlx::query("UPDATE users_tokens SET inserted_at = ? WHERE token = ?")
        .bind(eight_days_ago)
        .bind(&old_token)
        .execute(app.pool())
        .await
        .unwrap();

    let body = app.get("/api/session").await.assert_json(200);
    assert_eq!(body["data"]["username"], "owner");
    let new_token = app.session().get_bytes("user_token").unwrap();
    assert_ne!(new_token, old_token);
    let accounts = &app.state.accounts;
    assert!(accounts.get_user_by_session_token(&old_token).await.unwrap().is_none());
    assert!(accounts.get_user_by_session_token(&new_token).await.unwrap().is_some());
    assert!(accounts.get_user_by_session_token(&other_device).await.unwrap().is_some());
}

#[tokio::test]
async fn updates_and_returns_the_signed_in_users_deck_sources() {
    let app = TestApp::new().await;
    let user = create_user(&app, "owner", "admin").await;
    app.log_in(&user).await;
    let body = app
        .patch(
            "/api/session/user",
            json!({"user": {
                "display_name": "Deck Brewer",
                "moxfield_username": " brewer ",
                "archidekt_username": "arch-brewer",
                "manavault_url": "https://vault.example.com/"
            }}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["display_name"], "Deck Brewer");
    assert_eq!(body["data"]["moxfield_username"], "brewer");
    assert_eq!(body["data"]["archidekt_username"], "arch-brewer");
    assert_eq!(body["data"]["manavault_url"], "https://vault.example.com");
}

#[tokio::test]
async fn stores_the_manavault_api_key_encrypted_and_never_returns_it() {
    let app = TestApp::new().await;
    let user = create_user(&app, "owner", "admin").await;
    app.log_in(&user).await;
    let response = app
        .patch("/api/session/user", json!({"user": {"display_name": "Owner", "manavault_api_key": " mv_secret_key "}}))
        .await;
    let body = response.assert_json(200);
    assert_eq!(body["data"]["has_manavault_api_key"], true);
    assert!(body["data"].get("manavault_api_key").is_none());
    assert!(!response.text().contains("mv_secret_key"));
    let stored: String = sqlx::query_scalar("select manavault_api_key from users where id = ?")
        .bind(user.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(!stored.contains("mv_secret_key"));
    assert_eq!(app.reload(&user).await.unwrap().manavault_api_key.as_deref(), Some("mv_secret_key"));

    // A blank key keeps the saved one; an explicit null removes it.
    let body = app
        .patch("/api/session/user", json!({"user": {"display_name": "Owner", "manavault_api_key": ""}}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["has_manavault_api_key"], true);
    let body = app
        .patch("/api/session/user", json!({"user": {"display_name": "Owner", "manavault_api_key": null}}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["has_manavault_api_key"], false);
    assert!(app.reload(&user).await.unwrap().manavault_api_key.is_none());
}

#[tokio::test]
async fn saves_the_palette_and_surface_style_on_the_account() {
    let app = TestApp::new().await;
    let user = create_user(&app, "owner", "admin").await;
    app.log_in(&user).await;
    let body = app.get("/api/session").await.assert_json(200);
    assert_eq!((body["data"]["palette"].as_str(), body["data"]["theme_style"].as_str()), (Some("claret"), Some("glass")));
    let body = app
        .patch("/api/session/appearance", json!({"user": {"palette": "gruvbox", "theme_style": "classic"}}))
        .await
        .assert_json(200);
    assert_eq!((body["data"]["palette"].as_str(), body["data"]["theme_style"].as_str()), (Some("gruvbox"), Some("classic")));
    let reloaded = app.reload(&user).await.unwrap();
    assert_eq!((reloaded.palette.as_str(), reloaded.theme_style.as_str()), ("gruvbox", "classic"));
    let body = app.patch("/api/session/appearance", json!({"user": {"palette": "nord"}})).await.assert_json(200);
    assert_eq!((body["data"]["palette"].as_str(), body["data"]["theme_style"].as_str()), (Some("nord"), Some("classic")));
}

#[tokio::test]
async fn rejects_unknown_appearance_values_and_anonymous_updates() {
    let app = TestApp::new().await;
    app.patch("/api/session/appearance", json!({"user": {"palette": "nord"}})).await.assert_json(401);
    let user = create_user(&app, "owner", "admin").await;
    app.log_in(&user).await;
    let body = app
        .patch("/api/session/appearance", json!({"user": {"palette": "vaporwave", "theme_style": "frosted"}}))
        .await
        .assert_json(422);
    assert_eq!(body, json!({"errors": {"palette": ["is invalid"], "theme_style": ["is invalid"]}}));
}

#[tokio::test]
async fn rejects_invalid_deck_source_values() {
    let app = TestApp::new().await;
    let user = create_user(&app, "owner", "admin").await;
    app.log_in(&user).await;
    let body = app
        .patch(
            "/api/session/user",
            json!({"user": {"display_name": "Owner", "moxfield_username": "https://moxfield.com/users/owner", "manavault_url": "not a URL"}}),
        )
        .await
        .assert_json(422);
    assert_eq!(
        body,
        json!({"errors": {
            "manavault_url": ["must be an allowed origin (scheme, host, and optional port only)"],
            "moxfield_username": ["must be a username, not a URL"]
        }})
    );
}

#[tokio::test]
async fn disabled_users_cannot_log_in() {
    let app = TestApp::new().await;
    create_user(&app, "owner", "admin").await;
    let member = create_user(&app, "member", "member").await;
    app.state.accounts.disable_user(&member).await.unwrap();
    let response = app.post("/api/session", json!({"username": "member", "password": PASSWORD})).await;
    assert_eq!(response.assert_json(401), json!({"errors": {"detail": "Unauthorized"}}));
}

#[tokio::test]
async fn password_change_invalidates_every_old_session_and_issues_a_new_one() {
    let app = TestApp::new().await;
    let user = create_user(&app, "owner", "admin").await;
    let other = app.state.accounts.generate_user_session_token(&user).await.unwrap();
    log_in(&app, "owner").await.assert_json(200);
    let old = app.session().get_bytes("user_token").unwrap();
    let body = app
        .patch(
            "/api/session/password",
            json!({"password": "a-brand-new-password", "password_confirmation": "a-brand-new-password"}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["username"], "owner");
    let accounts = &app.state.accounts;
    assert!(accounts.get_user_by_session_token(&old).await.unwrap().is_none());
    assert!(accounts.get_user_by_session_token(&other).await.unwrap().is_none());
    let new = app.session().get_bytes("user_token").unwrap();
    assert_ne!(new, old);
    assert!(accounts.get_user_by_session_token(&new).await.unwrap().is_some());
}

#[tokio::test]
async fn stale_authentication_requires_sudo_mode_and_password_reauthentication_restores_it() {
    let app = TestApp::new().await;
    create_user(&app, "owner", "admin").await;
    log_in(&app, "owner").await.assert_json(200);
    let token = app.session().get_bytes("user_token").unwrap();
    sqlx::query("UPDATE users_tokens SET authenticated_at = ? WHERE token = ?")
        .bind(UtcDateTime::now().add(time::Duration::minutes(-11)))
        .bind(&token)
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(
        app.get("/api/admin/users").await.assert_json(403),
        json!({"errors": {"code": "sudo_required", "detail": "Reauthentication required"}})
    );
    let body = app.post("/api/session/sudo", json!({"password": PASSWORD})).await.assert_json(200);
    assert_eq!(body["data"]["username"], "owner");
    let body = app.get("/api/admin/users").await.assert_json(200);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn mutating_requests_need_the_csrf_token() {
    let app = TestApp::new().await;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-csrf-token", "bogus".parse().unwrap());
    let response = app
        .request_with(axum::http::Method::POST, "/api/session", Some(json!({"username": "x", "password": "y"})), headers)
        .await;
    assert_eq!(response.assert_json(403), json!({"errors": {"detail": "Forbidden"}}));
}
