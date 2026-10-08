//! Webcam tables: the table config (ICE servers and the socket token) and the open-room list.
//! The table's realtime connection is served by [`crate::web::channels`].

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::cloudflare_turn;
use crate::config::WebcamTableConfig;
use crate::error::ApiResult;
use crate::state::AppState;
use crate::web::auth::AuthUser;
use crate::web::channels::{self, rooms};
use crate::web::session::Session;

use super::check_user_limit;

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

/// `GET /api/webcam-table/config`: ICE servers (minting Cloudflare TURN credentials, so it is
/// rate-limited per account), SFU transport, and the socket token.
pub async fn config_show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    session: Session,
) -> ApiResult<Response> {
    check_user_limit(
        &state,
        "turn_credentials",
        user.id,
        state.config.rate_limits.turn_credentials,
    )?;
    let session_token = session.user_token().unwrap_or_default();
    let body = json!({
        "data": {
            "ice_servers": ice_servers(&state).await,
            "sfu": state.sfu.client_info(),
            "max_players": 10,
            "minimum_height": 1080,
            "socket_token": channels::socket_token(&state, &session_token),
        }
    });
    Ok(no_store(Json(body).into_response()))
}

/// Static servers from the environment first, then Cloudflare's short-lived TURN credentials.
/// A Cloudflare outage degrades to the static list rather than failing the room.
async fn ice_servers(state: &AppState) -> Vec<Value> {
    let config = &state.config.webcam_table;
    let static_servers: Vec<Value> = [stun_server(config), turn_server(config)]
        .into_iter()
        .flatten()
        .collect();
    let cloudflare = if cloudflare_turn::configured(&state.config.cloudflare_turn) {
        cloudflare_turn::ice_servers(&state.http, &state.config.cloudflare_turn)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let known: HashSet<String> = static_servers.iter().flat_map(urls).collect();
    let mut servers = static_servers;
    for mut server in cloudflare {
        let remaining: Vec<Value> = urls(&server)
            .into_iter()
            .filter(|url| !known.contains(url))
            .map(Value::String)
            .collect();
        if remaining.is_empty() {
            continue;
        }
        if let Some(object) = server.as_object_mut() {
            object.insert("urls".into(), Value::Array(remaining));
        }
        servers.push(server);
    }
    servers
}

/// `List.wrap(server.urls)`.
fn urls(server: &Value) -> Vec<String> {
    match server.get("urls") {
        Some(Value::String(url)) => vec![url.clone()],
        Some(Value::Array(urls)) => urls
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn stun_server(config: &WebcamTableConfig) -> Option<Value> {
    (!config.stun_urls.is_empty()).then(|| json!({ "urls": config.stun_urls }))
}

fn turn_server(config: &WebcamTableConfig) -> Option<Value> {
    (!config.turn_urls.is_empty()).then(|| {
        json!({
            "urls": config.turn_urls,
            "username": config.turn_username,
            "credential": config.turn_credential,
        })
    })
}

/// `GET /api/webcam-table/rooms`: open rooms with their connected seated players.
pub async fn rooms_index(State(state): State<AppState>, AuthUser(_user): AuthUser) -> Response {
    let rooms = rooms::active_rooms(&state);
    no_store(Json(json!({ "data": rooms })).into_response())
}
