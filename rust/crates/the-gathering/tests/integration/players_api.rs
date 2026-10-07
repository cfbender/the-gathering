//! The players API.
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
use the_gathering::games::Player;

struct Ctx {
    app: TestApp,
    admin: User,
    member: User,
    drew: Player,
    wax: Player,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let member = app.unique_member().await;
    let drew = app.player("Drew").await;
    let wax = app.player("waxpoetik").await;
    Ctx {
        app,
        admin,
        member,
        drew,
        wax,
    }
}

#[tokio::test]
async fn members_cannot_claim_an_account_or_discord_identity_through_patch() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.member).await;
    let body = ctx
        .app
        .patch(
            &format!("/api/players/{}", ctx.drew.id),
            json!({"player": {"name": "Drew!", "user_id": ctx.member.id, "discord_id": "1"}}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["name"], "Drew!");
    assert_eq!(body["data"]["user_id"], serde_json::Value::Null);
    assert_eq!(
        ctx.app
            .state
            .games
            .get_player(ctx.drew.id)
            .await
            .unwrap()
            .unwrap()
            .discord_id,
        None
    );
}

#[tokio::test]
async fn members_cannot_claim_an_account_or_discord_identity_through_post() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.member).await;
    let body = ctx
        .app
        .post("/api/players", json!({"player": {"name": "Injected", "user_id": ctx.member.id, "discord_id": "victim-id"}}))
        .await
        .assert_json(201);
    let player = ctx
        .app
        .state
        .games
        .get_player(body["data"]["id"].as_i64().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(player.user_id, None);
    assert_eq!(player.discord_id, None);
    assert_eq!(body["data"]["games_played"], 0);
    assert_eq!(body["data"]["decks"], json!([]));
}

#[tokio::test]
async fn members_cannot_rename_another_members_linked_player() {
    let ctx = setup().await;
    ctx.app
        .state
        .games
        .link_player_to_user(&ctx.drew, &ctx.admin)
        .await
        .unwrap();
    ctx.app.log_in(&ctx.member).await;
    let path = format!("/api/players/{}", ctx.drew.id);
    assert_eq!(
        ctx.app
            .patch(&path, json!({"player": {"name": "Nope"}}))
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
    ctx.app.delete(&path).await.assert_json(403);
    assert_eq!(
        ctx.app
            .state
            .games
            .get_player(ctx.drew.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Drew"
    );
}

#[tokio::test]
async fn admins_inside_the_sudo_window_merge_players_stale_admins_and_members_receive_403() {
    let ctx = setup().await;
    let path = format!("/api/players/{}/merge", ctx.wax.id);
    let target = json!({"target_id": ctx.drew.id});
    ctx.app.log_in(&ctx.member).await;
    assert_eq!(
        ctx.app.post(&path, target.clone()).await.assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );

    ctx.app.log_in(&ctx.admin).await;
    ctx.app.expire_sudo(602).await;
    assert_eq!(
        ctx.app.post(&path, target.clone()).await.assert_json(403)["errors"]["code"],
        "sudo_required"
    );

    ctx.app.expire_sudo(598).await;
    let body = ctx.app.post(&path, target.clone()).await.assert_json(200);
    assert_eq!(body["data"]["id"], ctx.drew.id);
    assert!(
        ctx.app
            .state
            .games
            .get_player(ctx.wax.id)
            .await
            .unwrap()
            .is_none()
    );

    ctx.app.log_in(&ctx.admin).await;
    assert_eq!(
        ctx.app.post(&path, target).await.assert_json(404),
        json!({"errors": {"detail": "Not Found"}})
    );
}

#[tokio::test]
async fn admins_link_an_account_to_a_player_and_the_players_list_exposes_user_id() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.admin).await;
    let body = ctx
        .app
        .put(
            &format!("/api/admin/users/{}/player", ctx.member.id),
            json!({"player_id": ctx.drew.id}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["user_id"], ctx.member.id);

    let players = ctx.app.get("/api/players").await.assert_json(200);
    let drew = players["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == ctx.drew.id)
        .unwrap();
    assert_eq!(drew["user_id"], ctx.member.id);

    let other = ctx.app.unique_member().await;
    assert_eq!(
        ctx.app
            .put(
                &format!("/api/admin/users/{}/player", other.id),
                json!({"player_id": ctx.drew.id})
            )
            .await
            .assert_json(422),
        json!({"errors": {"merge": ["players belong to different accounts"]}})
    );
}

#[tokio::test]
async fn player_paths_reject_non_integer_ids_and_players_with_history_cannot_be_deleted() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.member).await;
    ctx.app.get("/api/players/abc").await.assert_json(400);
    ctx.app.get("/api/players/0").await.assert_json(404);
    ctx.app
        .patch(
            &format!("/api/players/{}", ctx.drew.id),
            json!({"name": "no wrapper"}),
        )
        .await
        .assert_json(400);
    ctx.app
        .post("/api/players", json!({"name": "no wrapper"}))
        .await
        .assert_json(400);

    ctx.app
        .simple_game("2026-09-19T18:00:00Z", ctx.drew.id, ctx.wax.id, None)
        .await;
    assert_eq!(
        ctx.app
            .delete(&format!("/api/players/{}", ctx.drew.id))
            .await
            .assert_json(422),
        json!({"errors": {"game_players": ["are still associated with this entry"]}})
    );
    let guest = ctx.app.player("Guest").await;
    let response = ctx.app.delete(&format!("/api/players/{}", guest.id)).await;
    assert_eq!(response.status.as_u16(), 204);

    let detail = ctx
        .app
        .get(&format!("/api/players/{}", ctx.drew.id))
        .await
        .assert_json(200);
    assert_eq!(detail["data"]["games_played"], 1);
    assert_eq!(detail["data"]["wins"], 1);
    assert_eq!(detail["data"]["recent_games"][0]["result"], "win");
    assert_eq!(detail["data"]["recent_games"][0]["format"], "commander");
    assert_eq!(detail["data"]["avatar_url"], serde_json::Value::Null);
    assert_eq!(detail["data"]["discord_id"], serde_json::Value::Null);
}
