//! Static files from `priv/static` (`Plug.Static`).
//!
//! Vite output under `/assets/react` has hashed names, so it is cached forever and carries
//! COEP so the card recognizer's workers can start inside the cross-origin-isolated table.

use axum::Router;
use axum::http::HeaderValue;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::state::AppState;

/// Routes for the static paths (`assets fonts images favicon.ico robots.txt`).
pub fn router(state: &AppState) -> Router<AppState> {
    let root = state.config.static_dir();
    let react = Router::new()
        .nest_service(
            "/assets/react",
            ServeDir::new(root.join("assets/react")).precompressed_gzip(),
        )
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::HeaderName::from_static("cross-origin-embedder-policy"),
            HeaderValue::from_static("require-corp"),
        ));
    Router::new()
        .merge(react)
        .nest_service(
            "/assets",
            ServeDir::new(root.join("assets")).precompressed_gzip(),
        )
        .nest_service(
            "/fonts",
            ServeDir::new(root.join("fonts")).precompressed_gzip(),
        )
        .nest_service(
            "/images",
            ServeDir::new(root.join("images")).precompressed_gzip(),
        )
        .route_service("/favicon.ico", ServeFile::new(root.join("favicon.ico")))
        .route_service("/robots.txt", ServeFile::new(root.join("robots.txt")))
}
