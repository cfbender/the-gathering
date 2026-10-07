//! Discord OAuth sign-in, registration invitations, and Discord sudo.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use std::collections::HashMap;

use serde_json::json;
use support::{PASSWORD, TestApp, capture_logs};
use the_gathering::accounts::User;
use the_gathering::accounts::discord::DiscordClaims;
use the_gathering::accounts::registration_invite_hash;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Discord's token and profile endpoints: the code `discord-code:<id>` becomes the access
/// token `<id>`, and the profile of that token has that id.
struct Discord;

impl Respond for Discord {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        match (request.method.as_str(), request.url.path()) {
            ("POST", "/api/oauth2/token") => {
                let form: HashMap<String, String> = url::form_urlencoded::parse(&request.body)
                    .into_owned()
                    .collect();
                let id = form["code"].strip_prefix("discord-code:").unwrap();
                assert_eq!(form["client_secret"], "discord-client-secret");
                ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": id,
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "scope": "identify email"
                }))
            }
            ("GET", "/api/users/@me") => {
                let header = request.headers["authorization"].to_str().unwrap();
                let id = header.strip_prefix("Bearer ").unwrap();
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": id,
                    "username": "Discord_User",
                    "avatar": "avatar-hash",
                    "email": "member@example.com",
                    "verified": true
                }))
            }
            _ => ResponseTemplate::new(404),
        }
    }
}

async fn app_with(responder: impl Respond + 'static) -> (TestApp, MockServer) {
    let discord = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(responder)
        .mount(&discord)
        .await;
    let api_base = format!("{}/api", discord.uri());
    let app = TestApp::with_config(move |config| {
        if let Some(oauth) = &mut config.discord_oauth {
            oauth.api_base = api_base;
        }
    })
    .await;
    (app, discord)
}

async fn app() -> (TestApp, MockServer) {
    app_with(Discord).await
}

