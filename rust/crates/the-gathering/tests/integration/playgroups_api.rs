//! Playgroups: listing, switching, administration, and scoping of the other resources.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use crate::support;

use serde_json::{Value, json};
use support::{PLAYGROUP, TestApp};
use the_gathering::accounts::User;
use the_gathering::accounts::discord::DiscordClaims;

async fn admin_app() -> (TestApp, User) {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    (app, admin)
}

fn ids(body: &Value) -> Vec<i64> {
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn members_list_only_the_playgroups_they_belong_to_and_start_in_the_default_one() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    let other = app.unique_member().await;
    app.log_in(&member).await;
    let second = app.playgroup("Thursday crew", Some("4242"), &member).await;
    let private = app.playgroup("Someone else's", None, &other).await;

    assert_eq!(
        app.get("/api/session").await.assert_json(200)["data"]["playgroup_id"],
        PLAYGROUP
    );
    let body = app.get("/api/playgroups").await.assert_json(200);
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], PLAYGROUP);
    assert_eq!(rows[0]["name"], "Main");
    assert_eq!(rows[0]["discord_guild_id"], Value::Null);
    assert_eq!(rows[0]["invite_enabled"], false);
    assert_eq!(rows[1]["id"], second);
    assert_eq!(rows[1]["name"], "Thursday crew");
    assert_eq!(rows[1]["discord_guild_id"], "4242");
    assert!(rows[1].get("members").is_none());

    // A playgroup the member does not belong to is invisible and cannot be switched to.
    assert_eq!(
        app.patch("/api/session/playgroup", json!({"playgroup_id": private}))
            .await
            .assert_json(422),
        json!({"errors": {"playgroup_id": ["does not exist"]}})
    );
    assert_eq!(
        app.reload(&member).await.unwrap().playgroup_id,
        Some(PLAYGROUP)
    );
    app.log_in(&other).await;
    assert_eq!(
        ids(&app.get("/api/playgroups").await.assert_json(200)),
        [PLAYGROUP, private]
    );
}

#[tokio::test]
async fn an_account_without_memberships_cannot_use_scoped_resources() {
    let (app, admin) = admin_app().await;
    let member = app.unique_member().await;
    let main = app.state.playgroups.get(PLAYGROUP).await.unwrap().unwrap();
    app.state
        .playgroups
        .remove_member(&main, member.id)
        .await
        .unwrap();
    let member = app.reload(&member).await.unwrap();
    assert_eq!(member.playgroup_id, None);

    app.log_in(&member).await;
    assert_eq!(
        app.get("/api/session").await.assert_json(200)["data"]["playgroup_id"],
        Value::Null
    );
    assert_eq!(
        app.get("/api/playgroups").await.assert_json(200)["data"],
        json!([])
    );
    for path in [
        "/api/players",
        "/api/decks",
        "/api/games",
        "/api/stats/overview",
    ] {
        assert_eq!(
            app.get(path).await.assert_json(403),
            json!({"errors": {"code": "no_playgroup", "detail": "You are not in a playgroup yet"}}),
            "{path}"
        );
    }
    app.post("/api/players", json!({"name": "Nope"}))
        .await
        .assert_json(403);

    // Being added back restores access and the player they had.
    app.log_in(&admin).await;
    app.post(
        &format!("/api/admin/playgroups/{PLAYGROUP}/members"),
        json!({"user_id": member.id}),
    )
    .await
    .assert_json(201);
    app.log_in(&member).await;
    assert_eq!(
        app.get("/api/session").await.assert_json(200)["data"]["playgroup_id"],
        PLAYGROUP
    );
    app.get("/api/players").await.assert_json(200);
}

