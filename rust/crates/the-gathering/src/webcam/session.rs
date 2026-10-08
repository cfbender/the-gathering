//! Versioned server-owned snapshots in
//! `webcam_table_sessions`, retained for seven days after last activity or until their room
//! closes as idle. Writes finish before an action is acknowledged.
//!
//! The BLOB holds `{"version": 2, "state": entry}` JSON, the shape releases up to 0.2 also
//! wrote, so tables saved before an upgrade restore. Anything that does not decode as that (an
//! older version, an Erlang term those releases' predecessors wrote, garbage) is treated as
//! missing and the table starts fresh.
//!
//! `expires_at` is text: whole seconds (`2026-10-06T21:21:40Z`), or microseconds in rows
//! written by releases up to 0.2. [`UtcDateTime`] reads both, and the two forms compare in
//! the right order as text to within a second, which is all pruning needs.

use time::Duration;

use crate::db::{Pool, UtcDateTime};

use super::room::Entry;

const VERSION: i64 = 2;
const RETENTION_DAYS: i64 = 7;

#[derive(serde::Serialize)]
struct SnapshotRef<'a> {
    version: i64,
    state: &'a Entry,
}

#[derive(serde::Deserialize)]
struct Snapshot {
    version: i64,
    state: Entry,
}

/// The saved table, unless missing, expired, or in another format.
pub async fn load(pool: &Pool, id: &str) -> Result<Option<Entry>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT snapshot, expires_at AS "expires_at: UtcDateTime" FROM webcam_table_sessions WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.expires_at <= UtcDateTime::now() {
        return Ok(None);
    }
    Ok(serde_json::from_slice::<Snapshot>(&row.snapshot)
        .ok()
        .filter(|snapshot| snapshot.version == VERSION)
        .map(|snapshot| snapshot.state))
}

/// Saves the table, pushing its expiry a week out.
pub async fn save(pool: &Pool, id: &str, entry: &Entry) -> Result<(), sqlx::Error> {
    let snapshot = serde_json::to_vec(&SnapshotRef {
        version: VERSION,
        state: entry,
    })
    .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let expires_at = UtcDateTime::now().plus(Duration::days(RETENTION_DAYS));
    sqlx::query!(
        r#"INSERT INTO webcam_table_sessions (id, snapshot, expires_at) VALUES (?, ?, ?)
           ON CONFLICT (id) DO UPDATE SET snapshot = excluded.snapshot, expires_at = excluded.expires_at"#,
        id,
        snapshot,
        expires_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Deletes the table's session.
pub async fn delete(pool: &Pool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM webcam_table_sessions WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes expired sessions; returns how many.
pub async fn prune(pool: &Pool) -> Result<u64, sqlx::Error> {
    let now = UtcDateTime::now();
    let result = sqlx::query!(
        "DELETE FROM webcam_table_sessions WHERE expires_at < ?",
        now
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
