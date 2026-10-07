//! Statistics: `{"data": stats}` for the overview, players, decks, and commanders.

use axum::Json;
use axum::extract::State;
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::stats::{self, DateRange};
use crate::web::extract::{PathParam, QueryParams};

use super::data;

/// `GET /api/stats/overview`.
pub async fn overview(
    State(state): State<AppState>,
    QueryParams(range): QueryParams<DateRange>,
) -> ApiResult<Json<Value>> {
    Ok(data(stats::overview(&state.pool, &range).await?))
}

/// `GET /api/stats/players/:id`.
pub async fn player(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    QueryParams(range): QueryParams<DateRange>,
) -> ApiResult<Json<Value>> {
    let payload = stats::player(&state.pool, id, &range)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(data(payload))
}

/// `GET /api/stats/decks/:id`.
pub async fn deck(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    QueryParams(range): QueryParams<DateRange>,
) -> ApiResult<Json<Value>> {
    let payload = stats::deck(&state.pool, id, &range)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(data(payload))
}

/// `GET /api/stats/commanders`.
pub async fn commanders(
    State(state): State<AppState>,
    QueryParams(range): QueryParams<DateRange>,
) -> ApiResult<Json<Value>> {
    Ok(data(stats::commanders(&state.pool, &range).await?))
}

/// `GET /api/stats/commanders/:id` (a published id, stored Scryfall id, or card name).
pub async fn commander(
    State(state): State<AppState>,
    PathParam(id): PathParam<String>,
    QueryParams(range): QueryParams<DateRange>,
) -> ApiResult<Json<Value>> {
    let payload = stats::commander(&state.pool, &id, &range)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(data(payload))
}
