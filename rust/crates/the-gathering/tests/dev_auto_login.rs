//! Ported from `test/the_gathering_web/dev_auto_login_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use support::TestApp;

async fn app() -> TestApp {
    TestApp::with_config(|config| config.dev_auto_login = true).await
}

#[tokio::test]
async fn anonymous_requests_are_signed_in_as_a_newly_created_dev_admin() {
    let app = app().await;
    let body = app.get("/api/session").await.assert_json(200);
    assert_eq!(body["data"]["username"], "dev");
    assert_eq!(body["data"]["role"], "admin");
    assert_eq!(body["data"]["has_password"], false);

    assert!(app.session().get_bytes("user_token").is_some());
    let users = app.state.accounts.list_users().await.unwrap();
    assert_eq!(
        users
            .iter()
            .map(|u| u.username.as_str())
            .collect::<Vec<_>>(),
        ["dev"]
    );

    // The issued session is reused afterwards rather than signing in again.
    let token = app.session().get_bytes("user_token");
    app.get("/api/session").await.assert_json(200);
    assert_eq!(app.session().get_bytes("user_token"), token);
}

#[tokio::test]
async fn an_existing_enabled_admin_is_reused_instead_of_creating_dev() {
    let app = app().await;
    app.member("member").await;
    let retired = app.admin("retired").await;
    app.admin("owner").await;
    app.state.accounts.disable_user(&retired).await.unwrap();

    let body = app.get("/api/session").await.assert_json(200);
    assert_eq!(body["data"]["username"], "owner");
    assert!(
        app.state
            .accounts
            .get_user_by_username("dev")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn admin_routes_skip_sudo_re_authentication() {
    let app = app().await;
    let body = app.get("/api/admin/settings").await.assert_json(200);
    assert!(body["data"].get("registration_enabled").is_some());
}

#[tokio::test]
async fn auto_login_is_off_unless_configured() {
    let app = TestApp::new().await;
    app.get("/api/session").await.assert_json(401);
}
