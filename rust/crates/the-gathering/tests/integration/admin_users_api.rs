//! The admin users API.
// Test crates: helpers outside `#[test]` functions may unwrap and index freely, like the
// tests themselves (clippy.toml only exempts `#[test]` bodies).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty
)]

use crate::support;

use serde_json::json;
use support::TestApp;
use the_gathering::accounts::User;
use the_gathering::accounts::discord::DiscordClaims;

async fn discord_user(app: &TestApp, discord_id: &str) -> User {
    let claims = DiscordClaims {
        sub: discord_id.into(),
        preferred_username: Some("Discord Member".into()),
        picture: Some(format!(
            "https://cdn.discordapp.com/avatars/{discord_id}/avatar-hash"
        )),
    };
    app.state
        .accounts
        .sign_in_with_discord(&claims, None)
        .await
        .unwrap()
}

async fn has_tokens(app: &TestApp, user: &User) -> bool {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users_tokens WHERE user_id = ?")
        .bind(user.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    count > 0
}

#[tokio::test]
async fn delete_refuses_a_player_with_games_without_changing_their_account_or_history() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.settings(json!({"registration_enabled": true})).await;
    let user = discord_user(&app, "100000000000000101").await;
    let player = app
        .state
        .games
        .get_player_for_user(user.id)
        .await
        .unwrap()
        .unwrap();
    let opponent = app.player("Opponent").await;
    let deck = app
        .deck(player.id, "History Deck", "Alela, Artful Provocateur")
        .await;
    let game = app
        .game(
            json!({"played_at": "2026-09-20T18:00:00Z", "seats": [
                {"player_id": player.id, "deck_id": deck.id, "seat": 1, "result": "win"},
                {"player_id": opponent.id, "seat": 2, "result": "loss"},
            ]}),
            Some(user.id),
        )
        .await;
    let target_token = app
        .state
        .accounts
        .generate_user_session_token(&user)
        .await
        .unwrap();

    app.log_in(&admin).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", user.id))
            .await
            .assert_json(422),
        json!({"errors": {"player": ["must have zero games before deleting this user"]}})
    );
    assert!(app.reload(&user).await.is_some());
    assert!(
        app.state
            .accounts
            .get_user_by_session_token(&target_token)
            .await
            .unwrap()
            .is_some()
    );
    assert!(has_tokens(&app, &user).await);
    let stored = app
        .state
        .games
        .get_player(player.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.user_id, Some(user.id));
    assert_eq!(stored.discord_id, user.discord_id);
    let game = app.state.games.get_game(game.id).await.unwrap().unwrap();
    assert_eq!(game.created_by_user_id, Some(user.id));
    assert_eq!(
        app.state
            .games
            .get_deck(deck.id)
            .await
            .unwrap()
            .unwrap()
            .player_id,
        player.id
    );
    assert_eq!(
        game.seats
            .iter()
            .map(|seat| seat.player_id)
            .collect::<Vec<_>>(),
        [player.id, opponent.id]
    );
}

#[tokio::test]
async fn delete_removes_a_zero_game_player_their_decks_account_and_sessions() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.settings(json!({"registration_enabled": true})).await;
    let user = discord_user(&app, "delete-empty-player").await;
    let player = app
        .state
        .games
        .get_player_for_user(user.id)
        .await
        .unwrap()
        .unwrap();
    let deck = app.deck(player.id, "Unused", "Alela").await;
    let other = app
        .player_with(
            json!({"name": "Unrelated", "discord_id": "keep-this-identity"}),
            None,
        )
        .await;
    let target_token = app
        .state
        .accounts
        .generate_user_session_token(&user)
        .await
        .unwrap();

    app.log_in(&admin).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", user.id))
            .await
            .status
            .as_u16(),
        204
    );
    assert!(app.reload(&user).await.is_none());
    assert!(
        app.state
            .games
            .get_player(player.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(app.state.games.get_deck(deck.id).await.unwrap().is_none());
    assert!(
        app.state
            .accounts
            .get_user_by_session_token(&target_token)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        app.state
            .games
            .get_player(other.id)
            .await
            .unwrap()
            .unwrap()
            .discord_id
            .as_deref(),
        Some("keep-this-identity")
    );
}

#[tokio::test]
async fn delete_allows_an_account_without_a_player_and_preserves_games_they_recorded() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let user = app.unique_member().await;
    let one = app.player("One").await;
    let two = app.player("Two").await;
    let game = app
        .simple_game("2026-09-20T18:00:00Z", one.id, two.id, Some(user.id))
        .await;
    app.log_in(&admin).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", user.id))
            .await
            .status
            .as_u16(),
        204
    );
    let game = app.state.games.get_game(game.id).await.unwrap().unwrap();
    assert_eq!(game.created_by_user_id, None);
    assert_eq!(game.seats.len(), 2);
}

