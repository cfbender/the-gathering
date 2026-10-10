//! The SQLite database: connection pool, transactions, and migrations.

pub mod migrate;
pub mod time;

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub use self::time::{IsoDate, UtcDateTime};

/// The pool type every query runs on.
pub type Pool = SqlitePool;

/// An open write transaction from [`begin`].
///
/// It dereferences to the connection, so `&mut *tx` (or `&mut tx` where a
/// `&mut SqliteConnection` is expected) runs queries in it, and `conn.begin()` on it opens a
/// savepoint. While it is open, `audit_context` names the audit operation of the task that
/// began it (see [`crate::audit`]); [`Tx::commit`] clears that before committing, and dropping
/// it uncommitted rolls everything back, so the committed context is always empty.
#[derive(Debug)]
pub struct Tx {
    inner: Transaction<'static, Sqlite>,
    audited: bool,
}

impl Tx {
    /// Clears the audit context, then commits.
    pub async fn commit(mut self) -> Result<(), sqlx::Error> {
        if self.audited {
            set_audit_context(&mut self.inner, None).await?;
        }
        self.inner.commit().await
    }

    /// Rolls back every change, including the audit context.
    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        self.inner.rollback().await
    }
}

impl Deref for Tx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.inner
    }
}

impl DerefMut for Tx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.inner
    }
}

async fn set_audit_context(
    conn: &mut SqliteConnection,
    operation_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE audit_context SET operation_id = ? WHERE id = 1",
        operation_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Opens (creating if missing) the database at `path` with WAL, foreign keys on,
/// and a five-second busy timeout.
pub async fn connect(path: &Path, pool_size: u32) -> Result<Pool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(pool_size.max(1))
        .connect_with(options)
        .await
}

/// An in-memory database for tests: one connection, so every query sees the same data.
pub async fn connect_memory() -> Result<Pool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
}

/// Starts a write transaction that takes SQLite's write lock up front.
///
/// Deferred transactions that read before writing fail at once with "database is locked"
/// when another connection commits in between; `BEGIN IMMEDIATE` waits for the busy timeout
/// instead.
///
/// Inside [`crate::audit::scope`], the transaction's row changes are attributed to that
/// operation.
pub async fn begin(pool: &Pool) -> Result<Tx, sqlx::Error> {
    let mut inner = pool.begin_with("BEGIN IMMEDIATE").await?;
    let operation = crate::audit::current_operation();
    if operation.is_some() {
        // On failure `inner` drops and rolls back.
        set_audit_context(&mut inner, operation).await?;
    }
    Ok(Tx {
        inner,
        audited: operation.is_some(),
    })
}

/// Whether `error` is a unique-constraint violation, optionally on one of `columns`
/// (SQLite reports `UNIQUE constraint failed: players.name`).
pub fn is_unique_violation(error: &sqlx::Error, columns: &[&str]) -> bool {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            columns.is_empty() || columns.iter().any(|column| db.message().contains(column))
        }
        _ => false,
    }
}

/// Whether `error` is a foreign-key violation.
pub fn is_foreign_key_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.is_foreign_key_violation())
}
