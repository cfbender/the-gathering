//! Administrator-only persistent operation history and paginated row snapshots.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, header};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::db::UtcDateTime;
use crate::error::ApiError;
use crate::state::AppState;
use crate::web::extract::{PathParam, QueryParams};

/// Audit history filters. All text filters are bound parameters, not SQL fragments.
#[derive(Default, Deserialize)]
pub struct HistoryQuery {
    page: Option<i64>,
    per_page: Option<i64>,
    search: Option<String>,
    outcome: Option<String>,
}

impl HistoryQuery {
    fn pagination(&self) -> (i64, i64, i64) {
        let page = self.page.unwrap_or(1).clamp(1, 1_000_000);
        let per_page = self.per_page.unwrap_or(25).clamp(1, 100);
        (page, per_page, (page - 1) * per_page)
    }
}

fn response(data: &[Value], page: i64, per_page: i64, total: i64) -> (HeaderMap, Json<Value>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (
        headers,
        Json(
            json!({"data": data, "pagination": {"page": page, "per_page": per_page, "total": total}}),
        ),
    )
}

/// `GET /api/admin/audit`: newest operations first; NULL status means unknown/in progress.
pub async fn index(
    State(state): State<AppState>,
    QueryParams(query): QueryParams<HistoryQuery>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let (page, per_page, offset) = query.pagination();
    let search = format!("%{}%", query.search.as_deref().unwrap_or_default().trim());
    let outcome = query.outcome.as_deref().unwrap_or("");
    if !["", "success", "failed", "unknown"].contains(&outcome) {
        return Err(ApiError::BadRequest);
    }
    let rows = sqlx::query!(
        r#"SELECT o.id AS "id!", o.actor_id, o.actor_name, o.action, o.target, o.request_id, o.status,
                  o.inserted_at AS "inserted_at: UtcDateTime", o.completed_at AS "completed_at: UtcDateTime",
                  (SELECT count(*) FROM audit_changes c WHERE c.operation_id = o.id) AS "change_count!: i64"
           FROM audit_operations o
           WHERE (coalesce(o.actor_name, '') LIKE ?1 OR o.action LIKE ?1 OR o.target LIKE ?1)
             AND EXISTS (SELECT 1 FROM audit_changes c WHERE c.operation_id = o.id)
             AND (?2 = '' OR (?2 = 'success' AND o.status BETWEEN 200 AND 399)
                  OR (?2 = 'failed' AND o.status >= 400) OR (?2 = 'unknown' AND o.status IS NULL))
           ORDER BY o.id DESC LIMIT ?3 OFFSET ?4"#,
        search, outcome, per_page, offset
    ).fetch_all(&state.pool).await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) FROM audit_operations o
           WHERE (coalesce(o.actor_name, '') LIKE ?1 OR o.action LIKE ?1 OR o.target LIKE ?1)
             AND EXISTS (SELECT 1 FROM audit_changes c WHERE c.operation_id = o.id)
             AND (?2 = '' OR (?2 = 'success' AND o.status BETWEEN 200 AND 399)
                  OR (?2 = 'failed' AND o.status >= 400) OR (?2 = 'unknown' AND o.status IS NULL))"#,
        search, outcome
    ).fetch_one(&state.pool).await?;
    let data: Vec<_> = rows.into_iter().map(|row| json!({
        "id": row.id, "actor_id": row.actor_id, "actor_name": row.actor_name,
        "action": row.action, "target": row.target, "request_id": row.request_id,
        "status": row.status, "inserted_at": row.inserted_at, "completed_at": row.completed_at,
        "change_count": row.change_count,
    })).collect();
    Ok(response(&data, page, per_page, total))
}

/// `GET /api/admin/audit/{id}`: row changes ordered by commit-local sequence, never truncated.
pub async fn show(
    State(state): State<AppState>,
    PathParam(id): PathParam<i64>,
    QueryParams(query): QueryParams<HistoryQuery>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let exists = sqlx::query_scalar!("SELECT id FROM audit_operations WHERE id = ?", id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound);
    }
    let (page, per_page, offset) = query.pagination();
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", entity, entity_id, before_json AS "before: sqlx::types::Json<Value>",
                  after_json AS "after: sqlx::types::Json<Value>", inserted_at AS "inserted_at: UtcDateTime"
           FROM audit_changes WHERE operation_id = ? ORDER BY id LIMIT ? OFFSET ?"#,
        id, per_page, offset
    ).fetch_all(&state.pool).await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM audit_changes WHERE operation_id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await?;
    let data: Vec<_> = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id, "entity": row.entity, "entity_id": row.entity_id,
                "before": row.before.map(|v| v.0), "after": row.after.map(|v| v.0),
                "inserted_at": row.inserted_at,
            })
        })
        .collect();
    Ok(response(&data, page, per_page, total))
}
