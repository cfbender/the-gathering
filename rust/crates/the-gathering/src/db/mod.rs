//! The SQLite database: connection pool, transactions, and migrations.

pub mod migrate;
pub mod time;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

pub use self::time::{IsoDate, UtcDateTime};

/// The pool type every query runs on.
pub type Pool = SqlitePool;

/// An open write transaction.
pub type Tx = Transaction<'static, Sqlite>;

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
pub async fn begin(pool: &Pool) -> Result<Tx, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
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
