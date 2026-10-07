//! Applies the Ecto migrations a database is missing, recording them in `schema_migrations`
//! exactly as Ecto does, so the Elixir and Rust servers can share a database file.
//!
//! Each migration's SQL is generated from `mix ecto.migrate --log-migrations-sql` by
//! `rust/scripts/dump-migrations.py` and embedded at build time. The few Elixir migrations
//! that computed data in Elixir code have that step ported here ([`data_step`]).

use std::collections::{HashMap, HashSet};

use sqlx::{AssertSqlSafe, Connection, SqliteConnection};

use super::Pool;

#[allow(clippy::unreadable_literal)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/migrations.rs"));
}
pub use generated::MIGRATIONS;

/// Ecto's `schema_migrations` table.
const SCHEMA_MIGRATIONS: &str = r#"CREATE TABLE IF NOT EXISTS "schema_migrations" ("version" INTEGER PRIMARY KEY, "inserted_at" TEXT)"#;

/// Errors while migrating.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// A statement failed.
    #[error("migration {version} failed: {source}")]
    Migration {
        /// The failing migration.
        version: i64,
        /// The database error.
        source: sqlx::Error,
    },
    /// Bookkeeping failed.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Runs every pending migration, oldest first. Returns the versions applied.
pub async fn run(pool: &Pool) -> Result<Vec<i64>, MigrateError> {
    let mut conn = pool.acquire().await?;
    sqlx::raw_sql(SCHEMA_MIGRATIONS).execute(&mut *conn).await?;
    let applied: HashSet<i64> =
        sqlx::query_scalar::<_, i64>("SELECT version FROM schema_migrations")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();

    let mut ran = Vec::new();
    for &(version, name, sql) in MIGRATIONS {
        if applied.contains(&version) {
            continue;
        }
        tracing::info!("== Running {version} {name}");
        apply(&mut conn, version, sql)
            .await
            .map_err(|source| MigrateError::Migration { version, source })?;
        ran.push(version);
    }
    Ok(ran)
}

async fn apply(conn: &mut SqliteConnection, version: i64, sql: &str) -> Result<(), sqlx::Error> {
    // `PRAGMA foreign_keys` is a no-op inside a transaction, so migrations that rebuild
    // tables (Ecto's `@disable_ddl_transaction`) run statement by statement.
    let inserted_at = super::UtcDateTime::now()
        .to_ecto_string()
        .trim_end_matches('Z')
        .to_owned();
    if sql.contains("PRAGMA foreign_keys = OFF") {
        sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
            .execute(&mut *conn)
            .await?;
        data_step(&mut *conn, version).await?;
        record(&mut *conn, version, &inserted_at).await
    } else {
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
            .execute(&mut *tx)
            .await?;
        data_step(&mut tx, version).await?;
        record(&mut tx, version, &inserted_at).await?;
        tx.commit().await
    }
}

async fn record(
    conn: &mut SqliteConnection,
    version: i64,
    inserted_at: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO schema_migrations (version, inserted_at) VALUES (?, ?)")
        .bind(version)
        .bind(inserted_at)
        .execute(conn)
        .await
        .map(|_| ())
}

/// The data each migration computed in Elixir code.
async fn data_step(conn: &mut SqliteConnection, version: i64) -> Result<(), sqlx::Error> {
    match version {
        20_260_921_203_255 => backfill_portable_ids(conn).await,
        20_260_926_020_644 => recompute_commander_pairings(conn).await,
        20_260_929_224_420 => include_commander_colors(conn).await,
        20_261_007_074_539 => renormalize_card_names(conn).await,
        20_261_007_125_350 => recompute_can_be_commander(conn).await,
        _ => Ok(()),
    }
}

