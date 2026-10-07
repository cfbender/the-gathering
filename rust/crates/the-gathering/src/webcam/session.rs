//! `TheGathering.WebcamTables.Session`: versioned server-owned snapshots in
//! `webcam_table_sessions`, retained for seven days after last activity or until their room
//! closes as idle. Writes finish before an action is acknowledged.
//!
//! The BLOB holds `{"version": 2, "state": entry}` JSON, the same shape the Elixir server
//! writes with Jason, so a running table survives switching servers. Anything that does not
//! decode as that (an older version, an Erlang term, garbage) is treated as missing and the
//! table starts fresh, where the Elixir code raised on undecodable JSON.

use time::format_description::FormatItem;
use time::macros::format_description;
use time::{Duration, OffsetDateTime};

use crate::db::{Pool, UtcDateTime};

use super::room::Entry;

const VERSION: i64 = 2;
const RETENTION_DAYS: i64 = 7;
const USEC_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z");

/// `:utc_datetime_usec` as `ecto_sqlite3` stores it.
fn usec(at: OffsetDateTime) -> String {
    at.format(USEC_FORMAT).unwrap_or_default()
}

fn parse_usec(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .or_else(|| UtcDateTime::parse(value).map(UtcDateTime::inner))
}

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
    let row = sqlx::query!(r#"SELECT snapshot, expires_at FROM webcam_table_sessions WHERE id = ?"#, id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if parse_usec(&row.expires_at).is_none_or(|expires| expires <= OffsetDateTime::now_utc()) {
        return Ok(None);
    }
    Ok(serde_json::from_slice::<Snapshot>(&row.snapshot)
        .ok()
        .filter(|snapshot| snapshot.version == VERSION)
        .map(|snapshot| snapshot.state))
}

/// Saves the table, pushing its expiry a week out.
pub async fn save(pool: &Pool, id: &str, entry: &Entry) -> Result<(), sqlx::Error> {
    let snapshot = serde_json::to_vec(&SnapshotRef { version: VERSION, state: entry })
        .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let expires_at = usec(OffsetDateTime::now_utc() + Duration::days(RETENTION_DAYS));
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
    sqlx::query!("DELETE FROM webcam_table_sessions WHERE id = ?", id).execute(pool).await?;
    Ok(())
}

/// Deletes expired sessions; returns how many.
pub async fn prune(pool: &Pool) -> Result<u64, sqlx::Error> {
    let now = usec(OffsetDateTime::now_utc());
    let result = sqlx::query!("DELETE FROM webcam_table_sessions WHERE expires_at < ?", now).execute(pool).await?;
    Ok(result.rows_affected())
}
