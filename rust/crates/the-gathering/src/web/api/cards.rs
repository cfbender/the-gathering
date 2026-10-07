//! The card catalog API: search and cards (`CardController`, `CardJSON`), printings
//! (`CardPrintingController`, `CardPrintingJSON`), the sync status and admin triggers
//! (`CatalogController`, `CatalogJSON`), and the image cache (`CardImageController`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::catalog::image_cache::ImageError;
use crate::catalog::printings::{self, LookupError};
use crate::catalog::{Card, Catalog, Printing, images};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::web::params::Params;

use super::data;

fn catalog(state: &AppState) -> Catalog {
    Catalog {
        pool: state.pool.clone(),
    }
}

impl From<LookupError> for ApiError {
    fn from(error: LookupError) -> Self {
        match error {
            LookupError::BadRequest => Self::BadRequest,
            LookupError::NotFound => Self::NotFound,
            LookupError::BadGateway => Self::BadGateway,
            LookupError::Database => Self::Internal(anyhow::anyhow!("printing lookup failed")),
        }
    }
}

fn card_images(card: &Card, variants: &[&str]) -> Value {
    let taken = card
        .image_uris
        .iter()
        .filter(|(variant, _)| variants.contains(&variant.as_str()))
        .map(|(variant, url)| (variant.clone(), url.clone()))
        .collect();
    json!(images::urls(&taken))
}

/// `CardJSON.summary/1`.
pub fn card_summary_json(card: &Card) -> Value {
    json!({
        "id": card.id,
        "oracle_id": card.oracle_id,
        "name": card.name,
        "mana_cost": card.mana_cost,
        "type_line": card.type_line,
        "color_identity": card.color_identity,
        "game_changer": card.game_changer,
        "image_uris": card_images(card, &["small", "normal", "art_crop"]),
        "can_be_commander": card.can_be_commander,
        "commander_pairing": card.commander_pairing,
    })
}

/// `CardJSON.detail/1`.
pub fn card_detail_json(card: &Card) -> Value {
    let mut detail = card_summary_json(card);
    if let Some(object) = detail.as_object_mut() {
        for (key, value) in [
            ("cmc", json!(card.cmc)),
            ("oracle_text", json!(card.oracle_text)),
            ("colors", json!(card.colors)),
            ("set_code", json!(card.set_code)),
            ("collector_number", json!(card.collector_number)),
            ("released_at", json!(card.released_at)),
            ("layout", json!(card.layout)),
            ("rarity", json!(card.rarity)),
            ("commander_legal", json!(card.commander_legal)),
        ] {
            object.insert(key.to_owned(), value);
        }
    }
    detail
}

/// `CardPrintingJSON.summary/1`.
pub fn printing_summary_json(printing: &Printing) -> Value {
    json!({
        "id": printing.id,
        "name": printing.name,
        "game_changer": printing.game_changer,
        "set_code": printing.set_code,
        "set_name": printing.set_name,
        "collector_number": printing.collector_number,
        "lang": printing.lang,
        "image_uris": images::urls(&printing.image_uris),
    })
}

fn boolean_param(value: Option<&str>) -> Option<bool> {
    match value {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

/// `GET /api/cards`: search the local catalog.
pub async fn cards_index(State(state): State<AppState>, params: Params) -> ApiResult<Json<Value>> {
    let limit = params
        .str("limit")
        .and_then(|limit| limit.parse::<i64>().ok())
        .unwrap_or(20);
    let cards = catalog(&state)
        .search(
            params.str("q").unwrap_or_default(),
            Some(limit),
            boolean_param(params.str("commander")),
            boolean_param(params.str("partner")) == Some(true),
        )
        .await?;
    Ok(data(
        cards.iter().map(card_summary_json).collect::<Vec<_>>(),
    ))
}

/// `GET /api/cards/:id`.
pub async fn cards_show(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let card = catalog(&state)
        .get_card(&id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(data(card_detail_json(&card)))
}

/// `GET /api/card-printings`: a page of printings of the card named by `card_id` or `name`.
pub async fn printings_index(
    State(state): State<AppState>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let page = match params.get("page") {
        None => 1,
        Some(Value::String(page)) => page
            .parse::<u32>()
            .ok()
            .filter(|page| *page > 0)
            .ok_or(ApiError::BadRequest)?,
        Some(_) => return Err(ApiError::BadRequest),
    };
    let (printings, has_more) = catalog(&state)
        .list_printings(
            &state.scryfall,
            params.str("card_id"),
            params.str("name"),
            page,
        )
        .await?;
    Ok(Json(json!({
        "data": printings.iter().map(printing_summary_json).collect::<Vec<_>>(),
        "has_more": has_more,
    })))
}

/// `GET /api/card-printings/:id`: a cached printing.
pub async fn printings_show(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let printing = catalog(&state)
        .get_printing(&id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(data(printing_summary_json(&printing)))
}

/// `GET /api/card-printings/:id/details`: everything the card preview shows.
pub async fn printings_details(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let details = printings::details(&state.pool, &state.scryfall, &id).await?;
    let mut response = data(printings::render_details(&details)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(response)
}

/// `GET /api/card-printings/:id/rulings`.
pub async fn printings_rulings(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let rulings = printings::rulings(&state.pool, &state.scryfall, &id).await?;
    Ok(data(rulings))
}

/// `GET /api/catalog`: the latest sync.
pub async fn catalog_show(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let sync = catalog(&state).sync_status().await?;
    Ok(data(json!(sync)))
}

/// `POST /api/admin/catalog/sync`: starts a Scryfall sync unless one is running.
pub async fn catalog_sync(State(state): State<AppState>) -> Response {
    let result = state.catalog_sync.trigger(&state);
    (
        StatusCode::ACCEPTED,
        data(json!({ "status": result.as_str() })),
    )
        .into_response()
}

/// `POST /api/admin/catalog/backfill`: links decks and MVP picks to catalog cards.
pub async fn catalog_backfill(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let summary = crate::catalog::backfill::run(&state.pool).await?;
    Ok(data(json!(summary)))
}

/// `GET /api/card-images?url=<scryfall source>`: the cached JPEG, revalidated by `ETag`.
pub async fn card_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    params: Params,
) -> ApiResult<Response> {
    let source = params.str("url").ok_or(ApiError::BadRequest)?;
    let (body, cache) = state
        .card_images
        .fetch(source)
        .await
        .map_err(|error| match error {
            ImageError::BadRequest => ApiError::BadRequest,
            ImageError::NotFound => ApiError::NotFound,
            ImageError::BadGateway => ApiError::BadGateway,
        })?;
    let digest = crate::catalog::image_cache::hex(&Sha256::digest(&body));
    let etag = format!("\"{digest}\"");
    let revalidated = headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .any(|value| value.to_str().is_ok_and(|value| value == etag));
    let mut response = if revalidated {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (StatusCode::OK, body).into_response()
    };
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    if let Ok(value) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, value);
    }
    headers.insert(
        "x-card-image-cache",
        HeaderValue::from_static(cache.as_str()),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
