//! Ported from `test/the_gathering_web/controllers/api/csv_import_controller_test.exs`,
//! `mythic_track_import_controller_test.exs`, `sheet_import_controller_test.exs`, and
//! `portable_import_controller_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use serde_json::{Value, json};
use support::TestApp;

async fn admin_app() -> (TestApp, the_gathering::accounts::User) {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    (app, admin)
}

// csv_import_controller_test.exs

const CSV: &str = "game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
game-1,2026-09-18,Alice,Birds,Kangee,1,win,,60,8,
game-1,2026-09-18,Bob,Goblins,Krenko,2,loss,,60,8,
";

#[tokio::test]
async fn admin_can_preview_and_commit_a_csv_import() {
    let (app, admin) = admin_app().await;
    let preview = app
        .post("/api/imports/csv/preview", json!({"csv": CSV}))
        .await
        .assert_json(200);
    assert_eq!(preview["data"]["valid"], true);
    assert_eq!(preview["data"]["players"]["create"], json!(["Alice", "Bob"]));

    let result = app
        .post("/api/imports/csv", json!({"csv": CSV}))
        .await
        .assert_json(200);
    assert_eq!(result["data"]["created"], 1);
    assert_eq!(result["data"]["skipped"], 0);
    let game_id = result["data"]["game_ids"][0].as_i64().unwrap();
    let game = app.state.games.get_game(game_id).await.unwrap().unwrap();
    assert_eq!(game.created_by_user_id, Some(admin.id));
}

#[tokio::test]
async fn preview_includes_partner_zero_kills_and_the_win_condition() {
    let (app, _) = admin_app().await;
    let csv = "game_id,date,player,deck,commander,partner,seat,result,kills,win_condition
paired,2026-09-18,Alice,Partners,Ardenn,Kediss,1,win,0,alternate_win_con
paired,2026-09-18,Bob,Goblins,Krenko,,2,loss,,alternate_win_con
";
    let response = app
        .post("/api/imports/csv/preview", json!({"csv": csv}))
        .await
        .assert_json(200);
    assert_eq!(response["data"]["valid"], true);
    let games = response["data"]["games"].as_array().unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0]["win_condition"], "alternate_win_con");
    let seats = games[0]["seats"].as_array().unwrap();
    assert_eq!((&seats[0]["partner"], &seats[0]["kills"]), (&json!("Kediss"), &json!(0)));
    assert_eq!((&seats[1]["partner"], &seats[1]["kills"]), (&Value::Null, &Value::Null));
}

#[tokio::test]
async fn csv_commit_requires_authentication_inside_the_ten_minute_sudo_window() {
    let (app, _) = admin_app().await;
    app.expire_sudo(602).await;
    app.post("/api/imports/csv/preview", json!({"csv": CSV}))
        .await
        .assert_json(200);
    let body = app
        .post("/api/imports/csv", json!({"csv": CSV}))
        .await
        .assert_json(403);
    assert_eq!(body["errors"]["code"], "sudo_required");
    app.expire_sudo(598).await;
    let body = app
        .post("/api/imports/csv", json!({"csv": CSV}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["created"], 1);
}

#[tokio::test]
async fn csv_member_receives_403() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    let body = app
        .post("/api/imports/csv/preview", json!({"csv": CSV}))
        .await
        .assert_json(403);
    assert_eq!(body, json!({"errors": {"detail": "Forbidden"}}));
}

#[tokio::test]
async fn sample_endpoint_downloads_a_native_template() {
    let (app, _) = admin_app().await;
    let response = app.get("/api/imports/csv/sample").await;
    assert_eq!(response.status, 200);
    assert!(response.text().contains("game_id,date,player,deck,commander"));
    assert_eq!(response.header("content-type"), Some("text/csv"));
    assert!(
        response
            .header("content-disposition")
            .unwrap()
            .contains("the-gathering-games.csv")
    );
}

#[tokio::test]
async fn csv_create_without_a_csv_is_a_bad_request() {
    let (app, _) = admin_app().await;
    app.post("/api/imports/csv/preview", json!({}))
        .await
        .assert_json(400);
    app.post("/api/imports/csv", json!({"csv": 1}))
        .await
        .assert_json(400);
}

// mythic_track_import_controller_test.exs