#[tokio::test]
async fn switching_persists_and_scopes_players_decks_games_and_stats() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    let main_player = app.player("Main only").await;
    let deck = app
        .deck(main_player.id, "Main deck", "Krenko, Mob Boss")
        .await;
    let other = app.player("Other").await;
    let game = app
        .simple_game("2026-01-01T20:00:00Z", main_player.id, other.id, None)
        .await;
    let second = app.playgroup("Thursday crew", None, &member).await;

    let body = app
        .patch("/api/session/playgroup", json!({"playgroup_id": second}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["playgroup_id"], second);
    assert_eq!(
        app.reload(&member).await.unwrap().playgroup_id,
        Some(second)
    );
    assert_eq!(
        app.get("/api/session").await.assert_json(200)["data"]["playgroup_id"],
        second
    );

    assert_eq!(
        app.get("/api/players").await.assert_json(200)["data"],
        json!([])
    );
    assert_eq!(
        app.get("/api/decks").await.assert_json(200)["data"],
        json!([])
    );
    assert_eq!(
        app.get("/api/games").await.assert_json(200)["data"],
        json!([])
    );
    app.get(&format!("/api/players/{}", main_player.id))
        .await
        .assert_json(404);
    app.get(&format!("/api/decks/{}", deck.id))
        .await
        .assert_json(404);
    app.get(&format!("/api/games/{}", game.id))
        .await
        .assert_json(404);
    assert_eq!(
        app.get("/api/stats/overview").await.assert_json(200)["data"]["games_count"],
        0
    );

    // Players created now belong to the new playgroup and can share a name with the other one.
    let created = app
        .post("/api/players", json!({"name": "Main only"}))
        .await
        .assert_json(201);
    assert_ne!(created["data"]["id"], main_player.id);
    let players = app.get("/api/players").await.assert_json(200);
    assert_eq!(players["data"].as_array().unwrap().len(), 1);

    // Switching back shows the original data again.
    app.patch("/api/session/playgroup", json!({"playgroup_id": PLAYGROUP}))
        .await
        .assert_json(200);
    let players = app.get("/api/players").await.assert_json(200);
    assert_eq!(players["data"].as_array().unwrap().len(), 2);
    app.get(&format!("/api/games/{}", game.id))
        .await
        .assert_json(200);
}

#[tokio::test]
async fn switching_to_an_unknown_playgroup_is_a_validation_error() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    assert_eq!(
        app.patch("/api/session/playgroup", json!({"playgroup_id": 999}))
            .await
            .assert_json(422),
        json!({"errors": {"playgroup_id": ["does not exist"]}})
    );
    app.patch("/api/session/playgroup", json!({}))
        .await
        .assert_json(422);
    assert_eq!(
        app.reload(&member).await.unwrap().playgroup_id,
        Some(PLAYGROUP)
    );
}

