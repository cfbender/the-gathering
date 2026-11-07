//! Playgroups: the member's own list for the switcher and the active-playgroup switch, and
//! the administrators' CRUD, membership, and invitation management.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult};
use crate::playgroups::{Member, Playgroup, PlaygroupInput};
use crate::state::AppState;
use crate::validation::{DOES_NOT_EXIST, Validator};
use crate::web::auth::AuthUser;
use crate::web::extract::{JsonBody, PathParam};

use super::data;

fn playgroup_json(playgroup: &Playgroup) -> Value {
    json!({
        "id": playgroup.id,
        "name": playgroup.name,
        "discord_guild_id": playgroup.discord_guild_id,
        "invite_enabled": playgroup.invite_enabled,
        "inserted_at": playgroup.inserted_at,
        "updated_at": playgroup.updated_at,
    })
}

fn admin_playgroup_json(playgroup: &Playgroup, members: &[Member]) -> Value {
    let mut value = playgroup_json(playgroup);
    if let Value::Object(object) = &mut value {
        object.insert("members".into(), json!(members));
    }
    value
}

async fn fetch_playgroup(state: &AppState, id: i64) -> ApiResult<Playgroup> {
    state.playgroups.get(id).await?.ok_or(ApiError::NotFound)
}

async fn admin_playgroup_response(state: &AppState, playgroup: &Playgroup) -> ApiResult<Value> {
    let members = state.playgroups.members(playgroup.id).await?;
    Ok(admin_playgroup_json(playgroup, &members))
}

/// `GET /api/playgroups`: the playgroups the member belongs to, oldest first.
pub async fn index(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let playgroups: Vec<Value> = state
        .playgroups
        .list_for_user(user.id)
        .await?
        .iter()
        .map(playgroup_json)
        .collect();
    Ok(data(playgroups))
}

/// `GET /api/admin/playgroups`: every playgroup with its members, oldest first.
pub async fn admin_index(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let mut playgroups = Vec::new();
    for playgroup in state.playgroups.list().await? {
        playgroups.push(admin_playgroup_response(&state, &playgroup).await?);
    }
    Ok(data(playgroups))
}

/// `POST /api/admin/playgroups`: creates a playgroup with the administrator as its first
/// member.
pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(input): JsonBody<PlaygroupInput>,
) -> ApiResult<Response> {
    let playgroup = state.playgroups.create(&input, &user).await?;
    let body = admin_playgroup_response(&state, &playgroup).await?;
    Ok((StatusCode::CREATED, data(body)).into_response())
}

/// `PATCH /api/admin/playgroups/:id`.
pub async fn update(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    JsonBody(input): JsonBody<PlaygroupInput>,
) -> ApiResult<Json<Value>> {
    let playgroup = fetch_playgroup(&state, id).await?;
    let playgroup = state.playgroups.update(&playgroup, &input).await?;
    Ok(data(admin_playgroup_response(&state, &playgroup).await?))
}

/// `DELETE /api/admin/playgroups/:id`: only an empty playgroup that is not the last one.
pub async fn delete(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
) -> ApiResult<StatusCode> {
    let playgroup = fetch_playgroup(&state, id).await?;
    state.playgroups.delete(&playgroup).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/playgroups/:id/members` body.
#[derive(Debug, Deserialize)]
pub struct NewMember {
    /// The account to add.
    #[serde(default)]
    user_id: Option<i64>,
}

/// `POST /api/admin/playgroups/:id/members`: adds an account to the playgroup and renders
/// the playgroup with its members.
pub async fn members_create(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    JsonBody(input): JsonBody<NewMember>,
) -> ApiResult<Response> {
    let playgroup = fetch_playgroup(&state, id).await?;
    let mut cs = Validator::new();
    cs.required_value("user_id", input.user_id.as_ref());
    cs.finish()?;
    let user_id = input.user_id.unwrap_or_default();
    let user = state.accounts.get_user(user_id).await?.ok_or_else(|| {
        ApiError::from(crate::validation::ValidationError::single(
            "user_id",
            DOES_NOT_EXIST,
        ))
    })?;
    state.playgroups.add_member(&playgroup, &user).await?;
    let body = admin_playgroup_response(&state, &playgroup).await?;
    Ok((StatusCode::CREATED, data(body)).into_response())
}

/// `DELETE /api/admin/playgroups/:id/members/:user_id`.
pub async fn members_delete(
    State(state): State<AppState>,
    PathParam((id, user_id)): PathParam<(i64, i64)>,
) -> ApiResult<StatusCode> {
    let playgroup = fetch_playgroup(&state, id).await?;
    state.playgroups.remove_member(&playgroup, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn no_store(body: Json<Value>) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /api/admin/playgroups/:id/registration-invite`: whether an invite link is active.
pub async fn invite_show(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
) -> ApiResult<Response> {
    let playgroup = fetch_playgroup(&state, id).await?;
    Ok(no_store(data(
        json!({ "enabled": playgroup.invite_enabled }),
    )))
}

/// `POST /api/admin/playgroups/:id/registration-invite`: rotates the playgroup's reusable
/// invitation and returns the new secret, shown once.
pub async fn invite_create(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
) -> ApiResult<Response> {
    let playgroup = fetch_playgroup(&state, id).await?;
    let token = state.playgroups.rotate_invite(&playgroup).await?;
    Ok(no_store(data(json!({ "token": token }))))
}

/// `PATCH /api/session/playgroup` body.
#[derive(Debug, Deserialize)]
pub struct PlaygroupSwitch {
    /// The playgroup to view.
    #[serde(default)]
    playgroup_id: Option<i64>,
}

/// `PATCH /api/session/playgroup`: switches the member's active playgroup and renders them.
pub async fn switch(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(switch): JsonBody<PlaygroupSwitch>,
) -> ApiResult<Json<Value>> {
    let mut cs = Validator::new();
    cs.required_value("playgroup_id", switch.playgroup_id.as_ref());
    cs.finish()?;
    let user = state
        .accounts
        .set_playgroup(&user, switch.playgroup_id.unwrap_or_default())
        .await?;
    Ok(data(user.to_json()))
}
