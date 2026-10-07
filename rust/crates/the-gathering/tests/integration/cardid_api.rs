//! Card-recognition bundle and correction APIs.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use crate::support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::http::{HeaderMap, HeaderValue, Method};
use base64::Engine;
use serde_json::{Value, json};
use support::{TestApp, fixture};
use the_gathering::config::WindowLimit;

// ---- card id bundle controller ----

fn bundle_root(app: &TestApp) -> PathBuf {
    the_gathering::card_id::bundle_dir(&app.state.config.data_dir)
}

/// Lays out a bundle the way `python -m cardid.publish` does and points `current` at it.
fn publish(root: &Path, version: &str) {
    let dir = root.join(version);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "version": version,
        "created": "2026-09-22T00:00:00+00:00",
        "gallery": {"arts": 2, "dtype": "f16", "embed_dim": 128, "frame_penalty": 0.02, "topk": 5},
        "constants": {"scene": 640, "det_input": 256},
        "files": {}
    });
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    std::fs::write(
        dir.join("arts.json"),
        json!([{"id": "a", "name": "Forest", "set": "fin"}, {"id": "b", "name": "Island", "set": "fin"}]).to_string(),
    )
    .unwrap();
    for name in ["detector.onnx", "embed.onnx", "search.onnx"] {
        std::fs::write(dir.join(name), "onnx-bytes").unwrap();
    }
    std::fs::write(dir.join("SHA256SUMS"), "sums").unwrap();
    let current = root.join("current");
    let _ = std::fs::remove_file(&current);
    std::os::unix::fs::symlink(version, current).unwrap();
}

async fn member_app() -> TestApp {
    let app = TestApp::new().await;
    let user = app.member("member").await;
    app.log_in(&user).await;
    app
}

#[tokio::test]
async fn bundle_404_when_nothing_has_been_published() {
    let app = member_app().await;
    let body = app.get("/api/cardid/bundle").await.assert_json(404);
    assert_eq!(body, json!({"errors": {"detail": "Not Found"}}));
}

