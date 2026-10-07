//! `/api` controllers, one module per area.

pub mod accounts;
pub mod admin;
pub mod cardid;
pub mod cards;
pub mod discord;
pub mod games;
pub mod imports;
pub mod stats;
pub mod webcam;

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::config::WindowLimit;
use crate::error::ApiError;
use crate::rate_limit::{Decision, retry_after_seconds};
use crate::state::AppState;

use super::auth::CurrentUser;

/// `{"data": ...}`.
pub fn data(value: impl Into<Value>) -> axum::Json<Value> {
    axum::Json(json!({ "data": value.into() }))
}

/// `RateLimit.client_ip/1`: the peer address, or the proxy headers when trusted.
pub fn client_ip(state: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let peer_ip = peer.map_or_else(|| "127.0.0.1".to_owned(), |peer| peer.ip().to_string());
    if !state.config.rate_limits.trust_proxy_headers {
        return peer_ip;
    }
    let header = headers
        .get("x-real-ip")
        .or_else(|| headers.get_all("x-forwarded-for").iter().next_back())
        .and_then(|value| value.to_str().ok());
    header
        .and_then(|value| value.split(',').next_back())
        .map(str::trim)
        .and_then(|candidate| candidate.parse::<IpAddr>().ok())
        .map_or(peer_ip, |ip| ip.to_string())
}

fn peer(request: &Request) -> Option<SocketAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0)
}

fn user_id(request: &Request) -> Option<i64> {
    request
        .extensions()
        .get::<CurrentUser>()
        .and_then(|current| current.0.as_ref())
        .map(|user| user.id)
}

fn deny(decision: Decision) -> Option<Response> {
    match decision {
        Decision::Allow(_) => None,
        Decision::Deny(ms) => {
            Some(ApiError::TooManyRequests(retry_after_seconds(ms)).into_response())
        }
    }
}

/// Throttles password login and bootstrap registration per client address.
pub async fn rate_limit_credentials(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&state, request.headers(), peer(&request));
    let decision = state.rate_limiter.hit(
        &format!("credentials:{ip}"),
        state.config.rate_limits.credentials,
    );
    match deny(decision) {
        Some(response) => response,
        None => next.run(request).await,
    }
}

/// Password sudo: per user and address, then a global budget.
pub async fn rate_limit_sudo(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let limits = &state.config.rate_limits;
    let ip = client_ip(&state, request.headers(), peer(&request));
    let user = user_id(&request).unwrap_or_default();
    let mut decision = state
        .rate_limiter
        .hit(&format!("sudo:{user}:{ip}"), limits.sudo);
    if matches!(decision, Decision::Allow(_)) {
        decision = state.rate_limiter.hit(
            "sudo:global",
            WindowLimit {
                limit: limits.sudo_global,
                scale: limits.sudo.scale,
            },
        );
    }
    match deny(decision) {
        Some(response) => response,
        None => next.run(request).await,
    }
}

/// Requests authenticated with a personal API key, counted per owner.
pub async fn rate_limit_api_keys(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let user = user_id(&request).unwrap_or_default();
    let decision = state.rate_limiter.hit(
        &format!("api_keys:{user}"),
        state.config.rate_limits.api_keys,
    );
    match deny(decision) {
        Some(response) => response,
        None => next.run(request).await,
    }
}

/// Signed-in endpoints limited per account (`corrections`, `turn_credentials`).
pub fn check_user_limit(
    state: &AppState,
    bucket: &str,
    user_id: i64,
    limit: WindowLimit,
) -> Result<(), ApiError> {
    match state
        .rate_limiter
        .hit(&format!("{bucket}:{user_id}"), limit)
    {
        Decision::Allow(_) => Ok(()),
        Decision::Deny(ms) => Err(ApiError::TooManyRequests(retry_after_seconds(ms))),
    }
}
