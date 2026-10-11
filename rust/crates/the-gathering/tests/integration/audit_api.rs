//! Persistent user actions through the real router, including authorization and snapshots.

use serde_json::{Value, json};

use crate::support::{self, TestApp};

#[tokio::test]
async fn member_changes_can_be_reconstructed_after_deletion_and_actor_removal() {
    let app = TestApp::new().await;
    let member = app.member("audited_member").await;
    let admin = app.unique_admin().await;
    app.log_in(&member).await;
    let created = app
        .post("/api/players", json!({"name":"Original player"}))
        .await
        .assert_json(201);
    let player_id = created["data"]["id"].as_i64().unwrap();
    let path = format!("/api/players/{player_id}");
    app.patch(
        &format!("{path}?secret=do-not-record"),
        json!({"name":"Revised player", "password":"never-store-this"}),
    )
    .await
    .assert_json(200);
    app.patch(&path, json!({"name":"Revised player"}))
        .await
        .assert_json(200);
    app.patch(&path, json!({"name":""})).await.assert_json(422);
    assert_eq!(app.delete(&path).await.status, 204);
    app.get("/api/admin/audit").await.assert_json(403);
    app.get("/api/admin/audit/1").await.assert_json(403);

    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_operations")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(stored, 3, "no-op and rejected writes are not retained");
    // Existing installations contain completed operations without any row changes.
    for action in [
        "TABLE update_status",
        "GET /auth/discord/callback",
        "POST /api/deck-chooser/{id}/outcomes",
    ] {
        sqlx::query(
            "INSERT INTO audit_operations (actor_id, actor_name, action, target, status, inserted_at, completed_at)
             VALUES (?, ?, ?, '/legacy', 200, '2026-10-10T18:00:00Z', '2026-10-10T18:00:00Z')",
        )
        .bind(member.id)
        .bind(&member.username)
        .bind(action)
        .execute(app.pool())
        .await
        .unwrap();
    }
    app.log_in_sudo(&admin).await;
    assert_eq!(
        app.delete(&format!("/api/admin/users/{}", member.id))
            .await
            .status,
        204
    );
    let response = app.get("/api/admin/audit?search=audited_member").await;
    assert_eq!(response.header("cache-control"), Some("no-store"));
    let history = response.assert_json(200);
    let operations = history["data"].as_array().unwrap();
    assert_eq!(operations.len(), 3);
    assert!(
        operations
            .iter()
            .all(|op| op["actor_id"] == member.id && op["actor_name"] == member.username)
    );
    let mut rebuilt = Value::Null;
    for operation in operations.iter().rev() {
        let changes = app
            .get(&format!("/api/admin/audit/{}", operation["id"]))
            .await
            .assert_json(200);
        for change in changes["data"].as_array().unwrap() {
            assert_eq!(change["entity"], "players");
            assert_eq!(change["entity_id"], player_id.to_string());
            assert_eq!(change["before"], rebuilt);
            rebuilt = change["after"].clone();
        }
    }
    assert!(rebuilt.is_null(), "delete ends with no row");
    let updated = operations.iter().find(|op| op["status"] == 200).unwrap();
    let changes = app
        .get(&format!("/api/admin/audit/{}", updated["id"]))
        .await
        .assert_json(200);
    assert_eq!(changes["data"][0]["before"]["name"], "Original player");
    assert_eq!(changes["data"][0]["after"]["name"], "Revised player");
    assert!(!history.to_string().contains("do-not-record"));
    assert!(!changes.to_string().contains("never-store-this"));

    let failed = app
        .get("/api/admin/audit?search=audited_member&outcome=failed")
        .await
        .assert_json(200);
    assert_eq!(failed["pagination"]["total"], 0);
    // A failure after a commit must still expose the committed changes.
    the_gathering::audit::finish(app.pool(), updated["id"].as_i64().unwrap(), 500)
        .await
        .unwrap();
    let failed = app
        .get("/api/admin/audit?search=audited_member&outcome=failed")
        .await
        .assert_json(200);
    assert_eq!(failed["pagination"]["total"], 1);
    assert_eq!(failed["data"][0]["status"], 500);
    assert_eq!(failed["data"][0]["change_count"], 1);
    let first = app
        .get("/api/admin/audit?search=audited_member&per_page=1")
        .await
        .assert_json(200);
    let second = app
        .get("/api/admin/audit?search=audited_member&per_page=1&page=2")
        .await
        .assert_json(200);
    assert_eq!(first["pagination"]["total"], 3);
    assert_ne!(first["data"][0]["id"], second["data"][0]["id"]);
    app.expire_sudo(601).await;
    assert_eq!(
        app.get("/api/admin/audit").await.assert_json(403)["errors"]["code"],
        "sudo_required"
    );
    app.clear_cookies();
    app.get("/api/admin/audit").await.assert_json(401);
    app.get("/api/admin/audit/1").await.assert_json(401);
}

