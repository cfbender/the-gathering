//! Game history imports and the portable export (`CSVImportController`,
//! `MythicTrackImportController`, `SheetImportController`, `PortableImportController`)
//! with their JSON views.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

use crate::error::{ApiError, ApiResult};
use crate::imports::preview::{self, Source};
use crate::imports::{
    ImportError, ImportGame, ImportSeat, Preview, commit, csv_transfer, portable, sheet_commit,
    sheet_preview,
};
use crate::state::AppState;
use crate::web::auth::AuthUser;
use crate::web::params::Params;

use super::data;

const SAMPLE: &str = "game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
friday-001,2026-09-18,Alice,Birds of a Feather,\"Kangee, Sky Warden\",1,win,Swan Song,75,10,Friday Commander
friday-001,2026-09-18,Bob,Goblins,Krenko Mob Boss,2,loss,,75,10,Friday Commander
";

/// `send_download/3`: an attachment with the given content type.
fn download(body: Vec<u8>, filename: &str, content_type: &'static str) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(disposition) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
    {
        headers.insert(header::CONTENT_DISPOSITION, disposition);
    }
    response
}

// JSON views

fn game_json(game: &ImportGame, seats: &[Value]) -> Value {
    json!({
        "external_id": game.external_id,
        "game_id": game.game_id,
        "played_at": game.played_at,
        "duration_minutes": game.duration_minutes,
        "turns": game.turns,
        "win_condition": game.win_condition,
        "notes": game.notes,
        "lines": game.lines,
        "seats": seats,
        "action": game.action,
        "target_source": game.target_source,
        "target_external_id": game.target_external_id,
        "target_portable_id": game.target_portable_id,
    })
}

/// `CSVImportJSON.csv_seat/2`.
fn csv_seat(seat: &ImportSeat, game: &ImportGame) -> Value {
    json!({
        "game_id": game.game_id,
        "date": game.played_at,
        "player": seat.player,
        "deck": seat.deck,
        "commander": seat.commander,
        "partner": seat.partner_name,
        "seat": seat.seat,
        "result": seat.result,
        "kills": seat.kills,
        "mvp_card": seat.mvp_card,
        "duration_minutes": game.duration_minutes,
        "turns": game.turns,
        "notes": game.notes,
        "line": seat.line,
    })
}

/// `MythicTrackImportJSON.mythic_seat/1`: the seat with `partner_name` as `partner`.
fn mythic_seat(seat: &ImportSeat) -> Value {
    json!({
        "line": seat.line,
        "player": seat.player,
        "discord_id": seat.discord_id,
        "deck": seat.deck,
        "commander": seat.commander,
        "commander_card_id": seat.commander_card_id,
        "partner": seat.partner_name,
        "partner_card_id": seat.partner_card_id,
        "color_identity": seat.color_identity,
        "decklist_url": seat.decklist_url,
        "seat": seat.seat,
        "result": seat.result,
        "kills": seat.kills,
        "mvp_card": seat.mvp_card,
        "mvp_card_id": seat.mvp_card_id,
    })
}

/// `CSVImportJSON.preview/1` and `MythicTrackImportJSON.preview/1`.
fn preview_json(preview: &Preview, source: Source) -> Value {
    let games: Vec<Value> = preview
        .games
        .iter()
        .map(|game| {
            let seats: Vec<Value> = game
                .seats
                .iter()
                .map(|seat| match source {
                    Source::Csv => csv_seat(seat, game),
                    Source::MythicTrack => mythic_seat(seat),
                })
                .collect();
            game_json(game, &seats)
        })
        .collect();
    let mut body = Map::new();
    body.insert("valid".into(), json!(preview.valid));
    body.insert("games".into(), Value::Array(games));
    body.insert("players".into(), json!(preview.players));
    body.insert("decks".into(), json!(preview.decks));
    body.insert("errors".into(), json!(preview.errors));
    body.insert("warnings".into(), json!(preview.warnings));
    if let Some(revision) = &preview.revision {
        body.insert("revision".into(), json!(revision));
    }
    if let Some(review) = &preview.review {
        body.insert("review".into(), json!(review));
    }
    json!({ "data": Value::Object(body) })
}

/// A commit failure: 422 with the preview for invalid files, otherwise the fallback.
fn commit_error(error: ImportError, source: Source) -> Response {
    match error {
        ImportError::Validation(preview) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(preview_json(&preview, source)),
        )
            .into_response(),
        other => other.into_api().into_response(),
    }
}

fn string_param(params: &Params, key: &str) -> ApiResult<String> {
    params
        .str(key)
        .map(str::to_owned)
        .ok_or(ApiError::BadRequest)
}