fn query_param(location: &str, name: &str) -> Option<String> {
    let url = url::Url::parse(&format!("http://localhost{location}"))
        .or_else(|_| url::Url::parse(location))
        .unwrap();
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// `GET /auth/discord`, returning the OAuth `state`.
async fn start(app: &TestApp, params: &[(&str, &str)]) -> String {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();
    let location = app
        .get(&format!("/auth/discord?{query}"))
        .await
        .redirected_to();
    query_param(&location, "state").unwrap()
}

async fn finish(app: &TestApp, discord_id: &str, state: &str) -> support::TestResponse {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("code", &format!("discord-code:{discord_id}"))
        .append_pair("state", state)
        .finish();
    app.get(&format!("/auth/discord/callback?{query}")).await
}

async fn callback(app: &TestApp, discord_id: &str) -> support::TestResponse {
    callback_to(app, discord_id, "/", &[]).await
}

async fn callback_to(
    app: &TestApp,
    discord_id: &str,
    return_to: &str,
    params: &[(&str, &str)],
) -> support::TestResponse {
    let mut params = params.to_vec();
    params.push(("returnTo", return_to));
    let state = start(app, &params).await;
    finish(app, discord_id, &state).await
}

async fn accept_invite(app: &TestApp, token: &str) {
    assert_eq!(
        app.post("/api/registration-invite", json!({"token": token}))
            .await
            .assert_json(200),
        json!({"data": {"valid": true}})
    );
}

async fn create_admin(app: &TestApp) -> User {
    app.admin("owner").await
}

async fn open_registration(app: &TestApp) {
    app.settings(json!({"registration_enabled": true})).await;
}

async fn close_registration(app: &TestApp) {
    app.settings(json!({"registration_enabled": false})).await;
}

async fn create_discord_user(app: &TestApp, discord_id: &str) -> User {
    app.state
        .accounts
        .sign_in_with_discord(
            &DiscordClaims {
                sub: discord_id.into(),
                preferred_username: Some("Discord_User".into()),
                picture: Some(format!(
                    "https://cdn.discordapp.com/avatars/{discord_id}/avatar-hash"
                )),
            },
            None,
        )
        .await
        .unwrap()
}

async fn signed_in_user(app: &TestApp) -> Option<User> {
    let token = app.session().get_bytes("user_token")?;
    app.state
        .accounts
        .get_user_by_session_token(&token)
        .await
        .unwrap()
        .map(|(user, _)| user)
}

async fn discord_user(app: &TestApp, discord_id: &str) -> Option<User> {
    app.state
        .accounts
        .get_user_by_discord_id(discord_id)
        .await
        .unwrap()
}

/// `(user_id, name)` of the player linked to `discord_id`.
async fn discord_player(app: &TestApp, discord_id: &str) -> (Option<i64>, String) {
    sqlx::query_as("SELECT user_id, name FROM players WHERE discord_id = ?")
        .bind(discord_id)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn callback_creates_a_passwordless_member_and_linked_player_when_registration_is_open() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;

    let response = callback_to(&app, "100000000000000001", "/games", &[]).await;
    assert_eq!(response.redirected_to(), "/games");

    let user = signed_in_user(&app).await.unwrap();
    assert_eq!(user.discord_id.as_deref(), Some("100000000000000001"));
    assert_eq!(user.username, "discord_user");
    assert_eq!(user.role, "member");
    assert!(user.hashed_password.is_none());
    assert_eq!(
        user.avatar_url.as_deref(),
        Some("https://cdn.discordapp.com/avatars/100000000000000001/avatar-hash")
    );
    assert_eq!(
        discord_player(&app, "100000000000000001").await,
        (Some(user.id), "Discord_User".to_owned())
    );
}

#[tokio::test]
async fn callback_rejects_an_unknown_discord_account_when_registration_is_closed() {
    let (app, _discord) = app().await;
    create_admin(&app).await;

    let response = callback(&app, "100000000000000002").await;
    assert_eq!(response.redirected_to(), "/login?error=registration_closed");
    assert!(discord_user(&app, "100000000000000002").await.is_none());
    assert!(app.session().get("user_token").is_none());
}

#[tokio::test]
async fn one_invitation_admits_multiple_passwordless_members_without_opening_registration() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    let token = app
        .state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();

    for id in ["200000000000000001", "200000000000000002"] {
        app.clear_cookies();
        accept_invite(&app, &token).await;
        assert_eq!(callback(&app, id).await.redirected_to(), "/");
        let user = signed_in_user(&app).await.unwrap();
        assert_eq!(user.role, "member");
        assert!(user.hashed_password.is_none());
        assert_eq!(discord_player(&app, id).await.0, Some(user.id));
        assert!(app.session().get("discord_oauth").is_none());
        assert!(app.session().get("registration_invite_hash").is_none());
    }

    let status = app.state.accounts.registration_status().await.unwrap();
    assert!(!status.allowed && !status.bootstrap);
}

#[tokio::test]
async fn an_invitation_survives_restarting_oauth_before_completing_registration() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    let token = app
        .state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    accept_invite(&app, &token).await;
    start(&app, &[]).await;

    assert_eq!(
        callback(&app, "200000000000000010").await.redirected_to(),
        "/"
    );
    assert!(discord_user(&app, "200000000000000010").await.is_some());
    assert!(app.session().get("registration_invite_hash").is_none());
    assert!(app.session().get("discord_oauth").is_none());
}

#[tokio::test]
async fn an_invitation_survives_a_canceled_oauth_attempt_and_can_be_retried() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    let token = app
        .state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    accept_invite(&app, &token).await;
    let state = start(&app, &[]).await;

    let (_guard, _logs) = capture_logs();
    let response = app
        .get(&format!(
            "/auth/discord/callback?error=access_denied&state={state}"
        ))
        .await;
    assert_eq!(response.redirected_to(), "/login?error=discord_failed");
    assert!(app.session().get("discord_oauth").is_none());
    assert!(app.session().get("user_token").is_none());

    assert_eq!(
        callback(&app, "200000000000000011").await.redirected_to(),
        "/"
    );
    assert!(discord_user(&app, "200000000000000011").await.is_some());
    assert!(app.session().get("registration_invite_hash").is_none());
}

#[tokio::test]
async fn invalid_invitations_cannot_register_a_new_discord_member() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    app.state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    assert_eq!(
        app.post("/api/registration-invite", json!({"token": "x".repeat(43)}))
            .await
            .assert_json(200),
        json!({"data": {"valid": false}})
    );
    assert_eq!(
        callback(&app, "200000000000000009").await.redirected_to(),
        "/login?error=registration_closed"
    );
    assert!(discord_user(&app, "200000000000000009").await.is_none());
    assert!(app.session().get("user_token").is_none());
}