#[tokio::test]
async fn login_settings_and_key_changes_are_attributed_without_secrets() {
    let app = TestApp::new().await;
    let admin = app.admin("audit_admin").await;
    app.post(
        "/api/session",
        json!({"username":admin.username,"password":support::PASSWORD}),
    )
    .await
    .assert_json(200);
    app.patch("/api/admin/settings", json!({"registration_enabled":true}))
        .await
        .assert_json(200);
    app.patch("/api/session/appearance", json!({"palette":"nord"}))
        .await
        .assert_json(200);
    let key_response = app
        .post("/api/session/api-keys", json!({"name":"My automation"}))
        .await
        .assert_json(201);
    let key_id = key_response["data"]["id"].as_i64().unwrap();
    assert_eq!(
        app.delete(&format!("/api/session/api-keys/{key_id}"))
            .await
            .status,
        204
    );
    let history = app.get("/api/admin/audit").await.assert_json(200);
    let mut text = history.to_string();
    for operation in history["data"].as_array().unwrap() {
        assert_eq!(operation["actor_id"], admin.id);
        assert_eq!(operation["actor_name"], admin.username);
        assert_ne!(operation["action"], "POST /api/session");
        assert_eq!(operation["change_count"], 1);
        let changes = app
            .get(&format!("/api/admin/audit/{}", operation["id"]))
            .await
            .assert_json(200);
        text.push_str(&changes.to_string());
    }
    assert!(text.contains("My automation"));
    assert!(!text.contains(support::PASSWORD));
    assert!(!text.contains("token_hash"));
    assert!(!text.contains("hashed_password"));
    assert!(!text.contains("registration_invite_hash"));
    assert_eq!(
        history["pagination"]["total"], 4,
        "reads and sign-ins do not create audit history"
    );
}

#[tokio::test]
async fn deleted_game_details_include_every_seat_and_support_pagination() {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let one = app.player("Winner").await;
    let two = app.player("Other").await;
    let game = app
        .game(
            json!({"played_at":"2026-10-10T18:00:00Z", "notes":"Keep this history", "seats":[
                {"player_id":one.id,"seat":1,"result":"win"},
                {"player_id":two.id,"seat":2,"result":"loss"}
            ]}),
            Some(admin.id),
        )
        .await;
    app.log_in_sudo(&admin).await;
    assert_eq!(
        app.delete(&format!("/api/games/{}", game.id)).await.status,
        204
    );
    let history = app.get("/api/admin/audit").await.assert_json(200);
    let id = history["data"][0]["id"].as_i64().unwrap();
    assert_eq!(history["data"][0]["change_count"], 3);
    let details = app
        .get(&format!("/api/admin/audit/{id}"))
        .await
        .assert_json(200);
    let rows = details["data"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["entity"] == "game_players")
            .count(),
        2
    );
    assert!(
        rows.iter()
            .all(|row| row["after"].is_null() && row["before"].is_object())
    );
    assert_eq!(
        rows.iter().find(|row| row["entity"] == "games").unwrap()["before"]["notes"],
        "Keep this history"
    );
    let page = app
        .get(&format!("/api/admin/audit/{id}?per_page=1&page=2"))
        .await
        .assert_json(200);
    assert_eq!(page["pagination"]["total"], 3);
    assert_eq!(page["data"][0], rows[1]);
}
