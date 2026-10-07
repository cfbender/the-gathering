//! Server software updates (`AdminSoftwareUpdateController`).

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::error::{ApiError, ApiResult};
use crate::self_update::{RequestError, Status};
use crate::state::AppState;

use super::data;

fn render(status_code: StatusCode, status: &Status) -> ApiResult<Response> {
    let body = serde_json::to_value(status).map_err(|error| ApiError::Internal(error.into()))?;
    let mut response = (status_code, data(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// `GET /api/admin/software-update`: running version, update channel, configured updater,
/// and the newest build on GitHub.
pub async fn software_update_show(State(state): State<AppState>) -> ApiResult<Response> {
    render(StatusCode::OK, &state.self_update.status().await)
}

/// `POST /api/admin/software-update`: asks the configured updater to install the newest build
/// of this server's channel. The server restarts once that updater has finished, so the 202
/// only says the request was handed over.
pub async fn software_update_create(State(state): State<AppState>) -> ApiResult<Response> {
    match state.self_update.request_update().await {
        Ok(status) => render(StatusCode::ACCEPTED, &status),
        Err(RequestError::Unsupported) => Err(ApiError::BadRequest),
        Err(RequestError::UpdateInProgress) => Err(ApiError::Conflict),
        Err(RequestError::UpdaterUnavailable) => Err(ApiError::BadGateway),
    }
}
