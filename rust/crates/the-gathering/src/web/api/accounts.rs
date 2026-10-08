//! Sessions, registration, invitations, API keys, account administration, and Discord
//! OAuth (`SessionController`, `RegistrationController`, `RegistrationInviteController`,
//! `ApiKeyController`, `AdminUserController`, `AdminSettingsController`,
//! `AdminRegistrationInviteController`, `DiscordAuthController`).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::accounts::discord::{DiscordClaims, SignInError};
use crate::accounts::{
    AccountUpdate, AppearanceUpdate, NewAccount, NewApiKey, PasswordUpdate, ProfileUpdate,
    RegisterError, SettingsUpdate, User,
};
use crate::db::UtcDateTime;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::web::auth::{AuthUser, MaybeUser, log_in_user, log_out_user, put_fresh_csrf_token};
use crate::web::extract::{JsonBody, PathParam, QueryParams};
use crate::web::session::{DiscordOAuthAttempt, Session};

use super::data;

fn user_response(user: &User) -> Json<Value> {
    data(user.to_json())
}

/// Signs `user` in and renders them with a fresh CSRF token header.
async fn sign_in_response(
    state: &AppState,
    session: &Session,
    current: Option<&User>,
    user: &User,
    status: StatusCode,
) -> ApiResult<Response> {
    log_in_user(state, session, current, user).await?;
    let mut response = (status, user_response(user)).into_response();
    put_fresh_csrf_token(session, &mut response);
    Ok(response)
}

/// `GET /api/health`.
pub async fn health(State(state): State<AppState>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => Json(json!({ "status": "ok" })).into_response(),
        Err(_) => ApiError::Unavailable(json!({ "status": "error", "database": "unavailable" }))
            .into_response(),
    }
}

/// `GET /api/registration`.
pub async fn registration_show(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let status = state.accounts.registration_status().await?;
    Ok(data(json!({
        "allowed": status.allowed,
        "bootstrap": status.bootstrap,
        "discord_configured": state.config.discord_oauth.is_some(),
    })))
}

/// `POST /api/users`: the bootstrap administrator.
pub async fn registration_create(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(current): MaybeUser,
    JsonBody(account): JsonBody<NewAccount>,
) -> ApiResult<Response> {
    match state.accounts.register_user(&account).await {
        Ok(user) => {
            sign_in_response(
                &state,
                &session,
                current.as_ref(),
                &user,
                StatusCode::CREATED,
            )
            .await
        }
        Err(RegisterError::Closed) => Err(ApiError::Forbidden),
        Err(RegisterError::Invalid(errors)) => Err(errors.into()),
        Err(RegisterError::Database(error)) => Err(error.into()),
    }
}

