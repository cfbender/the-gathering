//! API errors, rendered the way `TheGatheringWeb.API.FallbackController` renders them.

use std::collections::BTreeMap;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

/// Field errors as `TheGatheringWeb.ChangesetJSON` renders them: `{"field": ["message"]}`,
/// with nested rows (`cast_assoc`) as `{"seats": [{}, {"seat": ["message"]}]}`.
///
/// Messages for one field are newest first, matching `Ecto.Changeset.traverse_errors/2`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Errors {
    fields: BTreeMap<String, Vec<String>>,
    nested: BTreeMap<String, Vec<Errors>>,
}

impl Errors {
    /// No errors.
    pub fn new() -> Self {
        Self::default()
    }

    /// One error.
    pub fn single(field: &str, message: impl Into<String>) -> Self {
        let mut errors = Self::new();
        errors.add(field, message);
        errors
    }

    /// Adds `message` to `field` (it renders before earlier messages for the same field).
    pub fn add(&mut self, field: &str, message: impl Into<String>) {
        self.fields.entry(field.to_owned()).or_default().insert(0, message.into());
    }

    /// Sets the per-row errors of a nested list.
    pub fn set_nested(&mut self, field: &str, rows: Vec<Errors>) {
        if rows.iter().any(|row| !row.is_empty()) {
            self.nested.insert(field.to_owned(), rows);
        }
    }

    /// Whether there are no errors at all.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.nested.values().all(|rows| rows.iter().all(Errors::is_empty))
    }

    /// Whether `field` has an error.
    pub fn has(&self, field: &str) -> bool {
        self.fields.contains_key(field)
    }

    /// Messages for `field`.
    pub fn messages(&self, field: &str) -> &[String] {
        self.fields.get(field).map_or(&[], Vec::as_slice)
    }

    /// Per-row errors of a nested list (empty when no row has errors).
    pub fn nested(&self, field: &str) -> &[Errors] {
        self.nested.get(field).map_or(&[], Vec::as_slice)
    }

    /// `Ok(())` when empty.
    pub fn into_result(self) -> Result<(), Errors> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }

    /// The JSON object.
    pub fn to_json(&self) -> Value {
        let mut object = serde_json::Map::new();
        for (field, messages) in &self.fields {
            object.insert(field.clone(), json!(messages));
        }
        for (field, rows) in &self.nested {
            let rows: Vec<Value> = rows.iter().map(Errors::to_json).collect();
            // `Ecto.Changeset.traverse_errors/2` replaces a list's own messages with its
            // per-row errors, so row `i` of the JSON is always params row `i` (the SPA numbers
            // rows by index).
            object.insert(field.clone(), Value::Array(rows));
        }
        Value::Object(object)
    }
}

/// Every error an API handler can return.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// 422 with field errors.
    #[error("validation failed")]
    Validation(Errors),
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

impl From<Errors> for ApiError {
    fn from(errors: Errors) -> Self {
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

/// `Plug.Conn.Status.reason_phrase/1`.
fn detail(status: StatusCode) -> Value {
    json!({ "errors": { "detail": status.canonical_reason().unwrap_or("Error") } })
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            Self::Validation(errors) => {
                (StatusCode::UNPROCESSABLE_ENTITY, json!({ "errors": errors.to_json() }))
            }
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
                (StatusCode::INTERNAL_SERVER_ERROR, detail(StatusCode::INTERNAL_SERVER_ERROR))
            }
        };
        (status, Json(body)).into_response()
    }
}

/// Handler result.
pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_newest_message_first_and_nested_rows() {
        let mut errors = Errors::new();
        errors.add("seats", "must contain between 2 and 10 players");
        errors.add("seats", "cannot contain the same player twice");
        assert_eq!(
            errors.to_json(),
            json!({ "seats": ["cannot contain the same player twice", "must contain between 2 and 10 players"] })
        );

        let mut nested = Errors::new();
        nested.set_nested("seats", vec![Errors::new(), Errors::single("seat", "has already been taken")]);
        assert_eq!(nested.to_json(), json!({ "seats": [{}, { "seat": ["has already been taken"] }] }));

        // Like Ecto, per-row errors replace the list's own messages.
        nested.add("seats", "must contain between 2 and 10 players");
        assert_eq!(nested.to_json(), json!({ "seats": [{}, { "seat": ["has already been taken"] }] }));
    }
}
