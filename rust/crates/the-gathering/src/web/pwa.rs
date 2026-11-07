//! Progressive web app files: the web app manifest and the service worker.
//!
//! Both are public (the install prompt and the worker load before anyone signs in) and never
//! cached, so a deploy's new icons, shortcuts, or worker reach installed apps on next launch.
//! The worker only gives page loads an offline fallback (and the files that page shows);
//! everything else goes straight to the network, so it never serves stale API data or
//! interferes with the table's sockets.

use axum::Router;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue};
use axum::response::IntoResponse;
use axum::routing::get;
use serde_json::{Value, json};

use crate::state::AppState;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NO_STORE: (HeaderName, HeaderValue) = (
    CACHE_CONTROL,
    HeaderValue::from_static("no-cache, no-store, must-revalidate"),
);

fn icon(src: &str, sizes: &str, purpose: &str) -> Value {
    json!({ "src": src, "sizes": sizes, "type": "image/png", "purpose": purpose })
}

fn shortcut(name: &str, description: &str, url: &str) -> Value {
    json!({
        "name": name,
        "short_name": name,
        "description": description,
        "url": url,
        "icons": [{ "src": "/images/icon-192.png", "sizes": "192x192", "type": "image/png" }],
    })
}

/// The web app manifest.
#[must_use]
pub fn manifest_json() -> Value {
    json!({
        "name": "The Gathering",
        "short_name": "The Gathering",
        "description": "Track your playgroup's Commander games, decks, and life totals.",
        "id": "/",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "prefer_related_applications": false,
        "background_color": "#180810",
        "theme_color": "#180810",
        "categories": ["games", "entertainment"],
        "icons": [
            icon("/images/icon-192.png", "192x192", "any"),
            icon("/images/icon-512.png", "512x512", "any"),
            icon("/images/icon-maskable-192.png", "192x192", "maskable"),
            icon("/images/icon-maskable-512.png", "512x512", "maskable"),
        ],
        "shortcuts": [
            shortcut("Tabletop", "Track life totals on one device at the table.", "/tabletop"),
            shortcut("Log a game", "Record a finished game.", "/games/new"),
        ],
    })
}

/// `GET /site.webmanifest`.
async fn manifest() -> impl IntoResponse {
    (
        [
            NO_STORE,
            (
                CONTENT_TYPE,
                HeaderValue::from_static("application/manifest+json; charset=utf-8"),
            ),
        ],
        manifest_json().to_string(),
    )
}

/// The service worker source.
#[must_use]
pub fn service_worker_js() -> String {
    format!(
        r#"const CACHE_NAME = "the-gathering-pwa-v{VERSION}"
const OFFLINE_URL = "/offline.html"
const PRECACHE_URLS = [OFFLINE_URL, "/images/icon-192.png", "/images/logo.svg"]

self.addEventListener("install", (event) => {{
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      .then((cache) => cache.addAll(PRECACHE_URLS))
      .then(() => self.skipWaiting()),
  )
}})

self.addEventListener("activate", (event) => {{
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((name) => name.startsWith("the-gathering-pwa-") && name !== CACHE_NAME)
            .map((name) => caches.delete(name)),
        ),
      )
      .then(() => self.clients.claim()),
  )
}})

self.addEventListener("fetch", (event) => {{
  const request = event.request
  const url = new URL(request.url)
  if (request.method !== "GET" || url.origin !== self.location.origin) return
  if (request.mode === "navigate" && !url.pathname.startsWith("/auth/")) {{
    event.respondWith(fetch(request).catch(() => caches.match(OFFLINE_URL)))
  }} else if (PRECACHE_URLS.includes(url.pathname)) {{
    event.respondWith(fetch(request).catch(() => caches.match(url.pathname)))
  }}
}})
"#
    )
}

/// `GET /sw.js`.
async fn service_worker() -> impl IntoResponse {
    (
        [
            NO_STORE,
            (
                CONTENT_TYPE,
                HeaderValue::from_static("text/javascript; charset=utf-8"),
            ),
            (
                HeaderName::from_static("service-worker-allowed"),
                HeaderValue::from_static("/"),
            ),
        ],
        service_worker_js(),
    )
}

/// Routes for `/site.webmanifest` and `/sw.js`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/site.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
}