fn mythic_json() -> String {
    json!([{
        "id": "8f3a0a44-0000-4000-8000-00000000abcd",
        "createdOn": "2026-03-14T19:30:15",
        "gameStatus": 3,
        "totalTurns": 9,
        "gameTimeInMinutes": 55,
        "notes": null,
        "players": [
            {"player": {"name": "Alice"}, "commander": {"name": "Kangee, Sky Warden", "colors": ["W", "U"]},
             "turnOrder": 1, "isWinner": true},
            {"player": {"name": "Bob"}, "commander": {"name": "Krenko, Mob Boss", "colors": ["R"]},
             "turnOrder": 2, "isWinner": false}
        ]
    }])
    .to_string()
}

#[tokio::test]
async fn admin_can_preview_and_commit_a_mythic_track_export() {
    let (app, admin) = admin_app().await;
    let preview = app
        .post("/api/imports/mythic_track/preview", json!({"json": mythic_json()}))
        .await
        .assert_json(200);
    assert_eq!(preview["data"]["valid"], true);
    assert_eq!(preview["data"]["warnings"], json!([]));
    assert_eq!(preview["data"]["players"]["create"], json!(["Alice", "Bob"]));
    assert_eq!(preview["data"]["games"][0]["seats"][0]["color_identity"], "WU");

    let result = app
        .post("/api/imports/mythic_track", json!({"json": mythic_json()}))
        .await
        .assert_json(200);
    assert_eq!(result["data"]["created"], 1);
    assert_eq!(result["data"]["skipped"], 0);
    let game_id = result["data"]["game_ids"][0].as_i64().unwrap();
    let game = app.state.games.get_game(game_id).await.unwrap().unwrap();
    assert_eq!(game.source.as_str(), "mythic_track");
    assert_eq!(game.created_by_user_id, Some(admin.id));
}

#[tokio::test]
async fn mythic_commit_requires_authentication_inside_the_ten_minute_sudo_window() {
    let (app, _) = admin_app().await;
    app.expire_sudo(602).await;
    app.post("/api/imports/mythic_track/preview", json!({"json": mythic_json()}))
        .await
        .assert_json(200);
    let body = app
        .post("/api/imports/mythic_track", json!({"json": mythic_json()}))
        .await
        .assert_json(403);
    assert_eq!(body["errors"]["code"], "sudo_required");
    app.expire_sudo(598).await;
    let body = app
        .post("/api/imports/mythic_track", json!({"json": mythic_json()}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["created"], 1);
}

#[tokio::test]
async fn invalid_export_returns_422_with_the_preview() {
    let (app, _) = admin_app().await;
    let response = app
        .post("/api/imports/mythic_track", json!({"json": "[]"}))
        .await
        .assert_json(422);
    assert_eq!(response["data"]["valid"], false);
    let errors = response["data"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["field"], "json");
}

#[tokio::test]
async fn mythic_member_receives_403() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    let body = app
        .post("/api/imports/mythic_track/preview", json!({"json": mythic_json()}))
        .await
        .assert_json(403);
    assert_eq!(body, json!({"errors": {"detail": "Forbidden"}}));
}

// sheet_import_controller_test.exs

const SHEET: &str = "Date\tWinner\tDeck\tA\tWin Con\tOther Decks\tNotes\n3/7/25\tA\tBirds\t1\tCombat\tB (Goblins)\tNote\n";

