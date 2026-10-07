//! Webcam tables: `WebcamTableConfigController`, `WebcamTableRoomController` (and its JSON
//! view), and the `/socket/websocket` upgrade (`TheGatheringWeb.UserSocket`).

use std::collections::HashSet;

use axum::Json;
use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::cloudflare_turn;
use crate::config::WebcamTableConfig;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::web::auth::AuthUser;
use crate::web::channels::{self, MAX_FRAME_SIZE, rooms};
use crate::web::session::Session;

use super::check_user_limit;

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

/// `GET /api/webcam-table/config`: ICE servers (minting Cloudflare TURN credentials, so it is
/// rate-limited per account), SFU transport, and the socket token.
pub async fn config_show(State(state): State<AppState>, AuthUser(user): AuthUser, session: Session) -> ApiResult<Response> {
    check_user_limit(&state, "turn_credentials", user.id, state.config.rate_limits.turn_credentials)?;
    let session_token = session.get_bytes("user_token").unwrap_or_default();
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
    let static_servers: Vec<Value> = [stun_server(config), turn_server(config)].into_iter().flatten().collect();
    let cloudflare = if cloudflare_turn::configured(&state.config.cloudflare_turn) {
        cloudflare_turn::ice_servers(&state.http, &state.config.cloudflare_turn).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let known: HashSet<String> = static_servers.iter().flat_map(urls).collect();
    let mut servers = static_servers;
    for mut server in cloudflare {
        let remaining: Vec<Value> = urls(&server).into_iter().filter(|url| !known.contains(url)).map(Value::String).collect();
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
        Some(Value::Array(urls)) => urls.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
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

/// The socket's connect params.
#[derive(Debug, serde::Deserialize)]
pub struct SocketParams {
    /// The encrypted socket token from the config endpoint.
    token: Option<String>,
    /// The serializer version (only `2.x` is served).
    vsn: Option<String>,
}

/// `GET /socket/websocket?token=…&vsn=2.0.0`: the Phoenix socket. An invalid or revoked token
/// is refused with 403 before upgrading, as Phoenix refuses a failed `connect/3`.
pub async fn socket(
    State(state): State<AppState>,
    Query(params): Query<SocketParams>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    if params.vsn.as_deref().is_some_and(|vsn| !vsn.starts_with("2.")) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let authenticated = match &params.token {
        Some(token) => channels::authenticate(&state, token).await,
        None => Ok(None),
    };
    let (user, session_token) = match authenticated {
        Ok(Some(authenticated)) => authenticated,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return ApiError::from(error).into_response(),
    };
    match upgrade {
        Ok(upgrade) => upgrade
            .max_frame_size(MAX_FRAME_SIZE)
            .max_message_size(MAX_FRAME_SIZE)
            .on_upgrade(move |socket| channels::run_socket(state, socket, user, session_token)),
        Err(rejection) => rejection.into_response(),
    }
}
