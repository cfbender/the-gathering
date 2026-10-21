//! `/api` controllers, one module per area.

pub mod accounts;
pub mod admin;
pub mod audit;
pub mod cardid;
pub mod cards;
pub mod discord;
pub mod games;
pub mod imports;
pub mod rulings;
pub mod server_logs;
pub mod stats;
pub mod webcam;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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

/// The peer address, or the proxy headers when the peer is a trusted proxy.
pub fn client_ip(state: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    // The dual-stack listener reports IPv4 peers as `::ffff:a.b.c.d`.
    let peer_ip = peer.map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |peer| {
        peer.ip().to_canonical()
    });
    if !state.config.rate_limits.trusts_proxy(peer_ip) {
        return peer_ip.to_string();
    }
    let header = headers
        .get("x-real-ip")
        .or_else(|| headers.get_all("x-forwarded-for").iter().next_back())
        .and_then(|value| value.to_str().ok());
    header
        .and_then(|value| value.split(',').next_back())
        .map(str::trim)
        .and_then(|candidate| candidate.parse::<IpAddr>().ok())
        .map_or(peer_ip, |ip| ip.to_canonical())
        .to_string()
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

/// Password login and bootstrap registration: per client address, then a global budget.
/// Login also checks a per-username bucket once the body is parsed (`session_create`).
pub async fn rate_limit_credentials(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let limits = &state.config.rate_limits;
    let ip = client_ip(&state, request.headers(), peer(&request));
    let mut decision = state
        .rate_limiter
        .hit(&format!("credentials:{ip}"), limits.credentials);
    if matches!(decision, Decision::Allow(_)) {
        decision = state.rate_limiter.hit(
            "credentials:global",
            WindowLimit {
                limit: limits.credentials_global,
                scale: limits.credentials.scale,
            },
        );
    }
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

/// Endpoints limited per account: a user id (`corrections`, `turn_credentials`) or an
/// attempted login username.
pub fn check_user_limit(
    state: &AppState,
    bucket: &str,
    user: impl std::fmt::Display,
    limit: WindowLimit,
) -> Result<(), ApiError> {
    match state.rate_limiter.hit(&format!("{bucket}:{user}"), limit) {
        Decision::Allow(_) => Ok(()),
        Decision::Deny(ms) => Err(ApiError::TooManyRequests(retry_after_seconds(ms))),
    }
}