#[tokio::test]
async fn admin_can_preview_and_commit_explicitly_selected_creates() {
    let (app, _) = admin_app().await;
    let mut params = json!({
        "text": SHEET,
        "players": {"A": "new", "B": "new"},
        "decks": {
            json!(["A", "Birds"]).to_string(): "new",
            json!(["B", "Goblins"]).to_string(): "new"
        }
    });
    let initial = app
        .post("/api/imports/sheet/preview", params.clone())
        .await
        .assert_json(200);
    let rows = initial["data"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["action"], "skip");
    let key = rows[0]["key"].as_str().unwrap().to_owned();
    params["actions"] = json!({key: "create"});
    let preview = app
        .post("/api/imports/sheet/preview", params.clone())
        .await
        .assert_json(200);
    params["revision"] = preview["data"]["revision"].clone();
    let body = app
        .post("/api/imports/sheet", params.clone())
        .await
        .assert_json(200);
    assert_eq!(body["data"]["created"], 1);
    let body = app
        .post("/api/imports/sheet", params)
        .await
        .assert_json(422);
    assert_eq!(body["errors"]["import"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn sheet_preview_and_commit_require_admin_and_commit_additionally_requires_sudo() {
    let app = TestApp::new().await;
    app.post("/api/imports/sheet/preview", json!({"text": SHEET}))
        .await
        .assert_json(401);
    let member = app.unique_member().await;
    app.log_in(&member).await;
    app.post("/api/imports/sheet/preview", json!({"text": SHEET}))
        .await
        .assert_json(403);
    app.post("/api/imports/sheet", json!({"text": SHEET}))
        .await
        .assert_json(403);
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    app.expire_sudo(601).await;
    app.post("/api/imports/sheet/preview", json!({"text": SHEET}))
        .await
        .assert_json(200);
    let body = app
        .post("/api/imports/sheet", json!({"text": SHEET}))
        .await
        .assert_json(403);
    assert_eq!(body["errors"]["code"], "sudo_required");
}

#[tokio::test]
async fn malformed_selections_return_errors_rather_than_crashing() {
    let (app, _) = admin_app().await;
    for input in [
        json!({}),
        json!({"text": 42}),
        json!({"text": SHEET, "players": []}),
        json!({"text": SHEET, "decks": {"x": []}}),
    ] {
        app.post("/api/imports/sheet/preview", input)
            .await
            .assert_json(400);
    }
    let body = app
        .post("/api/imports/sheet/preview", json!({"text": "bad"}))
        .await
        .assert_json(422);
    assert_eq!(body["errors"]["import"].as_array().unwrap().len(), 1);
}

// portable_import_controller_test.exs

#[tokio::test]
async fn download_is_versioned_private_and_can_be_previewed_and_reimported() {
    let (app, _) = admin_app().await;
    app.player("Unused player").await;
    let download = app.get("/api/exports/portable").await;
    assert_eq!(download.status, 200);
    let json = download.text();
    let data: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(data["format"], "the-gathering");
    assert_eq!(data["version"], 1);
    assert_eq!(data["players"].as_array().unwrap().len(), 1);
    assert_eq!(data["players"][0]["name"], "Unused player");
    assert_eq!(data["games"], json!([]));
    assert_eq!(download.header("cache-control"), Some("no-store"));
    assert_eq!(download.header("content-type"), Some("application/json"));
    assert!(download.header("content-disposition").unwrap().contains(".json"));

    let preview = app
        .post("/api/imports/portable/preview", json!({"json": json}))
        .await
        .assert_json(200);
    assert_eq!(
        preview,
        json!({"data": {
            "players": {"created": 0, "reused": 1},
            "decks": {"created": 0, "reused": 0},
            "games": {"created": 0, "reused": 0}
        }})
    );
    let body = app
        .post("/api/imports/portable", json!({"json": json}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["players"]["reused"], 1);
}

#[tokio::test]
async fn export_and_preview_are_admin_only_and_committing_also_requires_sudo() {
    let app = TestApp::new().await;
    app.get("/api/exports/portable").await.assert_json(401);
    let member = app.unique_member().await;
    app.log_in(&member).await;
    app.get("/api/exports/portable").await.assert_json(403);
    app.post("/api/imports/portable/preview", json!({"json": "{}"}))
        .await
        .assert_json(403);
    app.post("/api/imports/portable", json!({"json": "{}"}))
        .await
        .assert_json(403);

    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    app.expire_sudo(601).await;
    let json = app.get("/api/exports/portable").await.text();
    app.post("/api/imports/portable/preview", json!({"json": json}))
        .await
        .assert_json(200);
    let body = app
        .post("/api/imports/portable", json!({"json": json}))
        .await
        .assert_json(403);
    assert_eq!(body["errors"]["code"], "sudo_required");
}

#[tokio::test]
async fn malformed_upload_is_a_normal_api_error() {
    let (app, _) = admin_app().await;
    app.post("/api/imports/portable/preview", json!({"json": []}))
        .await
        .assert_json(400);
    let body = app
        .post("/api/imports/portable/preview", json!({"json": "not JSON"}))
        .await
        .assert_json(422);
    assert_eq!(body["errors"]["import"].as_array().unwrap().len(), 1);
}