#[tokio::test]
async fn describes_the_current_bundle_and_serves_its_files_immutably() {
    let app = member_app().await;
    publish(&bundle_root(&app), "2026-09-22-full-3");
    let response = app.get("/api/cardid/bundle").await;
    assert_eq!(response.header("cache-control"), Some("private, no-cache"));
    let data = response.assert_json(200)["data"].clone();
    assert_eq!(data["version"], "2026-09-22-full-3");
    assert_eq!(data["created"], "2026-09-22T00:00:00+00:00");
    assert_eq!(data["gallery"]["arts"], 2);
    assert_eq!(data["gallery"]["topk"], 5);
    assert_eq!(data["constants"]["scene"], 640);
    let detector = data["files"]["detector.onnx"].as_str().unwrap();
    assert_eq!(
        detector,
        "/api/cardid/bundles/2026-09-22-full-3/detector.onnx"
    );

    let response = app.get(detector).await;
    assert_eq!(response.status.as_u16(), 200);
    assert_eq!(response.text(), "onnx-bytes");
    assert_eq!(
        response.header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(
        response.header("cache-control"),
        Some("private, max-age=31536000, immutable")
    );

    let arts = app
        .get(data["files"]["arts.json"].as_str().unwrap())
        .await
        .assert_json(200);
    assert_eq!(arts[0]["name"], "Forest");
    assert_eq!(arts.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn an_old_version_stays_addressable_after_current_moves_on() {
    let app = member_app().await;
    publish(&bundle_root(&app), "v1");
    publish(&bundle_root(&app), "v2");
    assert_eq!(
        app.get("/api/cardid/bundle").await.assert_json(200)["data"]["version"],
        "v2"
    );
    assert_eq!(
        app.get("/api/cardid/bundles/v1/embed.onnx")
            .await
            .status
            .as_u16(),
        200
    );
}

#[tokio::test]
async fn only_advertises_the_optional_sibling_file_when_the_manifest_includes_it() {
    let app = member_app().await;
    let root = bundle_root(&app);
    publish(&root, "v1");
    let data = app.get("/api/cardid/bundle").await.assert_json(200);
    assert!(data["data"]["files"].get("printings.json").is_none());

    let manifest_path = root.join("v1/manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["files"]["printings.json"] = json!({});
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    std::fs::write(
        root.join("v1/printings.json"),
        r#"{"a":[{"id":"sibling"}]}"#,
    )
    .unwrap();
    let data = app.get("/api/cardid/bundle").await.assert_json(200);
    let url = data["data"]["files"]["printings.json"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(url, "/api/cardid/bundles/v1/printings.json");
    let response = app.get(&url).await;
    assert_eq!(
        response.header("cache-control"),
        Some("private, max-age=31536000, immutable")
    );
    assert_eq!(response.assert_json(200), json!({"a": [{"id": "sibling"}]}));
}

#[tokio::test]
async fn refuses_files_outside_the_bundle() {
    let app = member_app().await;
    let root = bundle_root(&app);
    publish(&root, "v1");
    std::fs::write(root.join("secret.txt"), "nope").unwrap();
    for path in [
        "/api/cardid/bundles/v1/SHA256SUMS",
        "/api/cardid/bundles/current/manifest.json",
        "/api/cardid/bundles/missing/manifest.json",
        "/api/cardid/bundles/..%2F/secret.txt",
        "/api/cardid/bundles/v1/..%2Fsecret.txt",
    ] {
        assert_eq!(app.get(path).await.status.as_u16(), 404, "{path}");
    }
}

#[tokio::test]
async fn bundle_requires_authentication() {
    let app = TestApp::new().await;
    publish(&bundle_root(&app), "v1");
    app.get("/api/cardid/bundle").await.assert_json(401);
    app.get("/api/cardid/bundles/v1/detector.onnx")
        .await
        .assert_json(401);
}

// ---- card id correction controller ----

const TOKEN_LENGTH: usize = 40;

fn payload() -> Value {
    json!({
        "capture_id": uuid::Uuid::new_v4().to_string(),
        "label": uuid::Uuid::new_v4().to_string(),
        "image": format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(fixture("cardid-crop.jpg"))
        ),
        "click": [123, 456],
        "quad": [[40, 20], [290, 20], [290, 370], [40, 370]],
        "up_vote": 0.92,
        "bundle_version": "full-3",
        "top1": uuid::Uuid::new_v4().to_string(),
        "similarity": 0.7,
        "margin": 0.02
    })
}

fn with(mut base: Value, changes: &Value) -> Value {
    for (key, value) in changes.as_object().unwrap() {
        base[key] = value.clone();
    }
    base
}

async fn page(app: &TestApp, cursor: usize) -> the_gathering::card_id::corrections::Page {
    app.state.corrections.page(cursor).await.unwrap()
}

async fn admin_session(app: &TestApp) {
    let admin = app.admin("admin").await;
    app.clear_cookies();
    app.log_in(&admin).await;
}

#[tokio::test]
async fn stores_native_jpeg_and_label_metadata_retries_once_and_allows_relabelling() {
    let app = member_app().await;
    let p = payload();
    for _ in 0..2 {
        let body = app
            .post("/api/cardid/corrections", p.clone())
            .await
            .assert_json(201);
        assert_eq!(body["data"]["capture_id"], p["capture_id"]);
    }
    let first = page(&app, 0).await;
    assert_eq!(first.cursor, 1);
    assert_eq!(first.corrections.len(), 1);
    let row = &first.corrections[0];
    assert_eq!(row["label"], p["label"]);
    assert_eq!(row["click"], json!([123, 456]));
    assert_eq!(row["quad"], p["quad"]);
    assert_eq!(row["up_vote"], 0.92);
    assert!(row["split"] == "train" || row["split"] == "eval");
    assert_eq!(row["source"], "webcam-table");
    assert!(row.get("image").is_none());
    assert!(row.get("up_correct").is_none());
    let crop = app
        .state
        .corrections
        .crop_path(p["capture_id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(std::fs::read(crop).unwrap(), fixture("cardid-crop.jpg"));

    let relabelled = with(
        p.clone(),
        &json!({"label": uuid::Uuid::new_v4().to_string()}),
    );
    app.post("/api/cardid/corrections", relabelled.clone())
        .await
        .assert_json(201);
    let next = page(&app, 1).await;
    assert_eq!(next.cursor, 2);
    assert_eq!(next.corrections[0]["label"], relabelled["label"]);
    assert_eq!(next.corrections[0]["split"], row["split"]);
}

#[tokio::test]
async fn preserves_face_labels_and_top1_through_storage_and_export() {
    let app = member_app().await;
    let p = payload();
    let face = format!("{}-1", p["label"].as_str().unwrap());
    app.post(
        "/api/cardid/corrections",
        with(p.clone(), &json!({"label": face, "top1": face})),
    )
    .await
    .assert_json(201);
    admin_session(&app).await;
    let body = app.get("/api/cardid/corrections").await.assert_json(200);
    let rows = body["data"]["corrections"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["label"], face);
    assert_eq!(rows[0]["top1"], face);
    assert_eq!(rows[0]["capture_id"], p["capture_id"]);
}

#[tokio::test]
async fn stores_exact_sibling_and_revised_printing_labels_independently_of_the_ranked_art() {
    let app = member_app().await;
    let p = payload();
    let labels = [
        "a51fb64d-cc0c-400d-971f-78c28d42043b",
        "97fa5f07-46ba-408d-a861-bdb1791cc188",
        "cb9b9a9d-ae4c-4e04-bf9d-cae48f01292c",
        "6d6deae3-3ed4-47eb-bf4a-4a766ce18135",
    ];
    for label in labels {
        let payload = with(
            p.clone(),
            &json!({"label": label, "capture_id": uuid::Uuid::new_v4().to_string()}),
        );
        app.post("/api/cardid/corrections", payload)
            .await
            .assert_json(201);
    }
    let rows = page(&app, 0).await.corrections;
    assert_eq!(
        rows.iter()
            .map(|row| row["label"].as_str().unwrap())
            .collect::<Vec<_>>(),
        labels
    );
    assert!(rows.iter().all(|row| row["top1"] == p["top1"]));
}

#[tokio::test]
async fn keeps_a_drawn_outlines_manual_source_for_the_exporter() {
    let app = member_app().await;
    let p = payload();
    app.post(
        "/api/cardid/corrections",
        with(p.clone(), &json!({"quad_source": "manual"})),
    )
    .await
    .assert_json(201);
    let rows = page(&app, 0).await.corrections;
    assert_eq!(rows[0]["quad_source"], "manual");
    assert_eq!(rows[0]["quad"], p["quad"]);
}

#[tokio::test]
async fn rejects_malformed_oversized_and_traversing_payloads() {
    let app = member_app().await;
    let p = payload();
    let capture = p["capture_id"].as_str().unwrap();
    let label = p["label"].as_str().unwrap();
    let empty_jpeg = base64::engine::general_purpose::STANDARD.encode([255, 216, 255, 217]);
    for change in [
        json!({"capture_id": "../escape"}),
        json!({"capture_id": format!("{capture}-1")}),
        json!({"label": "not-a-scryfall-id"}),
        json!({"label": format!("{label}-0")}),
        json!({"label": format!("{label}-2")}),
        json!({"label": format!("{label}-01")}),
        json!({"top1": format!("{label}-1/../escape")}),
        json!({"image": "data:image/png;base64,AAAA"}),
        json!({"image": format!("data:image/jpeg;base64,{}", "A".repeat(190_004))}),
        json!({"image": "data:image/jpeg;base64,AAAA"}),
        json!({"image": format!("data:image/jpeg;base64,{empty_jpeg}")}),
        json!({"quad": [[1, 2]]}),
        json!({"quad_source": "guessed"}),
        json!({"quad_source": "manual", "quad": null}),
        json!({"click": [641, 10]}),
        json!({"similarity": "0.5"}),
        json!({"bundle_version": "a".repeat(121)}),
    ] {
        let response = app
            .post("/api/cardid/corrections", with(p.clone(), &change))
            .await;
        assert_eq!(response.status.as_u16(), 400, "{change}");
    }
    assert_eq!(page(&app, 0).await.cursor, 0);
}

#[tokio::test]
async fn anonymous_users_cannot_upload_and_another_user_cannot_overwrite_a_capture() {
    let app = TestApp::new().await;
    let p = payload();
    app.post("/api/cardid/corrections", p.clone())
        .await
        .assert_json(401);
    let owner = app.member("owner").await;
    app.log_in(&owner).await;
    app.post("/api/cardid/corrections", p.clone())
        .await
        .assert_json(201);
    let other = app.member("other").await;
    app.clear_cookies();
    app.log_in(&other).await;
    app.post("/api/cardid/corrections", p)
        .await
        .assert_json(403);
}

#[tokio::test]
async fn correction_rate_limit_is_per_user() {
    let app = TestApp::with_config(|config| {
        config.rate_limits.corrections = WindowLimit {
            limit: 1,
            scale: Duration::from_mins(1),
        };
    })
    .await;
    let user = app.member("member").await;
    app.log_in(&user).await;
    app.post("/api/cardid/corrections", payload())
        .await
        .assert_json(201);
    let denied = app.post("/api/cardid/corrections", payload()).await;
    denied.assert_json(429);
    assert!(denied.header("retry-after").is_some());
    let other = app.member("other").await;
    app.clear_cookies();
    app.log_in(&other).await;
    app.post("/api/cardid/corrections", payload())
        .await
        .assert_json(201);
}

#[tokio::test]
async fn exports_require_admin_and_cursor_pages_do_not_expose_owner_ids() {
    let app = member_app().await;
    let p = payload();
    let capture = p["capture_id"].as_str().unwrap().to_owned();
    app.post("/api/cardid/corrections", p)
        .await
        .assert_json(201);
    app.get("/api/cardid/corrections").await.assert_json(403);
    app.get(&format!("/api/cardid/corrections/{capture}/crop"))
        .await
        .assert_json(403);

    admin_session(&app).await;
    let response = app.get("/api/cardid/corrections").await;
    assert_eq!(response.header("cache-control"), Some("private, no-store"));
    let body = response.assert_json(200);
    assert_eq!(body["data"]["cursor"], 1);
    assert_eq!(body["data"]["has_more"], false);
    let rows = body["data"]["corrections"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("user_id").is_none());
    let body = app
        .get("/api/cardid/corrections?cursor=1")
        .await
        .assert_json(200);
    assert_eq!(body["data"]["corrections"], json!([]));
    app.get("/api/cardid/corrections?cursor=-1")
        .await
        .assert_json(400);
    app.get("/api/cardid/corrections?cursor[]=1")
        .await
        .assert_json(400);
    assert_eq!(
        app.get("/api/cardid/corrections/..%2Fescape/crop")
            .await
            .status
            .as_u16(),
        404
    );
    let crop = app
        .get(&format!("/api/cardid/corrections/{capture}/crop"))
        .await;
    assert_eq!(crop.status.as_u16(), 200);
    assert_eq!(crop.header("content-type"), Some("image/jpeg"));
    assert_eq!(crop.header("cache-control"), Some("private, no-store"));
    assert_eq!(crop.body.as_ref(), fixture("cardid-crop.jpg"));
}

#[tokio::test]
async fn scoped_bearer_token_is_read_only_and_revoked_when_its_administrator_is_disabled() {
    let token = "t".repeat(TOKEN_LENGTH);
    let app = TestApp::with_config(|config| {
        config.cardid_corrections_token = Some("t".repeat(TOKEN_LENGTH));
        config.cardid_corrections_admin_id = Some(1);
    })
    .await;
    let admin = app.admin("admin").await;
    assert_eq!(admin.id, 1);
    let mut bearer = HeaderMap::new();
    bearer.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );

    app.request_with(Method::GET, "/api/cardid/corrections", None, bearer.clone())
        .await
        .assert_json(200);
    app.request_with(
        Method::POST,
        "/api/cardid/corrections",
        Some(payload()),
        bearer.clone(),
    )
    .await
    .assert_json(401);
    let mut wrong = HeaderMap::new();
    wrong.insert("authorization", HeaderValue::from_static("Bearer wrong"));
    app.request_with(Method::GET, "/api/cardid/corrections", None, wrong)
        .await
        .assert_json(403);

    sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
        .bind(the_gathering::db::UtcDateTime::now())
        .bind(admin.id)
        .execute(app.pool())
        .await
        .unwrap();
    app.request_with(Method::GET, "/api/cardid/corrections", None, bearer)
        .await
        .assert_json(403);
}
