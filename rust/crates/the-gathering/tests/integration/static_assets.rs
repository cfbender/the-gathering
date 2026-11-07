//! Static files and the SPA shell's Vite manifest mode.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use std::io::Write;

use axum::http::{HeaderMap, Method, StatusCode};
use serde_json::json;
use support::TestApp;
use the_gathering::config::ViteMode;

fn write(path: &std::path::Path, contents: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A `priv/` with a built React app.
fn priv_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let react = dir.path().join("static/assets/react");
    let script = b"console.log('the gathering')".repeat(20);
    write(&react.join("assets/main-abc123.js"), &script);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&script).unwrap();
    write(
        &react.join("assets/main-abc123.js.gz"),
        &gzip.finish().unwrap(),
    );
    write(&react.join("assets/main-def456.css"), b"body{}");
    write(
        &react.join(".vite/manifest.json"),
        json!({
            "assets/react/src/main.tsx": {
                "file": "assets/main-abc123.js",
                "css": ["assets/main-def456.css"],
                "isEntry": true
            }
        })
        .to_string()
        .as_bytes(),
    );
    write(&dir.path().join("static/images/logo.svg"), b"<svg/>");
    write(&dir.path().join("static/robots.txt"), b"User-agent: *");
    write(&dir.path().join("static/images/sub/index.html"), b"index");
    dir
}

async fn app(dir: &tempfile::TempDir) -> TestApp {
    let path = dir.path().to_path_buf();
    TestApp::with_config(move |config| {
        config.priv_dir = path;
        config.vite = ViteMode::Manifest;
    })
    .await
}

fn accept_gzip() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("accept-encoding", "gzip".parse().unwrap());
    headers
}

#[tokio::test]
async fn the_shell_links_the_built_entry_and_styles_from_the_manifest() {
    let dir = priv_dir();
    let app = app(&dir).await;
    let html = app.get("/").await.text();
    assert!(html.contains(
        r#"<link rel="stylesheet" href="/assets/react/assets/main-def456.css" />
<script type="module" src="/assets/react/assets/main-abc123.js"></script>"#
    ));
    assert!(!html.contains("@vite/client"));
}

#[tokio::test]
async fn vite_output_is_cached_forever_with_coep_and_served_gzipped() {
    let dir = priv_dir();
    let app = app(&dir).await;

    let plain = app.get("/assets/react/assets/main-abc123.js").await;
    assert_eq!(plain.status, StatusCode::OK);
    assert_eq!(
        plain.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(
        plain.header("cross-origin-embedder-policy"),
        Some("require-corp")
    );
    assert!(plain.text().starts_with("console.log"));

    let gzipped = app
        .request_with(
            Method::GET,
            "/assets/react/assets/main-abc123.js",
            None,
            accept_gzip(),
        )
        .await;
    assert_eq!(gzipped.status, StatusCode::OK);
    assert_eq!(gzipped.header("content-encoding"), Some("gzip"));
    assert!(gzipped.body.len() < plain.body.len());

    // A missing asset is a plain 404 that no cache keeps.
    let missing = app.get("/assets/react/assets/missing-000.js").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.header("cache-control"), None);
    assert_eq!(missing.header("cross-origin-embedder-policy"), None);
}

#[tokio::test]
async fn other_static_paths_are_served_without_the_immutable_header_or_directory_indexes() {
    let dir = priv_dir();
    let app = app(&dir).await;
    let logo = app.get("/images/logo.svg").await;
    assert_eq!(logo.status, StatusCode::OK);
    assert_eq!(logo.header("cache-control"), None);
    assert_eq!(app.get("/robots.txt").await.text(), "User-agent: *");
    assert_eq!(app.get("/images/sub/").await.status, StatusCode::NOT_FOUND);
    // Static files bypass the router: no session cookie, no request id.
    assert_eq!(logo.header("set-cookie"), None);
    assert_eq!(logo.header("x-request-id"), None);
}

#[tokio::test]
async fn the_app_is_installable_with_a_manifest_service_worker_and_offline_page() {
    // The default test config serves the repository's real `priv/static`.
    let app = TestApp::new().await;

    assert!(
        app.get("/")
            .await
            .text()
            .contains(r#"<link rel="manifest" href="/site.webmanifest" />"#)
    );

    let manifest = app.get("/site.webmanifest").await;
    assert_eq!(manifest.status, StatusCode::OK);
    assert_eq!(
        manifest.header("content-type"),
        Some("application/manifest+json; charset=utf-8")
    );
    assert_eq!(manifest.header("set-cookie"), None);
    let manifest = manifest.json();
    assert_eq!(manifest["display"], "standalone");
    assert_eq!(manifest["start_url"], "/");
    // Chrome requires a 192px and a 512px icon; every icon and shortcut must resolve.
    let icons = manifest["icons"].as_array().unwrap();
    for size in ["192x192", "512x512"] {
        for purpose in ["any", "maskable"] {
            assert!(
                icons
                    .iter()
                    .any(|icon| icon["sizes"] == size && icon["purpose"] == purpose),
                "missing {size} {purpose} icon"
            );
        }
    }
    for icon in icons {
        let src = icon["src"].as_str().unwrap();
        let response = app.get(src).await;
        assert_eq!(response.status, StatusCode::OK, "{src}");
        assert_eq!(response.header("content-type"), Some("image/png"), "{src}");
    }

    let worker = app.get("/sw.js").await;
    assert_eq!(worker.status, StatusCode::OK);
    assert_eq!(
        worker.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(worker.header("service-worker-allowed"), Some("/"));
    let source = worker.text();
    assert!(source.contains(r#"const OFFLINE_URL = "/offline.html""#));

    // Everything the worker precaches must exist, or its install fails.
    for path in ["/offline.html", "/images/icon-192.png", "/images/logo.svg"] {
        assert!(source.contains(&format!(r#""{path}""#)), "{path}");
        assert_eq!(app.get(path).await.status, StatusCode::OK, "{path}");
    }
}