/// `AddPortableIdToGames`: every existing game gets a random UUID.
async fn backfill_portable_ids(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM games WHERE portable_id IS NULL")
        .fetch_all(&mut *conn)
        .await?;
    for id in ids {
        sqlx::query("UPDATE games SET portable_id = ? WHERE id = ?")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// `RecomputeCommanderPairings`: recompute each card's pairing from its type line and text.
async fn recompute_commander_pairings(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    type PairingRow = (String, Option<String>, Option<String>, Option<String>);
    let rows: Vec<PairingRow> =
        sqlx::query_as("SELECT id, type_line, oracle_text, commander_pairing FROM cards")
            .fetch_all(&mut *conn)
            .await?;
    for (id, type_line, oracle_text, current) in rows {
        let pairing = lotus::commander_pairing(
            type_line.as_deref().unwrap_or_default(),
            oracle_text.as_deref().unwrap_or_default(),
        )
        .map(|pairing| pairing.as_str().to_owned());
        if pairing != current {
            sqlx::query("UPDATE cards SET commander_pairing = ? WHERE id = ?")
                .bind(pairing)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// `IncludeCommanderColorsInDeckIdentities`: widen each deck's identity to cover every
/// commander card, keeping colors already recorded.
async fn include_commander_colors(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    type DeckRow = (
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let decks: Vec<DeckRow> = sqlx::query_as(
        "SELECT id, color_identity, commander_card_id, commander_name, partner_card_id, partner_name FROM decks",
    )
    .fetch_all(&mut *conn)
    .await?;
    if decks.is_empty() {
        return Ok(());
    }
    let cards: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, normalized_name, color_identity FROM cards")
            .fetch_all(&mut *conn)
            .await?;
    let decode = |colors: &str| serde_json::from_str::<Vec<String>>(colors).unwrap_or_default();
    let by_id: HashMap<&str, Vec<String>> = cards
        .iter()
        .map(|(id, _, colors)| (id.as_str(), decode(colors)))
        .collect();
    // Stored names may predate the apostrophe-free normalization; compare both forms.
    let by_name: HashMap<String, Vec<String>> = cards
        .iter()
        .map(|(_, name, colors)| (lotus::normalize_name(name), decode(colors)))
        .collect();

    for (id, identity, commander_id, commander_name, partner_id, partner_name) in decks {
        let mut letters: String = identity.clone().unwrap_or_default();
        for (card_id, name) in [(commander_id, commander_name), (partner_id, partner_name)] {
            let colors = card_id
                .as_deref()
                .and_then(|card_id| by_id.get(card_id))
                .or_else(|| {
                    name.as_deref()
                        .and_then(|name| by_name.get(&lotus::normalize_name(name)))
                });
            if let Some(colors) = colors {
                letters.extend(colors.iter().map(String::as_str));
            }
        }
        let widened = crate::games::color_identity::canonical(&letters);
        if widened != identity.unwrap_or_default() {
            sqlx::query("UPDATE decks SET color_identity = ? WHERE id = ?")
                .bind(widened)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// `DropApostrophesFromNormalizedCardNames`: stored names follow lotus's
/// [`lotus::normalize_name`], which drops apostrophes and squashes whitespace. The Ecto
/// migration approximates this in SQL; this recomputes it exactly.
async fn renormalize_card_names(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    for table in ["cards", "catalog_cards_staging"] {
        let rows: Vec<(String, String, String)> = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT id, name, normalized_name FROM {table}"
        )))
        .fetch_all(&mut *conn)
        .await?;
        for (id, name, stored) in rows {
            let normalized = lotus::normalize_name(&name);
            if normalized != stored {
                sqlx::query(AssertSqlSafe(format!(
                    "UPDATE {table} SET normalized_name = ? WHERE id = ?"
                )))
                .bind(normalized)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    Ok(())
}

/// `RecomputeCanBeCommander`: the stored flag follows [`lotus::can_be_commander`] (CR 903.3:
/// legendary creature, Vehicle, or Spacecraft judged by the front face, or "can be your
/// commander" text). The Elixir rule accepted only legendary creatures and judged the whole
/// type line, so legendary Vehicles were missing and cards with a legendary-creature back face
/// were wrongly eligible.
async fn recompute_can_be_commander(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let rows: Vec<(String, Option<String>, Option<String>, bool)> =
        sqlx::query_as("SELECT id, type_line, oracle_text, can_be_commander FROM cards")
            .fetch_all(&mut *conn)
            .await?;
    for (id, type_line, oracle_text, current) in rows {
        let eligible = lotus::can_be_commander(
            type_line.as_deref().unwrap_or_default(),
            oracle_text.as_deref().unwrap_or_default(),
        );
        if eligible != current {
            sqlx::query("UPDATE cards SET can_be_commander = ? WHERE id = ?")
                .bind(eligible)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}