#[tokio::test]
async fn rotation_revokes_an_invitation_after_oauth_starts_not_just_at_landing() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    let accounts = &app.state.accounts;
    let token = accounts.rotate_registration_invite().await.unwrap();
    accept_invite(&app, &token).await;
    let location = app.get("/auth/discord").await.redirected_to();
    assert!(!location.contains(&token));
    let state = query_param(&location, "state").unwrap();
    assert_eq!(
        app.session().get_bytes("registration_invite_hash"),
        registration_invite_hash(&token)
    );
    let Some(eetf::Term::Map(attempt)) = app.session().get("discord_oauth") else {
        panic!("no OAuth attempt in the session");
    };
    assert!(matches!(
        attempt.map.get(&eetf::Term::Atom(eetf::Atom::from(
            "registration_invite_hash"
        ))),
        Some(eetf::Term::Binary(_))
    ));

    let new_token = accounts.rotate_registration_invite().await.unwrap();
    let response = finish(&app, "200000000000000003", &state).await;
    assert_eq!(response.redirected_to(), "/login?error=registration_closed");
    assert!(discord_user(&app, "200000000000000003").await.is_none());
    assert!(app.session().get("discord_oauth").is_none());
    assert!(app.session().get("user_token").is_none());

    assert_eq!(
        callback(&app, "200000000000000003").await.redirected_to(),
        "/login?error=registration_closed"
    );
    assert!(discord_user(&app, "200000000000000003").await.is_none());
    assert!(app.session().get("user_token").is_none());

    app.clear_cookies();
    accept_invite(&app, &new_token).await;
    assert_eq!(
        callback(&app, "200000000000000003").await.redirected_to(),
        "/"
    );
}

#[tokio::test]
async fn rotation_before_oauth_starts_revokes_the_accepted_invitation() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    let token = app
        .state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    accept_invite(&app, &token).await;
    app.state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    assert_eq!(
        callback(&app, "200000000000000004").await.redirected_to(),
        "/login?error=registration_closed"
    );
    assert!(discord_user(&app, "200000000000000004").await.is_none());
}

#[tokio::test]
async fn an_invitation_cannot_bypass_oauth_state_verification_or_be_supplied_at_callback() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    let token = app
        .state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    accept_invite(&app, &token).await;
    start(&app, &[]).await;
    let (guard, _logs) = capture_logs();
    let response = finish(&app, "200000000000000005", "wrong-state").await;
    drop(guard);
    assert_eq!(response.redirected_to(), "/login?error=discord_failed");
    assert!(app.session().get("discord_oauth").is_none());
    assert!(discord_user(&app, "200000000000000005").await.is_none());

    app.clear_cookies();
    let state = start(&app, &[]).await;
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("code", "discord-code:200000000000000005")
        .append_pair("state", &state)
        .append_pair("token", &token)
        .finish();
    let response = app.get(&format!("/auth/discord/callback?{query}")).await;
    assert_eq!(response.redirected_to(), "/login?error=registration_closed");
    assert!(discord_user(&app, "200000000000000005").await.is_none());
}

#[tokio::test]
async fn valid_invitations_cannot_bypass_disabled_accounts_or_password_bootstrap() {
    let (app, _discord) = app().await;
    let accounts = &app.state.accounts;
    let token = accounts.rotate_registration_invite().await.unwrap();
    accept_invite(&app, &token).await;
    assert_eq!(
        callback(&app, "200000000000000006").await.redirected_to(),
        "/login?error=registration_closed"
    );
    assert!(accounts.registration_status().await.unwrap().bootstrap);

    create_admin(&app).await;
    app.clear_cookies();
    accept_invite(&app, &token).await;
    assert_eq!(
        callback(&app, "200000000000000006").await.redirected_to(),
        "/"
    );
    let user = discord_user(&app, "200000000000000006").await.unwrap();
    accounts.disable_user(&user).await.unwrap();
    app.clear_cookies();
    accept_invite(&app, &token).await;
    assert_eq!(
        callback(&app, "200000000000000006").await.redirected_to(),
        "/login?error=account_disabled"
    );
    assert!(app.session().get("user_token").is_none());
}