// CSV

/// `GET /api/imports/csv/sample`: the native template.
pub async fn csv_sample() -> Response {
    download(
        SAMPLE.as_bytes().to_vec(),
        "the-gathering-games.csv",
        "text/csv",
    )
}

/// `POST /api/imports/csv/preview`.
pub async fn csv_preview(State(state): State<AppState>, params: Params) -> ApiResult<Json<Value>> {
    let csv = string_param(&params, "csv")?;
    let preview = csv_transfer::preview(&state, &csv).await?;
    Ok(Json(preview_json(&preview, Source::Csv)))
}

/// `POST /api/imports/csv`: creates, updates (with the reviewed `revision`), and skips.
pub async fn csv_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Response> {
    let csv = string_param(&params, "csv")?;
    let revision = params.str("revision");
    Ok(
        match csv_transfer::run(&state, &csv, Some(user.id), revision).await {
            Ok(result) => data(json!(result)).into_response(),
            Err(error) => commit_error(error, Source::Csv),
        },
    )
}

// Mythic Track

/// `POST /api/imports/mythic_track/preview`.
pub async fn mythic_track_preview(
    State(state): State<AppState>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let json = string_param(&params, "json")?;
    let preview = preview::run(
        &mut *state.pool.acquire().await?,
        Source::MythicTrack,
        &json,
    )
    .await?;
    Ok(Json(preview_json(&preview, Source::MythicTrack)))
}

/// `POST /api/imports/mythic_track`.
pub async fn mythic_track_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Response> {
    let json = string_param(&params, "json")?;
    Ok(
        match commit::run(&state, Source::MythicTrack, &json, Some(user.id)).await {
            Ok(result) => data(json!(result)).into_response(),
            Err(error) => commit_error(error, Source::MythicTrack),
        },
    )
}

// Google Sheet

/// `SheetImportController.validate/1`: `text` is a string and every player, deck, and
/// action choice is an id or `new`/`skip`/`create`.
fn validate_sheet(params: &Value) -> ApiResult<()> {
    if !params.get("text").is_some_and(Value::is_string) {
        return Err(ApiError::BadRequest);
    }
    let valid = ["players", "decks", "actions"]
        .iter()
        .all(|key| match params.get(*key) {
            None => true,
            Some(Value::Object(choices)) => choices.values().all(|value| {
                value.is_i64()
                    || value.is_u64()
                    || value
                        .as_str()
                        .is_some_and(|text| ["new", "skip", "create"].contains(&text))
            }),
            Some(_) => false,
        });
    if valid {
        Ok(())
    } else {
        Err(ApiError::BadRequest)
    }
}

/// `POST /api/imports/sheet/preview`.
pub async fn sheet_preview(
    State(state): State<AppState>,
    Params(params): Params,
) -> ApiResult<Json<Value>> {
    validate_sheet(&params)?;
    let preview = sheet_preview::run(&mut *state.pool.acquire().await?, &params)
        .await
        .map_err(ImportError::into_api)?;
    Ok(data(json!(preview)))
}

/// `POST /api/imports/sheet`: commits a preview with the reviewed `revision`.
pub async fn sheet_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Params(mut params): Params,
) -> ApiResult<Json<Value>> {
    let revision = params
        .as_object_mut()
        .and_then(|params| params.remove("revision"));
    validate_sheet(&params)?;
    let result = sheet_commit::run(
        &state,
        &params,
        revision.as_ref().and_then(Value::as_str),
        Some(user.id),
    )
    .await
    .map_err(ImportError::into_api)?;
    Ok(data(json!(result)))
}

// Portable

/// `GET /api/exports/portable`: the whole history as a versioned JSON download.
pub async fn portable_export(State(state): State<AppState>) -> ApiResult<Response> {
    let export = portable::export(&state).await?;
    let body =
        serde_json::to_vec_pretty(&export).map_err(|error| ApiError::Internal(error.into()))?;
    let today = crate::db::UtcDateTime::now().date();
    let mut response = download(
        body,
        &format!("the-gathering-{today}.json"),
        "application/json",
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// `POST /api/imports/portable/preview`.
pub async fn portable_preview(
    State(state): State<AppState>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let json = string_param(&params, "json")?;
    let summary = portable::preview(&state, &json)
        .await
        .map_err(ImportError::into_api)?;
    Ok(data(json!(summary)))
}

/// `POST /api/imports/portable`.
pub async fn portable_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Json<Value>> {
    let json = string_param(&params, "json")?;
    let summary = portable::run(&state, &json, Some(user.id))
        .await
        .map_err(ImportError::into_api)?;
    Ok(data(json!(summary)))
}
