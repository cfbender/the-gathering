//! Cookie-session authentication (`TheGatheringWeb.UserAuth`) and API-key authentication
//! (`TheGatheringWeb.ApiKeyAuth`).

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use eetf::Term;
use time::Duration;

use crate::accounts::{self, User};
use crate::crypto;
use crate::db::UtcDateTime;
use crate::error::ApiError;
use crate::state::AppState;

use super::session::Session;

const SESSION_REISSUE_AGE_DAYS: i64 = 7;

/// The signed-in user, if any (`conn.assigns.current_scope.user`).
#[derive(Clone, Debug, Default)]
pub struct CurrentUser(pub Option<User>);

/// Creates a tracked session token and signs `user` in (`UserAuth.log_in_user/3`).
pub async fn log_in_user(
    state: &AppState,
    session: &Session,
    current: Option<&User>,
    user: &User,
) -> Result<(), ApiError> {
    create_or_extend_session(state, session, current, user).await?;
    session.delete("user_return_to");
    Ok(())
}

async fn create_or_extend_session(
    state: &AppState,
    session: &Session,
    current: Option<&User>,
    user: &User,
) -> Result<(), ApiError> {
    let token = state.accounts.generate_user_session_token(user).await?;
    // Renewing for the same user keeps the session's other values.
    if current.map(|current| current.id) != Some(user.id) {
        session.delete_csrf_token();
        session.clear();
    }
    session.put_bytes("user_token", &token);
    session.put_bytes("live_socket_id", user_session_topic(&token).as_bytes());
    Ok(())
}

/// `users_sessions:<token>`, the topic sockets for this session subscribe to.
pub fn user_session_topic(token: &[u8]) -> String {
    format!("users_sessions:{}", crypto::url_encode64(token))
}

/// Deletes the session token and clears the session (`UserAuth.log_out_user/1`).
pub async fn log_out_user(state: &AppState, session: &Session) -> Result<(), ApiError> {
    if let Some(token) = session.get_bytes("user_token") {
        state.accounts.delete_user_session_token(&token).await?;
        state.disconnect_session(&token);
    }
    session.delete_csrf_token();
    session.clear();
    Ok(())
}

/// Adds the fresh CSRF token the SPA reads after signing in or out.
pub fn put_fresh_csrf_token(session: &Session, response: &mut Response) {
    if let Ok(value) = HeaderValue::from_str(&session.csrf_token()) {
        response.headers_mut().insert("x-csrf-token", value);
    }
}

/// `fetch_current_scope_for_user`: loads the user behind the session token, reissuing tokens
/// older than a week, or signs in as the development administrator when enabled.
pub async fn current_user_layer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(session) = request.extensions().get::<Session>().cloned() else {
        return next.run(request).await;
    };
    let user = match load_user(&state, &session).await {
        Ok(user) => user,
        Err(error) => return error.into_response(),
    };
    request.extensions_mut().insert(CurrentUser(user));
    next.run(request).await
}

async fn load_user(state: &AppState, session: &Session) -> Result<Option<User>, ApiError> {
    if let Some(token) = session.get_bytes("user_token")
        && let Some((user, inserted_at)) = state.accounts.get_user_by_session_token(&token).await?
    {
        // `DateTime.diff(now, inserted_at, :day) >= 7`.
        if inserted_at <= UtcDateTime::now().plus(Duration::days(-SESSION_REISSUE_AGE_DAYS)) {
            create_or_extend_session(state, session, Some(&user), &user).await?;
            state.accounts.delete_user_session_token(&token).await?;
        }
        return Ok(Some(user));
    }
    if state.config.dev_auto_login {
        let user = state.accounts.get_or_create_dev_admin().await?;
        log_in_user(state, session, None, &user).await?;
        return Ok(Some(User {
            authenticated_at: Some(UtcDateTime::now()),
            ..user
        }));
    }
    Ok(None)
}

/// `require_authenticated_user`: the API's 401 for anonymous requests. GET requests remember
/// where the visitor was going.
pub async fn require_authenticated_user(request: Request, next: Next) -> Response {
    let signed_in = request
        .extensions()
        .get::<CurrentUser>()
        .is_some_and(|current| current.0.is_some());
    if signed_in {
        return next.run(request).await;
    }
    if request.method() == Method::GET
        && let Some(session) = request.extensions().get::<Session>()
    {
        // Phoenix's `current_path/1`: the path plus any query string.
        let path = request
            .uri()
            .path_and_query()
            .map_or_else(|| request.uri().path(), |path| path.as_str());
        session.put("user_return_to", Term::Binary(path.as_bytes().into()));
    }
    ApiError::Unauthorized.into_response()
}

/// `require_admin`.
pub async fn require_admin(request: Request, next: Next) -> Response {
    let admin = request
        .extensions()
        .get::<CurrentUser>()
        .and_then(|current| current.0.as_ref())
        .is_some_and(User::is_admin);
    if admin {
        next.run(request).await
    } else {
        ApiError::Forbidden.into_response()
    }
}

/// `require_sudo_mode`: a password authentication within the last ten minutes.
pub async fn require_sudo_mode(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let sudo = state.config.dev_auto_login
        || request
            .extensions()
            .get::<CurrentUser>()
            .and_then(|current| current.0.as_ref())
            .is_some_and(|user| accounts::sudo_mode(user, 10));
    if sudo {
        next.run(request).await
    } else {
        ApiError::SudoRequired.into_response()
    }
}

/// `ApiKeyAuth`: `Authorization: Bearer tg_…` acts as the key's owner.
pub async fn api_key_layer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|token| token.trim().to_owned());
    let user = match token {
        Some(token) => match state.accounts.authenticate_api_key(&token).await {
            Ok(user) => user,
            Err(error) => return ApiError::from(error).into_response(),
        },
        None => None,
    };
    let Some(user) = user else {
        let mut response = ApiError::Unauthorized.into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(r#"Bearer realm="the-gathering""#),
        );
        return response;
    };
    request.extensions_mut().insert(CurrentUser(Some(user)));
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

/// Extracts the optional current user.
pub struct MaybeUser(pub Option<User>);

impl<S: Send + Sync> FromRequestParts<S> for MaybeUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self(
            parts
                .extensions
                .get::<CurrentUser>()
                .and_then(|current| current.0.clone()),
        )))
    }
}

/// Extracts the signed-in user or rejects with 401.
pub struct AuthUser(pub User);

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<CurrentUser>()
                .and_then(|current| current.0.clone())
                .map(Self)
                .ok_or(ApiError::Unauthorized),
        )
    }
}