#[tokio::test]
async fn revoked_invitations_do_not_block_existing_members_or_normal_open_registration() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "200000000000000007").await;
    close_registration(&app).await;
    let token = app
        .state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();
    accept_invite(&app, &token).await;
    let existing = app.cookie.lock().unwrap().clone();
    app.clear_cookies();
    accept_invite(&app, &token).await;
    let newcomer = app.cookie.lock().unwrap().clone();
    app.state
        .accounts
        .rotate_registration_invite()
        .await
        .unwrap();

    *app.cookie.lock().unwrap() = existing;
    assert_eq!(
        callback(&app, user.discord_id.as_deref().unwrap())
            .await
            .redirected_to(),
        "/"
    );
    open_registration(&app).await;
    *app.cookie.lock().unwrap() = newcomer;
    assert_eq!(
        callback(&app, "200000000000000008").await.redirected_to(),
        "/"
    );
}

#[tokio::test]
async fn oauth_failures_log_only_their_class_and_status() {
    let sentinel = "sentinel-discord-oauth-body";
    let (app, _discord) =
        app_with(ResponseTemplate::new(400).set_body_json(json!({"error": sentinel}))).await;

    let (guard, logs) = capture_logs();
    let response = callback(&app, "100000000000000099").await;
    drop(guard);
    assert_eq!(response.redirected_to(), "/login?error=discord_failed");
    let logs = logs.contents();
    assert!(!logs.contains(sentinel), "{logs}");
    assert!(!logs.contains("discord-code"), "{logs}");
    assert!(logs.contains("status=400"), "{logs}");
}

#[tokio::test]
async fn a_rejected_discord_account_is_created_once_the_administrator_opens_registration() {
    let (app, _discord) = app().await;
    let admin = create_admin(&app).await;

    assert_eq!(
        callback(&app, "100000000000000007").await.redirected_to(),
        "/login?error=registration_closed"
    );

    app.clear_cookies();
    app.log_in(&admin).await;
    assert_eq!(
        app.patch(
            "/api/admin/settings",
            json!({"settings": {"registration_enabled": true}})
        )
        .await
        .assert_json(200),
        json!({"data": {"registration_enabled": true, "detailed_stats_from": null}})
    );

    app.clear_cookies();
    assert_eq!(
        callback(&app, "100000000000000007").await.redirected_to(),
        "/"
    );
    assert_eq!(
        discord_user(&app, "100000000000000007").await.unwrap().role,
        "member"
    );
}

#[tokio::test]
async fn username_and_player_name_clashes_get_a_short_numeric_suffix() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    // "discord_user" and "Discord_User" are what the stubbed Discord profile yields.
    app.state
        .accounts
        .create_user(&json!({
            "username": "discord_user",
            "display_name": "Discord_User",
            "password": PASSWORD
        }))
        .await
        .unwrap();
    app.player("discord_user").await;
    app.player("discord_user (2)").await;

    assert_eq!(
        callback(&app, "100000000000000008").await.redirected_to(),
        "/"
    );
    let user = discord_user(&app, "100000000000000008").await.unwrap();
    assert_eq!(user.username, "discord_user2");
    assert_eq!(
        discord_player(&app, "100000000000000008").await.1,
        "Discord_User (3)"
    );

    app.clear_cookies();
    assert_eq!(
        callback(&app, "100000000000000009").await.redirected_to(),
        "/"
    );
    assert_eq!(
        discord_user(&app, "100000000000000009")
            .await
            .unwrap()
            .username,
        "discord_user3"
    );
}

#[tokio::test]
async fn an_administrator_can_rename_a_members_username() {
    let (app, _discord) = app().await;
    let admin = create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "100000000000000010").await;
    app.log_in(&admin).await;

    let path = format!("/api/admin/users/{}", user.id);
    let body = app
        .patch(&path, json!({"user": {"username": " Wax.Poetik "}}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["username"], "wax.poetik");

    let body = app
        .patch(&path, json!({"user": {"username": "owner"}}))
        .await
        .assert_json(422);
    assert_eq!(
        body["errors"]["username"],
        json!(["has already been taken"])
    );
}

#[tokio::test]
async fn callback_signs_in_an_existing_linked_member_while_registration_is_closed() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "100000000000000003").await;
    close_registration(&app).await;

    assert_eq!(
        callback(&app, "100000000000000003").await.redirected_to(),
        "/"
    );
    assert_eq!(signed_in_user(&app).await.unwrap().id, user.id);
}

