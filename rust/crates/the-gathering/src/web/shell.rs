//! The single-page app shell (`AppController` and `ViteAssets`).

use std::sync::OnceLock;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::ViteMode;
use crate::state::AppState;

use super::auth::MaybeUser;
use super::session::Session;

const ENTRY: &str = "assets/react/src/main.tsx";
const PUBLIC_PATH: &str = "/assets/react/";
const PROXY_HEADER: &str = "x-the-gathering-vite-proxy";

/// HTML-escapes an attribute value (`Plug.HTML.html_escape/1`).
pub fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn dev_server_tags(origin: &str, headers: &HeaderMap) -> String {
    // Requests proxied through the Vite dev server use relative URLs so the page also works
    // when served through a tunnel on the Vite port.
    let origin = if headers.contains_key(PROXY_HEADER) { "" } else { origin };
    format!(
        r#"<script type="module">
      import RefreshRuntime from "{origin}/@react-refresh"
      RefreshRuntime.injectIntoGlobalHook(window)
      window.$RefreshReg$ = () => {{}}
      window.$RefreshSig$ = () => (type) => type
      window.__vite_plugin_react_preamble_installed__ = true
    </script>
    <script type="module" src="{origin}/@vite/client"></script>
    <script type="module" src="{origin}/{ENTRY}"></script>
"#
    )
}

fn manifest_tags(state: &AppState) -> String {
    static TAGS: OnceLock<String> = OnceLock::new();
    TAGS.get_or_init(|| {
        let path = state.config.static_dir().join("assets/react/.vite/manifest.json");
        let manifest: serde_json::Value = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let Some(entry) = manifest.get(ENTRY) else {
            tracing::error!("Vite manifest {} has no entry for {ENTRY}; run `aube run build`", path.display());
            return String::new();
        };
        let file = entry.get("file").and_then(serde_json::Value::as_str).unwrap_or_default();
        let styles: Vec<String> = entry
            .get("css")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(|css| format!(r#"<link rel="stylesheet" href="{PUBLIC_PATH}{css}" />"#))
            .collect();
        format!(r#"{}
<script type="module" src="{PUBLIC_PATH}{file}"></script>"#, styles.join("\n"))
    })
    .clone()
}

/// Serves the React app shell for every client-side route.
pub async fn index(State(state): State<AppState>, MaybeUser(user): MaybeUser, session: Session, headers: HeaderMap) -> Response {
    let appearance = user.as_ref().map_or_else(String::new, |user| {
        format!(
            r#" data-palette="{}" data-theme-style="{}""#,
            html_escape(&user.palette),
            html_escape(&user.theme_style)
        )
    });
    let manavault_meta = state.config.manavault_url.as_deref().map_or_else(String::new, |url| {
        format!(r#"<meta name="manavault-url" content="{}" />"#, html_escape(url))
    });
    let tags = match &state.config.vite {
        ViteMode::DevServer { origin } => dev_server_tags(origin, &headers),
        ViteMode::Manifest => manifest_tags(&state),
    };
    let csrf = session.csrf_token();
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en"{appearance}>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <meta name="csrf-token" content="{csrf}" />
    <meta name="application-name" content="The Gathering" />
    {manavault_meta}
    <meta name="theme-color" content="#f5e6e2" media="(prefers-color-scheme: light)" />
    <meta name="theme-color" content="#180810" media="(prefers-color-scheme: dark)" />
    <title>The Gathering</title>
    <link rel="icon" href="/favicon.ico" sizes="32x32" />
    <link rel="icon" href="/images/logo.svg" type="image/svg+xml" />
    <link rel="apple-touch-icon" href="/images/apple-touch-icon.png" />
    <script>
      (() => {{
        const key = "the-gathering:theme"
        let stored = null
        try {{ stored = localStorage.getItem(key) }} catch {{}}
        const system = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
        document.documentElement.dataset.theme = stored === "light" || stored === "dark" ? stored : system
        // Signed-in users get their saved palette and style from the server
        // (attributes on <html>); anonymous visitors fall back to this device.
        const root = document.documentElement
        if (!root.dataset.themeStyle) {{
          let style = null
          try {{ style = localStorage.getItem("the-gathering:theme-style") }} catch {{}}
          root.dataset.themeStyle = style === "classic" ? "classic" : "glass"
        }}
        if (!root.dataset.palette) {{
          let palette = null
          try {{ palette = localStorage.getItem("the-gathering:palette") }} catch {{}}
          root.dataset.palette = palette || "claret"
        }}
      }})()
    </script>
    {tags}
  </head>
  <body>
    <div id="root"></div>
  </body>
</html>
"##
    );
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-store, must-revalidate")),
        ],
        html,
    )
        .into_response()
}

/// `put_secure_browser_headers` for the browser pipeline.
pub async fn secure_browser_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        ("content-security-policy", "base-uri 'self'; frame-ancestors 'self';"),
        ("referrer-policy", "strict-origin-when-cross-origin"),
        ("x-content-type-options", "nosniff"),
        ("x-download-options", "noopen"),
        ("x-frame-options", "SAMEORIGIN"),
        ("x-permitted-cross-domain-policies", "none"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

/// `CrossOriginIsolation` for the webcam table document.
pub async fn cross_origin_isolation(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    headers.insert("cross-origin-embedder-policy", HeaderValue::from_static("require-corp"));
    response
}
