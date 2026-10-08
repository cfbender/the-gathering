//! Static files from `priv/static`.
//!
//! Vite output under `/assets/react` has hashed names, so it is cached forever and carries
//! COEP so the card recognizer's workers can start inside the cross-origin-isolated table.
//! Only files are served (never directory indexes), and only successful responses get the
//! year-long cache header, so a 404 for an asset that is still deploying is not cached.
//!
//! A missing file under `/assets/react` is a 404 rather than falling through to the SPA
//! catch-all, so a stale tab loading an old chunk after a deploy gets an error instead of the
//! HTML shell with a 200.

use axum::Router;
use axum::http::{HeaderName, HeaderValue, Response, header};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::state::AppState;

fn on_success<B>(value: &'static str) -> impl Fn(&Response<B>) -> Option<HeaderValue> + Clone {
    move |response: &Response<B>| {
        response
            .status()
            .is_success()
            .then(|| HeaderValue::from_static(value))
    }
}

fn dir(path: std::path::PathBuf) -> ServeDir {
    ServeDir::new(path)
        .precompressed_gzip()
        .append_index_html_on_directories(false)
}

/// Routes for the static paths (`assets fonts images favicon.ico robots.txt`).
pub fn router(state: &AppState) -> Router<AppState> {
    let root = state.config.static_dir();
    let react = Router::new()
        .nest_service("/assets/react", dir(root.join("assets/react")))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            on_success("public, max-age=31536000, immutable"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-embedder-policy"),
            on_success("require-corp"),
        ));
    Router::new()
        .merge(react)
        .nest_service("/assets", dir(root.join("assets")))
        .nest_service("/fonts", dir(root.join("fonts")))
        .nest_service("/images", dir(root.join("images")))
        .route_service("/favicon.ico", ServeFile::new(root.join("favicon.ico")))
        .route_service("/robots.txt", ServeFile::new(root.join("robots.txt")))
}
