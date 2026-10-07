//! Accounts: registration, sign-in, sessions, profiles, administration, and the
//! immediate write transactions accounts rely on.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use serde_json::{Value, json};
use sqlx::Connection;
use std::collections::BTreeSet;
use support::TestApp;
use the_gathering::accounts::discord::{DiscordClaims, SignInError};
use the_gathering::accounts::user::PALETTES;
use the_gathering::accounts::{RegisterError, User};
use the_gathering::db::{self, UtcDateTime};
use the_gathering::error::ApiError;
use the_gathering::validation::ValidationError;

fn valid() -> Value {
    json!({
        "username": "Player.One",
        "display_name": "Player One",
        "password": "long-enough-password"
    })
}

async fn create_admin(app: &TestApp) -> User {
    let mut attrs = valid();
    attrs["role"] = json!("admin");
    app.state
        .accounts
        .create_user(&support::input(attrs))
        .await
        .unwrap()
}

async fn user_with_display_name(app: &TestApp, display_name: &str) -> User {
    app.state
        .accounts
        .create_user(&support::input(json!({
            "username": format!("user{}", support::unique()),
            "display_name": display_name,
            "password": support::PASSWORD,
            "role": "member",
        })))
        .await
        .unwrap()
}

fn validation_errors(error: ApiError) -> ValidationError {
    match error {
        ApiError::Validation(errors) => errors,
        other => panic!("expected validation errors, got {other:?}"),
    }
}