#[tokio::test]
async fn admins_create_rename_and_delete_playgroups() {
    let (app, admin) = admin_app().await;

    let body = app
        .post(
            "/api/admin/playgroups",
            json!({"name": "  Thursday crew ", "discord_guild_id": "4242"}),
        )
        .await
        .assert_json(201);
    let id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"], "Thursday crew");
    assert_eq!(body["data"]["discord_guild_id"], "4242");
    assert_eq!(body["data"]["invite_enabled"], false);
    // The creator is the first member; a password account gets no player automatically.
    assert_eq!(
        body["data"]["members"],
        json!([{"id": admin.id, "username": admin.username, "display_name": admin.display_name, "role": "admin"}])
    );
    assert_eq!(
        ids(&app.get("/api/playgroups").await.assert_json(200)),
        [PLAYGROUP, id]
    );
    let players: i64 = sqlx::query_scalar("SELECT count(*) FROM players WHERE playgroup_id = ?")
        .bind(id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(players, 0);

    assert_eq!(
        app.post("/api/admin/playgroups", json!({"name": "thursday CREW"}))
            .await
            .assert_json(422),
        json!({"errors": {"name": ["has already been taken"]}})
    );
    assert_eq!(
        app.post(
            "/api/admin/playgroups",
            json!({"name": "Friday", "discord_guild_id": "4242"})
        )
        .await
        .assert_json(422),
        json!({"errors": {"discord_guild_id": ["has already been taken"]}})
    );
    assert_eq!(
        app.post(
            "/api/admin/playgroups",
            json!({"name": "Friday", "discord_guild_id": "not-a-guild"})
        )
        .await
        .assert_json(422),
        json!({"errors": {"discord_guild_id": ["must be a Discord guild id (digits only)"]}})
    );
    app.post("/api/admin/playgroups", json!({"name": ""}))
        .await
        .assert_json(422);

    let body = app
        .patch(
            &format!("/api/admin/playgroups/{id}"),
            json!({"name": "Thursday night", "discord_guild_id": ""}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["name"], "Thursday night");
    assert_eq!(body["data"]["discord_guild_id"], Value::Null);
    // A partial update keeps the other field.
    let body = app
        .patch(
            &format!("/api/admin/playgroups/{id}"),
            json!({"discord_guild_id": "99"}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["name"], "Thursday night");
    assert_eq!(body["data"]["discord_guild_id"], "99");
    app.patch("/api/admin/playgroups/999", json!({"name": "Nope"}))
        .await
        .assert_json(404);

    assert_eq!(
        app.delete(&format!("/api/admin/playgroups/{id}"))
            .await
            .status
            .as_u16(),
        204
    );
    assert!(app.state.playgroups.get(id).await.unwrap().is_none());
    assert_eq!(
        ids(&app.get("/api/playgroups").await.assert_json(200)),
        [PLAYGROUP]
    );
    app.delete(&format!("/api/admin/playgroups/{id}"))
        .await
        .assert_json(404);
}

#[tokio::test]
async fn admins_see_every_playgroup_and_manage_memberships() {
    let (app, admin) = admin_app().await;
    let member = app.unique_member().await;
    app.settings(json!({"registration_enabled": true})).await;
    let outsider = app
        .state
        .accounts
        .sign_in_with_discord(
            &DiscordClaims {
                sub: "200000000000000030".into(),
                preferred_username: Some("Outsider".into()),
                picture: None,
            },
            None,
        )
        .await
        .unwrap();
    let crew = app.playgroup("Thursday crew", None, &member).await;

    // The admin is not a member of the crew, but the admin list shows it with its members.
    assert_eq!(
        ids(&app.get("/api/playgroups").await.assert_json(200)),
        [PLAYGROUP]
    );
    let listed = app.get("/api/admin/playgroups").await.assert_json(200);
    assert_eq!(ids(&listed), [PLAYGROUP, crew]);
    assert_eq!(
        listed["data"][1]["members"],
        json!([{"id": member.id, "username": member.username, "display_name": member.display_name, "role": "member"}])
    );
    assert_eq!(listed["data"][0]["members"].as_array().unwrap().len(), 3);
    app.patch("/api/session/playgroup", json!({"playgroup_id": crew}))
        .await
        .assert_json(422);

    // Adding the admin and another account.
    let body = app
        .post(
            &format!("/api/admin/playgroups/{crew}/members"),
            json!({"user_id": admin.id}),
        )
        .await
        .assert_json(201);
    // Members are listed by username (admin fixtures sort before member fixtures).
    let members: Vec<i64> = body["data"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect();
    assert_eq!(members, [admin.id, member.id]);
    app.post(
        &format!("/api/admin/playgroups/{crew}/members"),
        json!({"user_id": outsider.id}),
    )
    .await
    .assert_json(201);
    // Adding again is a no-op.
    let body = app
        .post(
            &format!("/api/admin/playgroups/{crew}/members"),
            json!({"user_id": outsider.id}),
        )
        .await
        .assert_json(201);
    assert_eq!(body["data"]["members"].as_array().unwrap().len(), 3);
    assert_eq!(
        app.post(
            &format!("/api/admin/playgroups/{crew}/members"),
            json!({"user_id": 999}),
        )
        .await
        .assert_json(422),
        json!({"errors": {"user_id": ["does not exist"]}})
    );
    assert_eq!(
        app.post(&format!("/api/admin/playgroups/{crew}/members"), json!({}))
            .await
            .assert_json(422),
        json!({"errors": {"user_id": ["can't be blank"]}})
    );
    app.post(
        "/api/admin/playgroups/999/members",
        json!({"user_id": admin.id}),
    )
    .await
    .assert_json(404);

    // Now the admin can switch to it. The Discord member got a player there, keyed by their
    // Discord identity; the password accounts did not.
    app.patch("/api/session/playgroup", json!({"playgroup_id": crew}))
        .await
        .assert_json(200);
    let players = app.get("/api/players").await.assert_json(200);
    let linked: Vec<(Option<i64>, &str)> = players["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row["user_id"].as_i64(), row["name"].as_str().unwrap()))
        .collect();
    assert_eq!(linked, [(Some(outsider.id), "Outsider")]);

    // Removing a member hides the playgroup from them and moves them back to another one,
    // but keeps their player.
    app.log_in(&outsider).await;
    app.patch("/api/session/playgroup", json!({"playgroup_id": crew}))
        .await
        .assert_json(200);
    app.log_in(&admin).await;
    assert_eq!(
        app.delete(&format!(
            "/api/admin/playgroups/{crew}/members/{}",
            outsider.id
        ))
        .await
        .status
        .as_u16(),
        204
    );
    app.delete(&format!(
        "/api/admin/playgroups/{crew}/members/{}",
        outsider.id
    ))
    .await
    .assert_json(404);
    assert_eq!(
        app.reload(&outsider).await.unwrap().playgroup_id,
        Some(PLAYGROUP)
    );
    app.log_in(&outsider).await;
    assert_eq!(
        ids(&app.get("/api/playgroups").await.assert_json(200)),
        [PLAYGROUP]
    );
    let kept: i64 =
        sqlx::query_scalar("SELECT count(*) FROM players WHERE playgroup_id = ? AND user_id = ?")
            .bind(crew)
            .bind(outsider.id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(kept, 1);
}

#[tokio::test]
async fn deleting_refuses_the_last_or_a_populated_playgroup_and_resets_viewers() {
    let (app, admin) = admin_app().await;
    assert_eq!(
        app.delete(&format!("/api/admin/playgroups/{PLAYGROUP}"))
            .await
            .assert_json(422),
        json!({"errors": {"id": ["is the last playgroup"]}})
    );

    let second = app.playgroup("Thursday crew", None, &admin).await;
    app.patch("/api/session/playgroup", json!({"playgroup_id": second}))
        .await
        .assert_json(200);
    let someone = app
        .post("/api/players", json!({"name": "Someone"}))
        .await
        .assert_json(201)["data"]["id"]
        .as_i64()
        .unwrap();
    let deck = app
        .post(
            "/api/decks",
            json!({"player_id": someone, "name": "Goblins", "commander_name": "Krenko, Mob Boss"}),
        )
        .await
        .assert_json(201)["data"]["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        app.delete(&format!("/api/admin/playgroups/{second}"))
            .await
            .assert_json(422),
        json!({"errors": {"decks": ["are still associated with this entry"]}})
    );
    assert_eq!(
        app.delete(&format!("/api/decks/{deck}"))
            .await
            .status
            .as_u16(),
        204
    );
    let mine = app
        .post("/api/players", json!({"name": "Me"}))
        .await
        .assert_json(201)["data"]["id"]
        .as_i64()
        .unwrap();
    app.post(
        "/api/games",
        json!({
            "played_at": "2026-01-01T20:00:00Z",
            "seats": [
                {"player_id": mine, "seat": 1, "result": "win"},
                {"player_id": someone, "seat": 2, "result": "loss"}
            ]
        }),
    )
    .await
    .assert_json(201);
    assert_eq!(
        app.delete(&format!("/api/admin/playgroups/{second}"))
            .await
            .assert_json(422),
        json!({"errors": {"games": ["are still associated with this entry"]}})
    );
    // The default playgroup is no longer the last one, but it still has games.
    let regular = app.player("Opponent").await;
    app.simple_game(
        "2026-01-02T20:00:00Z",
        regular.id,
        app.player("Regular").await.id,
        None,
    )
    .await;
    assert_eq!(
        app.delete(&format!("/api/admin/playgroups/{PLAYGROUP}"))
            .await
            .assert_json(422)["errors"]["games"],
        json!(["are still associated with this entry"])
    );

    // A playgroup with only a roster (players with no games or decks) can go, players and
    // memberships included.
    let empty = app.playgroup("Empty", None, &admin).await;
    app.patch("/api/session/playgroup", json!({"playgroup_id": empty}))
        .await
        .assert_json(200);
    app.post("/api/players", json!({"name": "Roster only"}))
        .await
        .assert_json(201);
    assert_eq!(
        app.delete(&format!("/api/admin/playgroups/{empty}"))
            .await
            .status
            .as_u16(),
        204
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM players WHERE playgroup_id = ?")
        .bind(empty)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
    // Viewers of a deleted playgroup fall back to another of theirs.
    assert_eq!(
        app.reload(&admin).await.unwrap().playgroup_id,
        Some(PLAYGROUP)
    );
}

#[tokio::test]
async fn playgroup_administration_needs_an_admin_with_recent_sudo() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    app.post("/api/admin/playgroups", json!({"name": "Nope"}))
        .await
        .assert_json(403);

    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    app.expire_sudo(11 * 60).await;
    assert_eq!(
        app.post("/api/admin/playgroups", json!({"name": "Nope"}))
            .await
            .assert_json(403),
        json!({"errors": {"code": "sudo_required", "detail": "Reauthentication required"}})
    );
    assert_eq!(app.state.playgroups.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn discord_guilds_route_to_their_playgroup_or_the_default() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    let second = app.playgroup("Thursday crew", Some("4242"), &member).await;
    assert_eq!(
        app.state
            .playgroups
            .for_discord_guild("4242")
            .await
            .unwrap(),
        second
    );
    assert_eq!(
        app.state.playgroups.for_discord_guild("1").await.unwrap(),
        PLAYGROUP
    );
}
