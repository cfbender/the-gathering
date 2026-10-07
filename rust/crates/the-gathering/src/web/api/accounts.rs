//! Sessions, registration, invitations, API keys, account administration, and Discord
//! OAuth (`SessionController`, `RegistrationController`, `RegistrationInviteController`,
//! `ApiKeyController`, `AdminUserController`, `AdminSettingsController`,
//! `AdminRegistrationInviteController`, `DiscordAuthController`).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use eetf::{Atom, Binary, Map, Term};
use serde_json::{Value, json};

use crate::accounts::discord::{DiscordClaims, SignInError};
use crate::accounts::{RegisterError, User};
use crate::db::UtcDateTime;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::web::auth::{AuthUser, MaybeUser, log_in_user, log_out_user, put_fresh_csrf_token};
use crate::web::params::Params;
use crate::web::session::{Session, term_string};

use super::{data, parse_id};

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
    params: Params,
) -> ApiResult<Response> {
    let attrs = params.object("user").ok_or(ApiError::BadRequest)?;
    match state.accounts.register_user(attrs).await {
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
    let hash = session.get_bytes("registration_invite_hash");
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

/// `POST /api/registration-invite`: remembers a valid invitation in the session.
pub async fn invite_create(
    State(state): State<AppState>,
    session: Session,
    params: Params,
) -> ApiResult<Response> {
    let token = params.str("token").ok_or(ApiError::BadRequest)?;
    let hash = crate::accounts::registration_invite_hash(token);
    if state
        .accounts
        .valid_registration_invite_hash(hash.as_deref())
        .await?
    {
        session.put_bytes("registration_invite_hash", &hash.unwrap_or_default());
    } else {
        session.delete("registration_invite_hash");
    }
    invite_response(&state, &session).await
}

/// `GET /api/session`.
pub async fn session_show(MaybeUser(user): MaybeUser) -> ApiResult<Json<Value>> {
    user.map(|user| user_response(&user))
        .ok_or(ApiError::Unauthorized)
}

/// `POST /api/session`: administrator password sign-in.
pub async fn session_create(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(current): MaybeUser,
    params: Params,
) -> ApiResult<Response> {
    let (Some(username), Some(password)) = (params.str("username"), params.str("password")) else {
        return Err(ApiError::Unauthorized);
    };
    match state
        .accounts
        .get_user_by_username_and_password(username, password)
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

/// `POST /api/session/sudo`: re-enter the password to unlock sensitive actions.
pub async fn session_sudo(
    State(state): State<AppState>,
    session: Session,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Response> {
    let password = params.str("password").ok_or(ApiError::Unauthorized)?;
    match state
        .accounts
        .get_user_by_username_and_password(&user.username, password)
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
    params: Params,
) -> ApiResult<Json<Value>> {
    let attrs = params.object("user").ok_or(ApiError::BadRequest)?;
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
        .update_profile(&user, attrs, allow_insecure)
        .await?;
    Ok(user_response(&user))
}

/// `PATCH /api/session/appearance`.
pub async fn update_appearance(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Json<Value>> {
    let attrs = params.object("user").ok_or(ApiError::BadRequest)?;
    let user = state.accounts.update_appearance(&user, attrs).await?;
    Ok(user_response(&user))
}

/// `PATCH /api/session/password`: signs every other session out.
pub async fn update_password(
    State(state): State<AppState>,
    session: Session,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Response> {
    if params.get("password").is_none() {
        return Err(ApiError::BadRequest);
    }
    let tokens: Vec<Vec<u8>> =
        sqlx::query_scalar!("SELECT token FROM users_tokens WHERE user_id = ?", user.id)
            .fetch_all(&state.pool)
            .await?;
    let updated = state
        .accounts
        .update_user_password(&user, &params.0)
        .await?;
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
    params: Params,
) -> ApiResult<Response> {
    let attrs = params.object("api_key").ok_or(ApiError::BadRequest)?;
    let name_only = json!({ "name": attrs.get("name").cloned().unwrap_or(Value::Null) });
    let (token, key) = state.accounts.create_api_key(user.id, &name_only).await?;
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
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state
        .accounts
        .delete_api_key(user.id, parse_id(&id)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/admin/users`.
pub async fn admin_users_index(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let users = state.accounts.list_users().await?;
    Ok(data(users.iter().map(User::to_json).collect::<Vec<_>>()))
}

async fn fetch_user(state: &AppState, id: &str) -> ApiResult<User> {
    state
        .accounts
        .get_user(parse_id(id)?)
        .await?
        .ok_or(ApiError::NotFound)
}

/// `PATCH /api/admin/users/:id`; `disabled: true/false` toggles `disabled_at`.
pub async fn admin_users_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let mut attrs = params.object("user").cloned().ok_or(ApiError::BadRequest)?;
    if let Value::Object(object) = &mut attrs {
        match object.remove("disabled") {
            Some(Value::Bool(true)) => {
                object.insert(
                    "disabled_at".into(),
                    Value::String(UtcDateTime::now().to_string()),
                );
            }
            Some(Value::Bool(false)) => {
                object.insert("disabled_at".into(), Value::Null);
            }
            Some(other) => {
                object.insert("disabled".into(), other);
            }
            None => {}
        }
    }
    let user = fetch_user(&state, &id).await?;
    let updated = state.accounts.update_user(&user, &attrs).await?;
    Ok(user_response(&updated))
}

/// `DELETE /api/admin/users/:id/sessions`.
pub async fn admin_users_revoke_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = fetch_user(&state, &id).await?;
    state.accounts.revoke_all_sessions(user.id).await?;
    Ok(user_response(&user))
}

/// `DELETE /api/admin/users/:id`.
pub async fn admin_users_delete(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = fetch_user(&state, &id).await?;
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
    params: Params,
) -> ApiResult<Json<Value>> {
    let attrs = params.object("settings").ok_or(ApiError::BadRequest)?;
    Ok(settings_json(&state.accounts.update_settings(attrs).await?))
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

const OAUTH_SESSION: &str = "discord_oauth";

fn atom(name: &str) -> Term {
    Term::Atom(Atom::from(name))
}

fn nil_or_binary(bytes: Option<&[u8]>) -> Term {
    bytes.map_or_else(|| atom("nil"), |bytes| Term::Binary(Binary::from(bytes)))
}

/// Phoenix's `redirect/2`: a 302 with a `location` header.
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

/// The parts of the stored OAuth attempt.
struct OauthAttempt {
    state: String,
    return_to: String,
    sudo_discord_id: Option<String>,
    registration_invite_hash: Option<Vec<u8>>,
}

fn map_get<'a>(map: &'a Map, key: &str) -> Option<&'a Term> {
    map.map
        .get(&atom(key))
        .or_else(|| map.map.get(&Term::Binary(Binary::from(key.as_bytes()))))
}

fn read_attempt(term: &Term) -> Option<OauthAttempt> {
    let Term::Map(map) = term else { return None };
    let Term::Map(session_params) = map_get(map, "session_params")? else {
        return None;
    };
    let state = term_string(map_get(session_params, "state")?)?;
    let nil_string =
        |term: Option<&Term>| term.and_then(term_string).filter(|value| value != "nil");
    Some(OauthAttempt {
        state,
        return_to: nil_string(map_get(map, "return_to")).unwrap_or_else(|| "/".into()),
        sudo_discord_id: nil_string(map_get(map, "sudo_discord_id")),
        registration_invite_hash: match map_get(map, "registration_invite_hash") {
            Some(Term::Binary(binary)) => Some(binary.bytes.clone()),
            _ => None,
        },
    })
}

/// `GET /auth/discord`: starts Discord OAuth (`?sudo=1` re-verifies the signed-in member).
pub async fn discord_request(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(user): MaybeUser,
    Query(params): Query<BTreeMap<String, String>>,
) -> Response {
    let Some(oauth) = &state.config.discord_oauth else {
        return login_error("discord_unavailable");
    };
    let sudo_discord_id = if params.get("sudo").map(String::as_str) == Some("1") {
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
    let invite = session.get_bytes("registration_invite_hash");
    let attempt: std::collections::HashMap<Term, Term> = [
        (
            atom("session_params"),
            Term::Map(Map::from([(
                atom("state"),
                Term::Binary(Binary::from(oauth_state.as_bytes())),
            )])),
        ),
        (
            atom("return_to"),
            Term::Binary(Binary::from(
                safe_return_to(params.get("returnTo").map(String::as_str)).as_bytes(),
            )),
        ),
        (
            atom("sudo_discord_id"),
            nil_or_binary(sudo_discord_id.as_deref().map(str::as_bytes)),
        ),
        (
            atom("registration_invite_hash"),
            nil_or_binary(invite.as_deref()),
        ),
    ]
    .into_iter()
    .collect();
    session.put(OAUTH_SESSION, Term::Map(Map::from(attempt)));
    found(&format!("{}?{query}", oauth.authorize_url))
}

async fn discord_profile(state: &AppState, code: &str) -> anyhow::Result<Value> {
    let oauth = state
        .config
        .discord_oauth
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("not configured"))?;
    let redirect_uri = format!("{}/auth/discord/callback", state.config.public_url());
    let token: Value = state
        .http
        .post(format!("{}/oauth2/token", oauth.api_base))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", oauth.client_id.as_str()),
            ("client_secret", oauth.client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("token response without access_token"))?;
    Ok(state
        .http
        .get(format!("{}/users/@me", oauth.api_base))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// `GET /auth/discord/callback`.
pub async fn discord_callback(
    State(state): State<AppState>,
    session: Session,
    MaybeUser(current): MaybeUser,
    Query(params): Query<BTreeMap<String, String>>,
) -> Response {
    let attempt = session.get(OAUTH_SESSION).as_ref().and_then(read_attempt);
    session.delete(OAUTH_SESSION);
    let Some(attempt) = attempt else {
        tracing::warn!("Discord sign-in failed: no OAuth attempt in the session");
        return login_error("discord_failed");
    };
    let state_matches = params.get("state").is_some_and(|given| {
        crate::crypto::secure_compare(given.as_bytes(), attempt.state.as_bytes())
    });
    let code = params
        .get("code")
        .filter(|_| state_matches && !params.contains_key("error"));
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
            session.delete("registration_invite_hash");
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
