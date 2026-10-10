//! Associates user operations with their transactional row snapshots. Request bodies,
//! query strings, credentials, reads and unknown routes are never recorded.

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::audit;
use crate::error::ApiError;
use crate::state::AppState;
use crate::web::auth::CurrentUser;

/// Records known mutating API routes and the OAuth sign-in callback after CSRF checks.
pub async fn layer(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str);
    let Some(route) = route.filter(|route| {
        (*route == "/auth/discord/callback")
            || (route.starts_with("/api/") && !route.contains('*') && !request.method().is_safe())
    }) else {
        return next.run(request).await;
    };
    let actor = request
        .extensions()
        .get::<CurrentUser>()
        .and_then(|user| user.0.as_ref());
    let action = format!("{} {route}", request.method());
    // Only IDs belong in route parameters. Invalid parameters must not become a way to
    // retain arbitrary text (including secrets) in the audit history.
    let target = route
        .split('/')
        .zip(request.uri().path().split('/'))
        .map(|(part, value)| {
            if !part.starts_with('{')
                || value.parse::<i64>().is_ok()
                || uuid::Uuid::parse_str(value).is_ok()
            {
                value
            } else {
                "[invalid-id]"
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let request_id = request
        .headers()
        .get(super::request_id::HEADER)
        .and_then(|value| value.to_str().ok());
    let id = match audit::start(&state.pool, actor, &action, &target, request_id).await {
        Ok(id) => id,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let response = audit::scope(id, next.run(request)).await;
    if let Err(error) = audit::finish(&state.pool, id, response.status().as_u16()).await {
        // The mutation may already have committed. Keep its real response rather than
        // inviting a duplicate retry; NULL status means its outcome was not recorded.
        tracing::error!(
            operation_id = id,
            "could not complete audit operation: {error}"
        );
    }
    response
}
