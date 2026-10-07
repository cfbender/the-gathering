//! Discord: the `/log` web handoff and the admin list of staged games.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::discord::{self, GamesSink, PendingGame, ResolveError, web_draft};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::web::auth::AuthUser;
use crate::web::extract::{JsonBody, PathParam};

use super::data;

/// `GET /api/discord/result-drafts/:id`.
pub async fn result_draft_show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    PathParam(id): PathParam<String>,
) -> ApiResult<Json<Value>> {
    let preview = web_draft::preview(&state, &id, &user)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(data(
        serde_json::to_value(preview).map_err(anyhow::Error::from)?,
    ))
}

/// `POST /api/discord/result-drafts/:id` with the finished result: 201 with the game.
pub async fn result_draft_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    PathParam(id): PathParam<String>,
    JsonBody(result): JsonBody<web_draft::DraftResult>,
) -> ApiResult<Response> {
    let game = web_draft::save(&state, &id, &user, &result).await?;
    let body = super::games::render_game(&state, &game).await?;
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

fn pending_json(pending: &PendingGame) -> Value {
    json!({
        "id": pending.id,
        "external_id": pending.external_id,
        "guild_id": pending.guild_id,
        "channel_id": pending.channel_id,
        "played_at": pending.played_at,
        "players": pending.players(),
        "inserted_at": pending.inserted_at,
        "updated_at": pending.updated_at,
    })
}

/// `GET /api/admin/discord/pending`.
pub async fn pending_index(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let pending = discord::list_pending(&state.pool).await?;
    Ok(data(pending.iter().map(pending_json).collect::<Vec<_>>()))
}

/// The staged game's winner.
#[derive(Debug, serde::Deserialize)]
pub struct PendingWinner {
    /// The winner's Discord id (a string: snowflakes exceed JavaScript's safe integers).
    winner_discord_id: String,
}

/// `PATCH /api/admin/discord/pending/:id` with `winner_discord_id`: records the winner.
pub async fn pending_update(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    JsonBody(PendingWinner { winner_discord_id }): JsonBody<PendingWinner>,
) -> ApiResult<StatusCode> {
    match discord::resolve_pending(&state.pool, id, &winner_discord_id, &GamesSink).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(ResolveError::UnknownGame | ResolveError::NoGameInChannel) => Err(ApiError::NotFound),
        Err(ResolveError::NotAPlayer | ResolveError::SinkFailed(_)) => Err(ApiError::BadRequest),
        Err(ResolveError::Database(error)) => Err(error.into()),
    }
}

/// `DELETE /api/admin/discord/pending/:id`.
pub async fn pending_delete(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
) -> ApiResult<StatusCode> {
    if discord::discard_pending(&state.pool, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
