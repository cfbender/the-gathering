//! Request extractors whose rejections are the API's JSON errors.
//!
//! axum's own `Json`, `Query`, and `Path` reject with plain-text bodies; these wrap them so
//! malformed input answers `{"errors": {"detail": "Bad Request"}}` (or 413/415) like every
//! other API error.

use axum::extract::{FromRequest, FromRequestParts};

use crate::error::ApiError;

/// A JSON request body (`content-type: application/json`).
#[derive(Debug, FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct JsonBody<T>(pub T);

/// Query string parameters.
#[derive(Debug, FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct QueryParams<T>(pub T);

/// Path parameters, such as a numeric id.
#[derive(Debug, FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct PathParam<T>(pub T);