#[tokio::test]
async fn request_skips_discords_consent_screen_for_members_who_already_authorized() {
    let (app, _discord) = app().await;
    let location = app.get("/auth/discord").await.redirected_to();
    let url = url::Url::parse(&location).unwrap();
    assert!(url.host_str().unwrap().contains("discord"));
    assert_eq!(query_param(&location, "prompt").as_deref(), Some("none"));
    assert_eq!(
        query_param(&location, "scope").as_deref(),
        Some("identify email")
    );
    assert_eq!(
        query_param(&location, "redirect_uri").as_deref(),
        Some("http://localhost:4002/auth/discord/callback")
    );
}

#[tokio::test]
async fn signing_in_issues_a_persistent_cookie_matching_the_session_token_validity() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    create_discord_user(&app, "100000000000000003").await;

    let response = callback(&app, "100000000000000003").await;
    assert_eq!(response.redirected_to(), "/");
    let cookie = response.header("set-cookie").unwrap();
    assert!(cookie.starts_with("_the_gathering_key="), "{cookie}");
    assert!(cookie.contains("; max-age=1209600"), "{cookie}");
    assert!(cookie.contains("; HttpOnly"), "{cookie}");
    assert!(cookie.contains("; SameSite=Lax"), "{cookie}");
    assert!(cookie.contains("; path=/"), "{cookie}");
}

#[tokio::test]
async fn callback_rejects_a_disabled_linked_member() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "100000000000000004").await;
    app.state.accounts.disable_user(&user).await.unwrap();

    assert_eq!(
        callback(&app, "100000000000000004").await.redirected_to(),
        "/login?error=account_disabled"
    );
    assert!(app.session().get("user_token").is_none());
}

#[tokio::test]
async fn password_login_fails_for_a_passwordless_discord_member() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "100000000000000005").await;
    assert_eq!(
        app.post(
            "/api/session",
            json!({"username": user.username, "password": "any-long-password"})
        )
        .await
        .assert_json(401),
        json!({"errors": {"detail": "Unauthorized"}})
    );
}

#[tokio::test]
async fn sudo_oauth_refreshes_authentication_for_the_currently_linked_discord_identity() {
    let (app, _discord) = app().await;
    create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "100000000000000006").await;
    app.log_in(&user).await;
    let old_token = app.session().get_bytes("user_token").unwrap();
    app.expire_sudo(11 * 60).await;

    let response = callback_to(&app, "100000000000000006", "/admin/users", &[("sudo", "1")]).await;
    assert_eq!(response.redirected_to(), "/admin/users");
    let new_token = app.session().get_bytes("user_token").unwrap();
    assert_ne!(new_token, old_token);
    let (reauthenticated, _) = app
        .state
        .accounts
        .get_user_by_session_token(&new_token)
        .await
        .unwrap()
        .unwrap();
    assert!(the_gathering::accounts::sudo_mode(&reauthenticated, 1));
}

#[tokio::test]
async fn sudo_oauth_rejects_another_discord_identity_and_members_without_one() {
    let (app, _discord) = app().await;
    let admin = create_admin(&app).await;
    open_registration(&app).await;
    let user = create_discord_user(&app, "100000000000000011").await;
    app.log_in(&user).await;
    assert_eq!(
        callback_to(&app, "100000000000000012", "/", &[("sudo", "1")])
            .await
            .redirected_to(),
        "/login?error=discord_sudo_mismatch"
    );

    app.log_in(&admin).await;
    assert_eq!(
        app.get("/auth/discord?sudo=1").await.redirected_to(),
        "/login?error=discord_sudo_unavailable"
    );
}

#[tokio::test]
async fn a_callback_without_an_oauth_attempt_fails() {
    let (app, _discord) = app().await;
    let (guard, _logs) = capture_logs();
    let response = finish(&app, "100000000000000013", "any-state").await;
    drop(guard);
    assert_eq!(response.redirected_to(), "/login?error=discord_failed");
}

#[tokio::test]
async fn sign_in_is_unavailable_without_discord_credentials() {
    let app = TestApp::with_config(|config| config.discord_oauth = None).await;
    assert_eq!(
        app.get("/auth/discord").await.redirected_to(),
        "/login?error=discord_unavailable"
    );
    assert_eq!(
        app.get("/api/registration").await.assert_json(200)["data"]["discord_configured"],
        false
    );
}