async fn invite_response(state: &AppState, session: &Session) -> ApiResult<Response> {
    let hash = session.data().registration_invite_hash;
    let valid = state
        .accounts
        .valid_registration_invite_hash(hash.as_deref())
        .await?;
    let mut response = data(json!({ "valid": valid })).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// `GET /api/registration-invite`.
pub async fn invite_show(State(state): State<AppState>, session: Session) -> ApiResult<Response> {
    invite_response(&state, &session).await
}

/// The invitation secret from an invite link.
#[derive(serde::Deserialize)]
pub struct InviteToken {
    token: String,
}

/// `POST /api/registration-invite`: remembers a valid invitation in the session.
pub async fn invite_create(
    State(state): State<AppState>,
    session: Session,
    JsonBody(InviteToken { token }): JsonBody<InviteToken>,
) -> ApiResult<Response> {
    let hash = crate::accounts::registration_invite_hash(&token);
    if state
        .accounts
        .valid_registration_invite_hash(hash.as_deref())
        .await?
    {
        session.update(|data| data.registration_invite_hash = hash);
    } else {
        session.update(|data| data.registration_invite_hash = None);
    }
    invite_response(&state, &session).await
}

/// `GET /api/session`.
pub async fn session_show(MaybeUser(user): MaybeUser) -> ApiResult<Json<Value>> {
    user.map(|user| user_response(&user))
        .ok_or(ApiError::Unauthorized)
}

/// Username and password.
#[derive(serde::Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

/// `POST /api/session`: administrator password sign-in.
pub async fn session_create(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(current): MaybeUser,
    JsonBody(credentials): JsonBody<Credentials>,
) -> ApiResult<Response> {
    match state
        .accounts
        .get_user_by_username_and_password(&credentials.username, &credentials.password)
        .await?
    {
        Some(user) => {
            sign_in_response(&state, &session, current.as_ref(), &user, StatusCode::OK).await
        }
        None => Err(ApiError::Unauthorized),
    }
}

/// `DELETE /api/session`.
pub async fn session_delete(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Response> {
    log_out_user(&state, &session).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    put_fresh_csrf_token(&session, &mut response);
    Ok(response)
}

/// The current password, re-entered.
#[derive(serde::Deserialize)]
pub struct Reauthentication {
    password: String,
}

/// `POST /api/session/sudo`: re-enter the password to unlock sensitive actions.
pub async fn session_sudo(
    State(state): State<AppState>,
    session: Session,
    AuthUser(user): AuthUser,
    JsonBody(Reauthentication { password }): JsonBody<Reauthentication>,
) -> ApiResult<Response> {
    match state
        .accounts
        .get_user_by_username_and_password(&user.username, &password)
        .await?
    {
        Some(reauthenticated) => {
            sign_in_response(
                &state,
                &session,
                Some(&user),
                &reauthenticated,
                StatusCode::OK,
            )
            .await
        }
        None => Err(ApiError::Unauthorized),
    }
}

/// `PATCH /api/session/user`.
pub async fn update_profile(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(update): JsonBody<ProfileUpdate>,
) -> ApiResult<Json<Value>> {
    let config = &state.config;
    let allow_insecure = |host: &str| {
        config.manavault_allow_insecure_urls
            || config
                .manavault_allowed_hosts
                .iter()
                .any(|allowed| allowed == host)
    };
    let user = state
        .accounts
        .update_profile(&user, &update, allow_insecure)
        .await?;
    Ok(user_response(&user))
}

/// `PATCH /api/session/appearance`.
pub async fn update_appearance(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(update): JsonBody<AppearanceUpdate>,
) -> ApiResult<Json<Value>> {
    let user = state.accounts.update_appearance(&user, &update).await?;
    Ok(user_response(&user))
}

/// `PATCH /api/session/password`: signs every other session out.
pub async fn update_password(
    State(state): State<AppState>,
    session: Session,
    AuthUser(user): AuthUser,
    JsonBody(update): JsonBody<PasswordUpdate>,
) -> ApiResult<Response> {
    let tokens: Vec<Vec<u8>> =
        sqlx::query_scalar!("SELECT token FROM users_tokens WHERE user_id = ?", user.id)
            .fetch_all(&state.pool)
            .await?;
    let updated = state.accounts.update_user_password(&user, &update).await?;
    for token in tokens {
        state.disconnect_session(&token);
    }
    // The old session token is gone, so this signs in afresh.
    sign_in_response(&state, &session, Some(&user), &updated, StatusCode::OK).await
}

/// `GET /api/session/api-keys`.
pub async fn api_keys_index(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let keys = state.accounts.list_api_keys(user.id).await?;
    Ok(data(
        keys.iter()
            .map(crate::accounts::ApiKey::to_json)
            .collect::<Vec<_>>(),
    ))
}

/// `POST /api/session/api-keys`: the only response that shows the secret.
pub async fn api_keys_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(key): JsonBody<NewApiKey>,
) -> ApiResult<Response> {
    let (token, key) = state.accounts.create_api_key(user.id, &key).await?;
    let mut body = key.to_json();
    if let Value::Object(object) = &mut body {
        object.insert("token".into(), Value::String(token));
    }
    let mut response = (StatusCode::CREATED, data(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

/// `DELETE /api/session/api-keys/:id`.
pub async fn api_keys_delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    PathParam(id): PathParam<i64>,
) -> ApiResult<StatusCode> {
    state.accounts.delete_api_key(user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/admin/users`.
pub async fn admin_users_index(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let users = state.accounts.list_users().await?;
    Ok(data(users.iter().map(User::to_json).collect::<Vec<_>>()))
}

async fn fetch_user(state: &AppState, id: i64) -> ApiResult<User> {
    state.accounts.get_user(id).await?.ok_or(ApiError::NotFound)
}

/// `PATCH /api/admin/users/:id`.
pub async fn admin_users_update(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    JsonBody(update): JsonBody<AccountUpdate>,
) -> ApiResult<Json<Value>> {
    let user = fetch_user(&state, id).await?;
    let updated = state.accounts.update_user(&user, &update).await?;
    Ok(user_response(&updated))
}

/// `DELETE /api/admin/users/:id/sessions`.
pub async fn admin_users_revoke_sessions(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
) -> ApiResult<Json<Value>> {
    let user = fetch_user(&state, id).await?;
    state.accounts.revoke_all_sessions(user.id).await?;
    Ok(user_response(&user))
}

/// `DELETE /api/admin/users/:id`.
pub async fn admin_users_delete(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    PathParam(id): PathParam<i64>,
) -> ApiResult<StatusCode> {
    let user = fetch_user(&state, id).await?;
    state.accounts.delete_user(&user, &actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn settings_json(settings: &crate::accounts::ServerSettings) -> Json<Value> {
    data(json!({
        "registration_enabled": settings.registration_enabled,
        "detailed_stats_from": settings.detailed_stats_from,
    }))
}

/// `GET /api/admin/settings`.
pub async fn admin_settings_show(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(settings_json(&state.accounts.get_settings().await?))
}

/// `PATCH /api/admin/settings`.
pub async fn admin_settings_update(
    State(state): State<AppState>,
    JsonBody(update): JsonBody<SettingsUpdate>,
) -> ApiResult<Json<Value>> {
    Ok(settings_json(
        &state.accounts.update_settings(&update).await?,
    ))
}

fn no_store(body: Json<Value>) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /api/admin/registration-invite`.
pub async fn admin_invite_show(State(state): State<AppState>) -> ApiResult<Response> {
    let enabled = state
        .accounts
        .get_settings()
        .await?
        .registration_invite_hash
        .is_some();
    Ok(no_store(data(json!({ "enabled": enabled }))))
}

/// `POST /api/admin/registration-invite`: rotates the reusable invitation.
pub async fn admin_invite_create(State(state): State<AppState>) -> ApiResult<Response> {
    let token = state.accounts.rotate_registration_invite().await?;
    Ok(no_store(data(json!({ "token": token }))))
}

/// A redirect: a 302 with a `location` header.
pub fn found(location: &str) -> Response {
    let mut response = StatusCode::FOUND.into_response();
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
}

fn login_error(error: &str) -> Response {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("error", error)
        .finish();
    found(&format!("/login?{query}"))
}

fn safe_return_to(path: Option<&str>) -> String {
    match path {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_owned(),
        _ => "/".to_owned(),
    }
}

/// `GET /auth/discord` parameters.
#[derive(serde::Deserialize)]
pub struct DiscordRequest {
    /// `1` re-verifies the signed-in member instead of signing in.
    sudo: Option<String>,
    /// Where to go afterwards.
    #[serde(rename = "returnTo")]
    return_to: Option<String>,
}

/// `GET /auth/discord`: starts Discord OAuth (`?sudo=1` re-verifies the signed-in member).
pub async fn discord_request(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(user): MaybeUser,
    QueryParams(request): QueryParams<DiscordRequest>,
) -> Response {
    let Some(oauth) = &state.config.discord_oauth else {
        return login_error("discord_unavailable");
    };
    let sudo_discord_id = if request.sudo.as_deref() == Some("1") {
        match user.and_then(|user| user.discord_id) {
            Some(discord_id) => Some(discord_id),
            None => return login_error("discord_sudo_unavailable"),
        }
    } else {
        None
    };
    let oauth_state = crate::crypto::url_encode64_unpadded(&crate::crypto::random_bytes::<32>());
    let redirect_uri = format!("{}/auth/discord/callback", state.config.public_url());
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", &oauth.client_id)
        .append_pair("prompt", "none")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "identify email")
        .append_pair("state", &oauth_state)
        .finish();
    let return_to = safe_return_to(request.return_to.as_deref());
    session.update(|data| {
        data.discord_oauth = Some(DiscordOAuthAttempt {
            state: oauth_state,
            return_to,
            sudo_discord_id,
            registration_invite_hash: data.registration_invite_hash.clone(),
        });
    });
    found(&format!("{}?{query}", oauth.authorize_url))
}

/// Why the token exchange or profile request failed. The message names the step and the
/// status or error class only: Discord's response body (and the request, which carries the
/// code and client secret) never reach the log.
#[derive(Debug, thiserror::Error)]
enum OauthError {
    #[error("Discord OAuth is not configured")]
    NotConfigured,
    #[error("{0} answered with status={1}")]
    Status(&'static str, u16),
    #[error("{0} failed: {1}")]
    Transport(&'static str, String),
    #[error("{0} returned an invalid response")]
    InvalidResponse(&'static str),
}

async fn oauth_json(
    step: &'static str,
    request: reqwest::RequestBuilder,
) -> Result<Value, OauthError> {
    let response = request
        .send()
        .await
        .map_err(|error| OauthError::Transport(step, error.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(OauthError::Status(step, status.as_u16()));
    }
    response
        .json()
        .await
        .map_err(|_| OauthError::InvalidResponse(step))
}

async fn discord_profile(state: &AppState, code: &str) -> Result<Value, OauthError> {
    let oauth = state
        .config
        .discord_oauth
        .as_ref()
        .ok_or(OauthError::NotConfigured)?;
    let redirect_uri = format!("{}/auth/discord/callback", state.config.public_url());
    let token = oauth_json(
        "token request",
        state
            .http
            .post(format!("{}/oauth2/token", oauth.api_base))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", oauth.client_id.as_str()),
                ("client_secret", oauth.client_secret.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
            ]),
    )
    .await?;
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(OauthError::InvalidResponse("token request"))?;
    oauth_json(
        "profile request",
        state
            .http
            .get(format!("{}/users/@me", oauth.api_base))
            .bearer_auth(access_token),
    )
    .await
}

/// What Discord sends back to the callback.
#[derive(serde::Deserialize)]
pub struct DiscordCallback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

/// `GET /auth/discord/callback`.
pub async fn discord_callback(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(current): MaybeUser,
    QueryParams(callback): QueryParams<DiscordCallback>,
) -> Response {
    let attempt = session.update(|data| data.discord_oauth.take());
    let Some(attempt) = attempt else {
        tracing::warn!("Discord sign-in failed: no OAuth attempt in the session");
        return login_error("discord_failed");
    };
    let state_matches = callback.state.as_ref().is_some_and(|given| {
        crate::crypto::secure_compare(given.as_bytes(), attempt.state.as_bytes())
    });
    let code = callback
        .code
        .as_deref()
        .filter(|_| state_matches && callback.error.is_none());
    let Some(code) = code else {
        tracing::warn!(
            "Discord sign-in failed: the callback carried an error or a mismatched state"
        );
        return login_error("discord_failed");
    };
    let claims = match discord_profile(&state, code)
        .await
        .map(|profile| DiscordClaims::from_discord_user(&profile))
    {
        Ok(Some(claims)) => claims,
        Ok(None) => {
            tracing::warn!("Discord sign-in failed: profile without an id");
            return login_error("discord_failed");
        }
        Err(error) => {
            tracing::warn!("Discord sign-in failed: {error}");
            return login_error("discord_failed");
        }
    };
    if let Some(expected) = &attempt.sudo_discord_id
        && expected != &claims.sub
    {
        return login_error("discord_sudo_mismatch");
    }
    match state
        .accounts
        .sign_in_with_discord(&claims, attempt.registration_invite_hash.as_deref())
        .await
    {
        Ok(user) => {
            session.update(|data| data.registration_invite_hash = None);
            let user = User {
                authenticated_at: Some(UtcDateTime::now()),
                ..user
            };
            if let Err(error) = log_in_user(&state, &session, current.as_ref(), &user).await {
                tracing::warn!("Discord sign-in failed: {error}");
                return login_error("discord_failed");
            }
            found(&attempt.return_to)
        }
        Err(SignInError::RegistrationClosed) => {
            tracing::info!(
                "Discord sign-in rejected an unknown account because registration is closed"
            );
            login_error("registration_closed")
        }
        Err(SignInError::Disabled) => login_error("account_disabled"),
        Err(other) => {
            tracing::warn!("Discord sign-in failed: {other:?}");
            login_error("discord_failed")
        }
    }
}