async fn player_name(app: &TestApp, id: i64) -> String {
    sqlx::query_scalar::<_, String>("SELECT name FROM players WHERE id = ?")
        .bind(id)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

fn claims(sub: &str, preferred_username: &str) -> DiscordClaims {
    DiscordClaims {
        sub: sub.into(),
        preferred_username: Some(preferred_username.into()),
        picture: None,
    }
}

#[tokio::test]
async fn the_first_registration_becomes_admin_and_registration_then_closes() {
    let app = TestApp::new().await;
    let accounts = &app.state.accounts;
    let status = accounts.registration_status().await.unwrap();
    assert!(status.allowed && status.bootstrap);

    let user = accounts
        .register_user(&support::input(valid()))
        .await
        .unwrap();
    assert_eq!(user.role, "admin");
    assert_eq!(user.username, "player.one");

    let status = accounts.registration_status().await.unwrap();
    assert!(!status.allowed && !status.bootstrap);

    let mut another = valid();
    another["username"] = json!("another");
    assert!(matches!(
        accounts
            .register_user(&support::input(another.clone()))
            .await,
        Err(RegisterError::Closed)
    ));

    accounts
        .update_settings(&support::input(json!({"registration_enabled": true})))
        .await
        .unwrap();
    another["username"] = json!("password-member");
    assert!(matches!(
        accounts
            .register_user(&support::input(another.clone()))
            .await,
        Err(RegisterError::Closed)
    ));
}

#[tokio::test]
async fn usernames_are_unique_after_case_normalization() {
    let app = TestApp::new().await;
    create_admin(&app).await;
    let result = app
        .state
        .accounts
        .create_user(&support::input(json!({
            "username": "PLAYER.ONE",
            "display_name": "Someone Else",
            "password": "another-long-password",
            "role": "member"
        })))
        .await;
    let Err(RegisterError::Invalid(errors)) = result else {
        panic!("expected a validation error, got {result:?}");
    };
    assert!(
        errors
            .messages("username")
            .contains(&"has already been taken".to_owned())
    );
}

#[tokio::test]
async fn disabled_users_cannot_authenticate() {
    let app = TestApp::new().await;
    create_admin(&app).await;
    let accounts = &app.state.accounts;
    let user = accounts
        .create_user(&support::input(json!({
            "username": "member",
            "display_name": "Member",
            "password": "member-long-password",
            "role": "member"
        })))
        .await
        .unwrap();
    accounts.disable_user(&user).await.unwrap();
    assert!(
        accounts
            .get_user_by_username_and_password("MEMBER", "member-long-password")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn disabling_revokes_sessions_permanently_while_re_enabled_admins_can_sign_in_again() {
    let app = TestApp::new().await;
    create_admin(&app).await;
    let accounts = &app.state.accounts;
    let user = accounts
        .create_user(&support::input(json!({
            "username": "second-admin",
            "display_name": "Second Admin",
            "password": "second-admin-password",
            "role": "admin"
        })))
        .await
        .unwrap();

    let stolen = accounts.generate_user_session_token(&user).await.unwrap();
    assert!(
        accounts
            .get_user_by_session_token(&stolen)
            .await
            .unwrap()
            .is_some()
    );

    let disabled = accounts.disable_user(&user).await.unwrap();
    assert!(
        accounts
            .get_user_by_session_token(&stolen)
            .await
            .unwrap()
            .is_none()
    );

    accounts
        .update_user(&disabled, &support::input(json!({"disabled": false})))
        .await
        .unwrap();
    assert!(
        accounts
            .get_user_by_session_token(&stolen)
            .await
            .unwrap()
            .is_none()
    );

    let fresh = accounts
        .get_user_by_username_and_password("second-admin", "second-admin-password")
        .await
        .unwrap()
        .expect("re-enabled admin signs in");
    let token = accounts.generate_user_session_token(&fresh).await.unwrap();
    assert!(
        accounts
            .get_user_by_session_token(&token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn a_profile_edit_renames_the_linked_player() {
    let app = TestApp::new().await;
    let user = user_with_display_name(&app, "cody_discord").await;
    let player = app
        .player_with(json!({"name": "cody_discord"}), Some(user.id))
        .await;

    let updated = app
        .state
        .accounts
        .update_profile(
            &user,
            &support::input(json!({"display_name": "  Cody  "})),
            |_| false,
        )
        .await
        .unwrap();
    assert_eq!(updated.display_name, "Cody");
    assert_eq!(player_name(&app, player.id).await, "Cody");
}

#[tokio::test]
async fn an_admin_edit_renames_the_linked_player() {
    let app = TestApp::new().await;
    let user = user_with_display_name(&app, "member").await;
    let player = app
        .player_with(json!({"name": "member"}), Some(user.id))
        .await;

    app.state
        .accounts
        .update_user(
            &user,
            &support::input(json!({"display_name": "Member Name"})),
        )
        .await
        .unwrap();
    assert_eq!(player_name(&app, player.id).await, "Member Name");
}

#[tokio::test]
async fn a_name_held_by_another_player_is_rejected_without_saving_either_record() {
    let app = TestApp::new().await;
    let user = user_with_display_name(&app, "member").await;
    let player = app
        .player_with(json!({"name": "member"}), Some(user.id))
        .await;
    app.player("Taken").await;

    let error = app
        .state
        .accounts
        .update_profile(
            &user,
            &support::input(json!({"display_name": "taken"})),
            |_| false,
        )
        .await
        .unwrap_err();
    assert!(
        validation_errors(error)
            .messages("display_name")
            .contains(&"is already used by another player".to_owned())
    );
    assert_eq!(app.reload(&user).await.unwrap().display_name, "member");
    assert_eq!(player_name(&app, player.id).await, "member");
}

#[tokio::test]
async fn users_without_a_linked_player_can_still_change_their_display_name() {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let updated = app
        .state
        .accounts
        .update_profile(
            &user,
            &support::input(json!({"display_name": "Solo"})),
            |_| false,
        )
        .await
        .unwrap();
    assert_eq!(updated.display_name, "Solo");
}

#[tokio::test]
async fn revoking_all_sessions_deletes_only_the_target_users_tokens() {
    let app = TestApp::new().await;
    let accounts = &app.state.accounts;
    let target = app.unique_member().await;
    let other = app.unique_member().await;
    let target_tokens = [
        accounts.generate_user_session_token(&target).await.unwrap(),
        accounts.generate_user_session_token(&target).await.unwrap(),
    ];
    let other_token = accounts.generate_user_session_token(&other).await.unwrap();

    accounts.revoke_all_sessions(target.id).await.unwrap();
    for token in &target_tokens {
        assert!(
            accounts
                .get_user_by_session_token(token)
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        accounts
            .get_user_by_session_token(&other_token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn issuing_a_session_prunes_expired_token_rows() {
    let app = TestApp::new().await;
    let accounts = &app.state.accounts;
    let user = create_admin(&app).await;
    let expired = accounts.generate_user_session_token(&user).await.unwrap();
    sqlx::query("UPDATE users_tokens SET inserted_at = ? WHERE token = ?")
        .bind(UtcDateTime::now().plus(time::Duration::days(-15)))
        .bind(&expired)
        .execute(app.pool())
        .await
        .unwrap();

    let fresh = accounts.generate_user_session_token(&user).await.unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM users_tokens WHERE token = ?")
        .bind(&expired)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    assert!(
        accounts
            .get_user_by_session_token(&fresh)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn discord_sign_in_surfaces_a_player_ownership_conflict_and_rolls_back_the_user() {
    let app = TestApp::new().await;
    let accounts = &app.state.accounts;
    create_admin(&app).await;
    accounts
        .update_settings(&support::input(json!({"registration_enabled": true})))
        .await
        .unwrap();
    let owner = accounts
        .create_user(&support::input(json!({
            "username": "player-owner",
            "display_name": "Player Owner",
            "password": "another-long-password",
            "role": "member"
        })))
        .await
        .unwrap();
    app.player_with(
        json!({"name": "Claimed", "discord_id": "discord-conflict"}),
        Some(owner.id),
    )
    .await;

    let result = accounts
        .sign_in_with_discord(&claims("discord-conflict", "Claimed"), None)
        .await;
    assert!(
        matches!(result, Err(SignInError::DiscordIdentityConflict)),
        "{result:?}"
    );
    assert!(
        accounts
            .get_user_by_discord_id("discord-conflict")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn discord_sign_in_derives_a_valid_username_from_handles_the_format_rule_rejects() {
    let app = TestApp::new().await;
    let accounts = &app.state.accounts;
    create_admin(&app).await;
    accounts
        .update_settings(&support::input(json!({"registration_enabled": true})))
        .await
        .unwrap();

    // Discord allows a leading dot; our usernames must start with a letter or digit.
    let dotted = accounts
        .sign_in_with_discord(&claims("dot-1", ".dreamlan"), None)
        .await
        .unwrap();
    assert_eq!(dotted.username, "dreamlan");
    assert_eq!(dotted.display_name, ".dreamlan");

    // A handle that is only punctuation falls back to the generic base.
    let punct = accounts
        .sign_in_with_discord(&claims("dot-2", "._."), None)
        .await
        .unwrap();
    assert_eq!(punct.username, "discord");

    // Truncation to 32 characters must not leave trailing punctuation behind.
    let long = format!("{}._long_tail", "a".repeat(31));
    let truncated = accounts
        .sign_in_with_discord(&claims("dot-3", &long), None)
        .await
        .unwrap();
    assert_eq!(truncated.username, "a".repeat(31));
}

#[tokio::test]
async fn reissued_session_tokens_preserve_the_original_password_authentication_time() {
    let app = TestApp::new().await;
    let accounts = &app.state.accounts;
    let user = create_admin(&app).await;
    let authenticated_at = UtcDateTime::now().plus(time::Duration::minutes(-30));
    let token = accounts
        .generate_user_session_token(&User {
            authenticated_at: Some(authenticated_at),
            ..user
        })
        .await
        .unwrap();
    let (fetched, _inserted_at) = accounts
        .get_user_by_session_token(&token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.authenticated_at, Some(authenticated_at));
}

// -- user ------------------------------------------------------------------------

fn react_src(relative: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../assets/react/src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn palette_ids_match_the_react_picker_and_every_palette_has_light_and_dark_css() {
    let picker = regex::Regex::new(r#"\{ id: "(\w+)", label:"#).unwrap();
    let theme = react_src("lib/theme.tsx");
    let picker_ids: Vec<&str> = picker
        .captures_iter(&theme)
        .map(|captures| captures.get(1).unwrap().as_str())
        .collect();
    assert_eq!(picker_ids, PALETTES);

    let block =
        regex::Regex::new(r#"\[data-palette="(\w+)"\]\[data-theme="(light|dark)"\]"#).unwrap();
    let css = react_src("palettes.css");
    let css_blocks: BTreeSet<(String, String)> = block
        .captures_iter(&css)
        .map(|captures| (captures[1].to_owned(), captures[2].to_owned()))
        .collect();

    // Claret is the base daisyUI light/dark theme in app.css, so it has no override block.
    let expected: BTreeSet<(String, String)> = PALETTES
        .iter()
        .filter(|palette| **palette != "claret")
        .flat_map(|palette| ["light", "dark"].map(|mode| ((*palette).to_owned(), mode.to_owned())))
        .collect();
    assert_eq!(css_blocks, expected);
}

// -- repo ------------------------------------------------------------------------

/// Deferred SQLite transactions that read before writing fail immediately with "database is
/// locked" when another connection commits in between, e.g. registering the first admin
/// while the catalog sync is running. Immediate mode queues on `busy_timeout` instead.
#[tokio::test]
async fn write_transactions_take_the_sqlite_write_lock_up_front() {
    let app = TestApp::new().await;
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(busy_timeout >= 5_000, "busy_timeout = {busy_timeout}");

    let mut first = db::begin(app.pool()).await.unwrap();
    // No statement has run yet, but the write lock is already held: another writer that
    // will not wait is refused at once.
    let mut other = sqlx::SqliteConnection::connect(&format!(
        "sqlite://{}",
        app.state.config.database_path.display()
    ))
    .await
    .unwrap();
    sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut other)
        .await
        .unwrap();
    let error = sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("locked"), "{error}");
    sqlx::query("UPDATE server_settings SET registration_enabled = 1 WHERE id = 1")
        .execute(&mut *first)
        .await
        .unwrap();
    first.commit().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .await
        .unwrap();
    sqlx::query("ROLLBACK").execute(&mut other).await.unwrap();
}
