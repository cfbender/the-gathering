//! The card-recognition bundle (`CardIdBundleController`, `CardIdBundleJSON`) and the
//! labelled-crop corrections with their export (`CardIdCorrectionController`,
//! `CardIdCorrectionJSON`, `CardIdExportAuth`).

use std::path::Path as FsPath;

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};
use subtle::ConstantTimeEq;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::accounts::User;
use crate::card_id::{self, corrections::CorrectionError};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::web::auth::{AuthUser, MaybeUser};
use crate::web::extract::{JsonBody, PathParam, QueryParams};

use super::{check_user_limit, data};

const NO_STORE: &str = "private, no-store";

/// Streams a file with the given content type and cache policy.
async fn send_file(
    path: &FsPath,
    content_type: &'static str,
    cache_control: &'static str,
) -> ApiResult<Response> {
    let request = Request::new(Body::empty());
    let response = ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    if response.status() != StatusCode::OK {
        return Err(ApiError::NotFound);
    }
    let mut response = response.map(Body::new);
    let headers = response.headers_mut();
    headers.remove(header::LAST_MODIFIED);
    headers.remove(header::ACCEPT_RANGES);
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    Ok(response)
}

/// `GET /api/cardid/bundle`: the current bundle's manifest and file URLs.
pub async fn bundle_show(State(state): State<AppState>) -> ApiResult<Response> {
    let manifest = card_id::current_manifest(&state.config.data_dir)
        .await
        .ok_or(ApiError::NotFound)?;
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let listed = manifest.get("files").and_then(Value::as_object);
    let files: Map<String, Value> = card_id::FILES
        .iter()
        .filter(|name| {
            **name != "printings.json" || listed.is_some_and(|files| files.contains_key(**name))
        })
        .map(|name| {
            (
                (*name).to_owned(),
                json!(format!("/api/cardid/bundles/{version}/{name}")),
            )
        })
        .collect();
    let field = |key: &str| manifest.get(key).cloned().unwrap_or(Value::Null);
    let mut response = data(json!({
        "version": version,
        "created": field("created"),
        "gallery": field("gallery"),
        "constants": field("constants"),
        "files": files,
    }))
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    Ok(response)
}

/// `GET /api/cardid/bundles/:version/:name`: one file of a version, cacheable forever.
pub async fn bundle_file(
    State(state): State<AppState>,
    PathParam((version, name)): PathParam<(String, String)>,
) -> ApiResult<Response> {
    let path = card_id::file_path(&state.config.data_dir, &version, &name)
        .await
        .ok_or(ApiError::NotFound)?;
    send_file(
        &path,
        card_id::content_type(&name),
        "private, max-age=31536000, immutable",
    )
    .await
}

impl From<CorrectionError> for ApiError {
    fn from(error: CorrectionError) -> Self {
        match error {
            CorrectionError::BadRequest => Self::BadRequest,
            CorrectionError::Forbidden => Self::Forbidden,
            CorrectionError::Io(error) => Self::Internal(error.into()),
        }
    }
}

/// `POST /api/cardid/corrections`: stores a labelled crop (rate limited per member).
pub async fn corrections_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(correction): JsonBody<Value>,
) -> ApiResult<Response> {
    check_user_limit(
        &state,
        "corrections",
        user.id,
        state.config.rate_limits.corrections,
    )?;
    let capture_id = state.corrections.save(&correction, user.id).await?;
    Ok((
        StatusCode::CREATED,
        data(json!({ "capture_id": capture_id })),
    )
        .into_response())
}

/// `CardIdExportAuth`: the read-only export is open to a signed-in administrator, or to
/// the configured bearer token while the administrator it acts for is active.
async fn export_authorized(
    state: &AppState,
    headers: &HeaderMap,
    user: Option<&User>,
) -> ApiResult<()> {
    let mut authorization = headers.get_all(header::AUTHORIZATION).iter();
    let authorized = match (authorization.next(), authorization.next()) {
        (None, _) => user.is_some_and(User::is_admin),
        (Some(value), None) => match value
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
        {
            Some(token) => valid_export_token(state, token).await?,
            None => false,
        },
        (Some(_), Some(_)) => false,
    };
    if authorized {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn valid_export_token(state: &AppState, token: &str) -> ApiResult<bool> {
    let (Some(expected), Some(admin_id)) = (
        state.config.cardid_corrections_token.as_deref(),
        state.config.cardid_corrections_admin_id,
    ) else {
        return Ok(false);
    };
    if expected.len() < 32 || !bool::from(token.as_bytes().ct_eq(expected.as_bytes())) {
        return Ok(false);
    }
    let admin = state.accounts.get_user(admin_id).await?;
    Ok(admin.is_some_and(|admin| admin.is_admin() && admin.disabled_at.is_none()))
}

/// `GET /api/cardid/corrections` parameters.
#[derive(Debug, Default, serde::Deserialize)]
pub struct CursorQuery {
    /// Where the previous page ended.
    #[serde(default)]
    cursor: usize,
}

/// `GET /api/cardid/corrections?cursor=N`: a page of labels for Oracle's importer.
pub async fn corrections_index(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    headers: HeaderMap,
    QueryParams(query): QueryParams<CursorQuery>,
) -> ApiResult<Response> {
    export_authorized(&state, &headers, user.as_ref()).await?;
    let page = state.corrections.page(query.cursor).await?;
    let mut response = Json(json!({ "data": page })).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
    Ok(response)
}

/// `GET /api/cardid/corrections/:id/crop`: a capture's JPEG.
pub async fn corrections_crop(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    headers: HeaderMap,
    PathParam(id): PathParam<String>,
) -> ApiResult<Response> {
    export_authorized(&state, &headers, user.as_ref()).await?;
    let path = state
        .corrections
        .crop_path(&id)
        .await
        .ok_or(ApiError::NotFound)?;
    send_file(&path, "image/jpeg", NO_STORE).await
}
