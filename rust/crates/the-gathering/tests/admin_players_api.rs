//! The admin player identities API.
// Test crates: helpers outside `#[test]` functions may unwrap and index freely, like the
// tests themselves (clippy.toml only exempts `#[test]` bodies).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty
)]

mod support;

use serde_json::{Value, json};
use support::TestApp;
use the_gathering::accounts::discord::DiscordClaims;

async fn admin_app() -> (TestApp, the_gathering::accounts::User) {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    (app, admin)
}

#[tokio::test]
async fn lists_archived_and_orphaned_identities_with_pagination_and_search() {
    let (app, _admin) = admin_app().await;
    let user = app.member("linked_member").await;
    let linked = app
        .player_with(json!({"name": "Alpha", "discord_id": "111"}), Some(user.id))
        .await;
    let archived = app
        .player_with(
            json!({"name": "Beta", "discord_id": "222", "archived_at": "2026-09-20T00:00:00Z"}),
            None,
        )
        .await;
    app.player("Gamma").await;

    let body = app
        .get("/api/admin/players?per_page=1&page=2")
        .await
        .assert_json(200);
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], archived.id);
    assert_eq!(rows[0]["discord_id"], "222");
    assert_eq!(rows[0]["user"], Value::Null);
    assert_ne!(rows[0]["archived_at"], Value::Null);
    assert_eq!(
        body["meta"],
        json!({"page": 2, "per_page": 1, "total": 3, "total_pages": 3})
    );

    for search in ["ALPHA", "111", "LINKED_MEMBER"] {
        let body = app
            .get(&format!("/api/admin/players?search={search}"))
            .await
            .assert_json(200);
        let rows = body["data"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{search}");
        assert_eq!(rows[0]["id"], linked.id);
        assert_eq!(
            rows[0]["user"],
            json!({"id": user.id, "username": "linked_member"})
        );
    }
    assert_eq!(
        app.get("/api/admin/players?search=absent")
            .await
            .assert_json(200)["data"],
        json!([])
    );
}

#[tokio::test]
async fn unlinking_an_orphaned_discord_identity_permits_merging_while_retaining_history() {
    let (app, _admin) = admin_app().await;
    let source = app
        .player_with(
            json!({"name": "Imported", "discord_id": "old-discord"}),
            None,
        )
        .await;
    let target = app
        .player_with(
            json!({"name": "Correct", "discord_id": "correct-discord"}),
            None,
        )
        .await;
    let opponent = app.player("Opponent").await;
    let deck = app.deck(source.id, "Original deck", "Alela").await;
    let game = app
        .game(
            json!({"played_at": "2026-09-20T18:00:00Z", "seats": [
                {"player_id": source.id, "deck_id": deck.id, "seat": 1, "result": "win"},
                {"player_id": opponent.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let merge = format!("/api/players/{}/merge", source.id);
    app.post(&merge, json!({"target_id": target.id}))
        .await
        .assert_json(422);

    let response = app
        .delete(&format!("/api/admin/players/{}/identity", source.id))
        .await;
    assert_eq!(response.status.as_u16(), 204);
    let games = &app.state.games;
    assert_eq!(
        games
            .get_player(source.id)
            .await
            .unwrap()
            .unwrap()
            .discord_id,
        None
    );
    assert_eq!(
        games.get_deck(deck.id).await.unwrap().unwrap().player_id,
        source.id
    );
    assert!(
        games
            .get_game(game.id)
            .await
            .unwrap()
            .unwrap()
            .seats
            .iter()
            .any(|seat| seat.player_id == source.id)
    );

    app.post(&merge, json!({"target_id": target.id}))
        .await
        .assert_json(200);
    assert_eq!(
        games
            .get_player(target.id)
            .await
            .unwrap()
            .unwrap()
            .discord_id
            .as_deref(),
        Some("correct-discord")
    );
    assert_eq!(
        games.get_deck(deck.id).await.unwrap().unwrap().player_id,
        target.id
    );
    assert!(
        games
            .get_game(game.id)
            .await
            .unwrap()
            .unwrap()
            .seats
            .iter()
            .any(|seat| seat.player_id == target.id)
    );
}

#[tokio::test]
async fn unlinking_detaches_the_account_without_breaking_subsequent_discord_sign_in() {
    let (app, _admin) = admin_app().await;
    app.settings(json!({"registration_enabled": true})).await;
    let claims = DiscordClaims {
        sub: "linked-discord".into(),
        preferred_username: Some("linked_member".into()),
        picture: None,
    };
    let user = app
        .state
        .accounts
        .sign_in_with_discord(&claims, None)
        .await
        .unwrap();
    let player = app
        .state
        .games
        .get_player_for_user(user.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        app.delete(&format!("/api/admin/players/{}/identity", player.id))
            .await
            .status
            .as_u16(),
        204
    );
    let games = &app.state.games;
    let unlinked = games.get_player(player.id).await.unwrap().unwrap();
    assert_eq!((unlinked.user_id, unlinked.discord_id), (None, None));
    assert_eq!(
        app.state
            .accounts
            .get_user(user.id)
            .await
            .unwrap()
            .unwrap()
            .discord_id
            .as_deref(),
        Some("linked-discord")
    );
    let signed_in = app
        .state
        .accounts
        .sign_in_with_discord(&claims, None)
        .await
        .unwrap();
    assert_eq!(signed_in.id, user.id);
    assert_ne!(
        games
            .get_player_for_user(user.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        player.id
    );
    assert_eq!(
        games.get_player(player.id).await.unwrap().unwrap().user_id,
        None
    );
    assert_eq!(
        app.delete(&format!("/api/admin/players/{}/identity", player.id))
            .await
            .status
            .as_u16(),
        204
    );
}

#[tokio::test]
async fn requires_admin_and_recent_sudo_authentication_missing_players_return_404() {
    let (app, admin) = admin_app().await;
    let player = app
        .player_with(json!({"name": "Protected", "discord_id": "keep"}), None)
        .await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    app.get("/api/admin/players").await.assert_json(403);
    app.delete(&format!("/api/admin/players/{}/identity", player.id))
        .await
        .assert_json(403);

    app.log_in(&admin).await;
    app.delete("/api/admin/players/0/identity")
        .await
        .assert_json(404);
    app.expire_sudo(11 * 60).await;
    assert_eq!(
        app.get("/api/admin/players").await.assert_json(403)["errors"]["code"],
        "sudo_required"
    );
    assert_eq!(
        app.delete(&format!("/api/admin/players/{}/identity", player.id))
            .await
            .assert_json(403)["errors"]["code"],
        "sudo_required"
    );
    assert_eq!(
        app.state
            .games
            .get_player(player.id)
            .await
            .unwrap()
            .unwrap()
            .discord_id
            .as_deref(),
        Some("keep")
    );
    assert!(
        app.state
            .accounts
            .get_user(admin.id)
            .await
            .unwrap()
            .is_some()
    );
}
