//! API errors and the JSON bodies the frontend expects for them.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::validation::ValidationError;

/// Every error an API handler can return.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// 422 with field errors.
    #[error("validation failed: {0}")]
    Validation(ValidationError),
    /// 400.
    #[error("bad request")]
    BadRequest,
    /// 401 (not signed in).
    #[error("unauthorized")]
    Unauthorized,
    /// 403 (signed in, not allowed).
    #[error("forbidden")]
    Forbidden,
    /// 403 asking for a recent password.
    #[error("sudo required")]
    SudoRequired,
    /// 404.
    #[error("not found")]
    NotFound,
    /// 409.
    #[error("conflict")]
    Conflict,
    /// 429 with `retry-after` seconds.
    #[error("too many requests")]
    TooManyRequests(u64),
    /// 502.
    #[error("bad gateway")]
    BadGateway,
    /// 503 with a JSON body.
    #[error("service unavailable")]
    Unavailable(Value),
    /// Any status with a custom JSON body.
    #[error("custom error {0}")]
    Custom(StatusCode, Value),
    /// 500; the cause is logged, not shown.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<ValidationError> for ApiError {
    fn from(errors: ValidationError) -> Self {
        Self::Validation(errors)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

/// `{"errors": {"detail": "<reason phrase>"}}`.
fn detail(status: StatusCode) -> Value {
    json!({ "errors": { "detail": status.canonical_reason().unwrap_or("Error") } })
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            Self::Validation(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({ "errors": errors.to_json() }),
            ),
            Self::BadRequest => (StatusCode::BAD_REQUEST, detail(StatusCode::BAD_REQUEST)),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, detail(StatusCode::UNAUTHORIZED)),
            Self::Forbidden => (StatusCode::FORBIDDEN, detail(StatusCode::FORBIDDEN)),
            Self::SudoRequired => (
                StatusCode::FORBIDDEN,
                json!({ "errors": { "code": "sudo_required", "detail": "Reauthentication required" } }),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, detail(StatusCode::NOT_FOUND)),
            Self::Conflict => (StatusCode::CONFLICT, detail(StatusCode::CONFLICT)),
            Self::TooManyRequests(retry_after) => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({ "errors": { "detail": "Too Many Requests" } })),
                )
                    .into_response();
                if let Ok(value) = HeaderValue::from_str(&retry_after.max(1).to_string()) {
                    response.headers_mut().insert(header::RETRY_AFTER, value);
                }
                return response;
            }
            Self::BadGateway => (StatusCode::BAD_GATEWAY, detail(StatusCode::BAD_GATEWAY)),
            Self::Unavailable(body) => (StatusCode::SERVICE_UNAVAILABLE, body),
            Self::Custom(status, body) => (status, body),
            Self::Internal(error) => {
                tracing::error!("internal error: {error:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    detail(StatusCode::INTERNAL_SERVER_ERROR),
                )
            }
        };
        (status, Json(body)).into_response()
    }
}

/// Handler result.
pub type ApiResult<T> = Result<T, ApiError>;
