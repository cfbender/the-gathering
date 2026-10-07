//! Statistics (`StatsController`, `StatsJSON`: `{"data": stats}`).

use axum::Json;
use axum::extract::{Path, State};
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::stats;
use crate::web::params::Params;

use super::{data, parse_id};

/// `GET /api/stats/overview`.
pub async fn overview(State(state): State<AppState>, params: Params) -> ApiResult<Json<Value>> {
    Ok(data(stats::overview(&state.pool, &params.0).await?))
}

/// `GET /api/stats/players/:id`.
pub async fn player(State(state): State<AppState>, Path(id): Path<String>, params: Params) -> ApiResult<Json<Value>> {
    let stats = stats::player(&state.pool, parse_id(&id)?, &params.0).await?.ok_or(ApiError::NotFound)?;
    Ok(data(stats))
}

/// `GET /api/stats/decks/:id`.
pub async fn deck(State(state): State<AppState>, Path(id): Path<String>, params: Params) -> ApiResult<Json<Value>> {
    let stats = stats::deck(&state.pool, parse_id(&id)?, &params.0).await?.ok_or(ApiError::NotFound)?;
    Ok(data(stats))
}

/// `GET /api/stats/commanders`.
pub async fn commanders(State(state): State<AppState>, params: Params) -> ApiResult<Json<Value>> {
    Ok(data(stats::commanders(&state.pool, &params.0).await?))
}

/// `GET /api/stats/commanders/:id` (a published id, stored Scryfall id, or card name).
pub async fn commander(State(state): State<AppState>, Path(id): Path<String>, params: Params) -> ApiResult<Json<Value>> {
    let stats = stats::commander(&state.pool, &id, &params.0).await?.ok_or(ApiError::NotFound)?;
    Ok(data(stats))
}