#[tokio::test]
async fn an_administrator_cannot_delete_their_own_account() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", admin.id))
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
    assert!(app.reload(&admin).await.is_some());
}

#[tokio::test]
async fn delete_sessions_revokes_the_target_users_sessions() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let user = app.unique_member().await;
    let target_token = app
        .state
        .accounts
        .generate_user_session_token(&user)
        .await
        .unwrap();
    let admin_token = app
        .state
        .accounts
        .generate_user_session_token(&admin)
        .await
        .unwrap();
    app.log_in(&admin).await;
    let response = app
        .delete(&format!("/api/admin/users/{}/sessions", user.id))
        .await
        .assert_json(200);
    assert_eq!(response["data"]["id"], user.id);
    assert_eq!(response["data"]["disabled"], false);
    assert!(
        app.state
            .accounts
            .get_user_by_session_token(&target_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        app.state
            .accounts
            .get_user_by_session_token(&admin_token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn delete_sessions_returns_404_for_an_unknown_user() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    assert_eq!(
        app.delete("/api/admin/users/0/sessions")
            .await
            .assert_json(404),
        json!({"errors": {"detail": "Not Found"}})
    );
}

#[tokio::test]
async fn session_revocation_requires_recent_sudo_authentication() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    app.log_in(&admin).await;
    app.expire_sudo(11 * 60).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}/sessions", member.id))
            .await
            .assert_json(403),
        json!({"errors": {"code": "sudo_required", "detail": "Reauthentication required"}})
    );
}

#[tokio::test]
async fn the_last_administrator_cannot_be_deleted() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    assert!(matches!(
        app.state.accounts.delete_user(&admin, &member).await,
        Err(the_gathering::error::ApiError::Forbidden)
    ));
    assert!(app.reload(&admin).await.is_some());
}

#[tokio::test]
async fn a_non_administrator_receives_403() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", admin.id))
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
}

#[tokio::test]
async fn deletion_requires_recent_sudo_authentication() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    app.log_in(&admin).await;
    app.expire_sudo(11 * 60).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", member.id))
            .await
            .assert_json(403),
        json!({"errors": {"code": "sudo_required", "detail": "Reauthentication required"}})
    );
    assert!(app.reload(&member).await.is_some());
}

#[tokio::test]
async fn link_player_needs_a_player_id_and_known_records() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    let player = app.player("Drew").await;
    app.log_in(&admin).await;
    app.put(&format!("/api/admin/users/{}/player", member.id), json!({}))
        .await
        .assert_json(400);
    app.put("/api/admin/users/0/player", json!({"player_id": player.id}))
        .await
        .assert_json(404);
    app.put(
        &format!("/api/admin/users/{}/player", member.id),
        json!({"player_id": 0}),
    )
    .await
    .assert_json(404);
    let body = app
        .put(
            &format!("/api/admin/users/{}/player", member.id),
            json!({"player_id": player.id.to_string()}),
        )
        .await;
    assert_eq!(body.assert_json(200)["data"]["user_id"], member.id);
}
