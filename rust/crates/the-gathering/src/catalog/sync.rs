//! Streams a Scryfall generation into staging and atomically publishes it, recording each run in `catalog_syncs`.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use lotus::scryfall::{BulkError, JsonLines, ScryfallCard};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::card_data::{self, CardData};
use super::scryfall::Scryfall;
use crate::db::{self, Pool, UtcDateTime};

const BATCH_SIZE: usize = 250;
const MAX_ERROR_CHARS: usize = 4_000;

/// Where a generation comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Download Scryfall's `default_cards` bulk file.
    Scryfall,
    /// An uncompressed JSON Lines file.
    File(PathBuf),
    /// A gzip-compressed JSON Lines file.
    GzipFile(PathBuf),
}

/// The latest run (`%SyncState{}`); a catalog that was never synced reports `never`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SyncState {
    /// `never`, `running`, `succeeded`, or `failed`.
    pub status: String,
    /// When the run started.
    pub last_started_at: Option<UtcDateTime>,
    /// When it finished.
    pub last_finished_at: Option<UtcDateTime>,
    /// Cards published (or present when the run started).
    pub card_count: i64,
    /// Scryfall's generation time.
    pub scryfall_updated_at: Option<UtcDateTime>,
    /// The failure, truncated to 4000 characters.
    pub last_error: Option<String>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            status: "never".to_owned(),
            last_started_at: None,
            last_finished_at: None,
            card_count: 0,
            scryfall_updated_at: None,
            last_error: None,
        }
    }
}

/// The state of the latest catalog sync.
pub async fn status(pool: &Pool) -> Result<SyncState, sqlx::Error> {
    let row = sqlx::query_as!(
        SyncState,
        r#"SELECT status, last_started_at AS "last_started_at: UtcDateTime",
                  last_finished_at AS "last_finished_at: UtcDateTime", card_count,
                  scryfall_updated_at AS "scryfall_updated_at: UtcDateTime", last_error
           FROM catalog_syncs ORDER BY id DESC LIMIT 1"#
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or_default())
}

type Batch = Result<Vec<CardData>, String>;

/// Reads records on a blocking thread and hands them over in batches. A line that is not
/// JSON fails the run; a JSON object that is not a usable Scryfall card (one
/// [`card_data::from_scryfall`] rejects) is skipped.
fn read_batches<R: std::io::BufRead>(lines: JsonLines<R, Value>, sender: &mpsc::Sender<Batch>) {
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for record in lines {
        let value = match record {
            Ok(value) => value,
            Err(BulkError::Json { line, source }) => {
                let _ = sender.blocking_send(Err(format!(
                    "invalid Scryfall bulk JSON: {source} (line {line})"
                )));
                return;
            }
            Err(error) => {
                let _ = sender.blocking_send(Err(format!("could not stream bulk data: {error}")));
                return;
            }
        };
        if !value.is_object() {
            let _ = sender.blocking_send(Err("Scryfall bulk line is not a JSON object".to_owned()));
            return;
        }
        let Some(row) = serde_json::from_value::<ScryfallCard>(value)
            .ok()
            .as_ref()
            .and_then(card_data::from_scryfall)
        else {
            continue;
        };
        batch.push(row);
        if batch.len() == BATCH_SIZE {
            if sender
                .blocking_send(Ok(std::mem::take(&mut batch)))
                .is_err()
            {
                return;
            }
            batch.reserve(BATCH_SIZE);
        }
    }
    if !batch.is_empty() {
        let _ = sender.blocking_send(Ok(batch));
    }
}

fn spawn_reader(path: &Path, compressed: bool) -> Result<mpsc::Receiver<Batch>, String> {
    let file = File::open(path)
        .map_err(|error| format!("could not stream {}: {error}", path.display()))?;
    let (sender, receiver) = mpsc::channel(4);
    tokio::task::spawn_blocking(move || {
        if compressed {
            read_batches(JsonLines::gzip(file), &sender);
        } else {
            read_batches(JsonLines::new(BufReader::new(file)), &sender);
        }
    });
    Ok(receiver)
}

/// The best printing of each card in the batch replaces the staged one
/// when its selection key is greater.
async fn stage_batch(pool: &Pool, rows: Vec<CardData>) -> Result<(), sqlx::Error> {
    let mut candidates: HashMap<String, CardData> = HashMap::new();
    for row in rows {
        match candidates.get(&row.oracle_id) {
            Some(best) if best.selection_key >= row.selection_key => {}
            _ => {
                candidates.insert(row.oracle_id.clone(), row);
            }
        }
    }
    let oracle_ids: Vec<&String> = candidates.keys().collect();
    let ids_json = serde_json::to_string(&oracle_ids).unwrap_or_else(|_| "[]".to_owned());
    let mut tx = db::begin(pool).await?;
    let existing: HashMap<String, String> = sqlx::query!(
        "SELECT oracle_id, selection_key FROM catalog_cards_staging
         WHERE oracle_id IN (SELECT value FROM json_each(?))",
        ids_json
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| (row.oracle_id, row.selection_key))
    .collect();
    for candidate in candidates.values() {
        let staged = existing
            .get(&candidate.oracle_id)
            .map_or("", String::as_str);
        if staged < candidate.selection_key.as_str() {
            card_data::stage_card(&mut tx, candidate).await?;
        }
    }
    tx.commit().await
}

/// Replaces `cards` with the staged generation in one transaction.
async fn publish(pool: &Pool) -> Result<(), sqlx::Error> {
    let mut tx = db::begin(pool).await?;
    sqlx::query!("DELETE FROM cards").execute(&mut *tx).await?;
    sqlx::query!(
        "INSERT INTO cards (id, oracle_id, name, normalized_name, mana_cost, cmc, type_line, oracle_text, colors,
            color_identity, image_uris, set_code, collector_number, released_at, layout, rarity, game_changer,
            commander_legal, can_be_commander, commander_pairing, inserted_at, updated_at)
         SELECT id, oracle_id, name, normalized_name, mana_cost, cmc, type_line, oracle_text, colors,
            color_identity, image_uris, set_code, collector_number, released_at, layout, rarity, game_changer,
            commander_legal, can_be_commander, commander_pairing, inserted_at, updated_at
         FROM catalog_cards_staging"
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

async fn stage_all(pool: &Pool, path: &Path, compressed: bool) -> Result<i64, String> {
    let mut batches = spawn_reader(path, compressed)?;
    sqlx::query!("DELETE FROM catalog_cards_staging")
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    while let Some(batch) = batches.recv().await {
        stage_batch(pool, batch?)
            .await
            .map_err(|error| error.to_string())?;
    }
    let count =
        sqlx::query_scalar!(r#"SELECT count(*) AS "count!: i64" FROM catalog_cards_staging"#)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
    if count == 0 {
        return Err("staged catalog generation is empty".to_owned());
    }
    publish(pool).await.map_err(|error| error.to_string())?;
    Ok(count)
}

async fn start_state(pool: &Pool) -> Result<i64, sqlx::Error> {
    let now = UtcDateTime::now();
    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!: i64" FROM cards"#)
        .fetch_one(pool)
        .await?;
    sqlx::query_scalar!(
        r#"INSERT INTO catalog_syncs (status, last_started_at, card_count, last_error, inserted_at, updated_at)
           VALUES ('running', ?, ?, NULL, ?, ?) RETURNING id AS "id!: i64""#,
        now,
        count,
        now,
        now
    )
    .fetch_one(pool)
    .await
}

async fn finish_state(
    pool: &Pool,
    id: i64,
    count: i64,
    updated_at: Option<UtcDateTime>,
) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE catalog_syncs SET status = 'succeeded', last_finished_at = ?, card_count = ?,
         scryfall_updated_at = ?, last_error = NULL, updated_at = ? WHERE id = ?",
        now,
        count,
        updated_at,
        now,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn fail_state(pool: &Pool, id: i64, error: &str) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    let message: String = error.chars().take(MAX_ERROR_CHARS).collect();
    sqlx::query!(
        "UPDATE catalog_syncs SET status = 'failed', last_finished_at = ?, last_error = ?, updated_at = ? WHERE id = ?",
        now,
        message,
        now,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Stages a generation in batches, publishes it only when it is complete and non-empty,
/// records the run, and links catalog references afterwards.
/// Returns the number of cards published, or the failure message. A failure never touches
/// the published catalog.
pub async fn run(pool: &Pool, scryfall: &Scryfall, source: Source) -> Result<i64, String> {
    let state = start_state(pool).await.map_err(|error| error.to_string())?;
    let mut temporary = None;
    let result = async {
        let (path, compressed, updated_at) = match &source {
            Source::Scryfall => {
                let (path, updated_at) = scryfall
                    .download_bulk(&std::env::temp_dir())
                    .await
                    .map_err(|error| format!("{error:#}"))?;
                temporary = Some(path.clone());
                (path, true, updated_at)
            }
            Source::File(path) => (path.clone(), false, None),
            Source::GzipFile(path) => (path.clone(), true, None),
        };
        let count = stage_all(pool, &path, compressed).await?;
        Ok::<_, String>((count, updated_at))
    }
    .await;
    if let Some(path) = temporary {
        let _ = tokio::fs::remove_file(path).await;
    }
    match result {
        Ok((count, updated_at)) => {
            finish_state(pool, state, count, updated_at)
                .await
                .map_err(|error| error.to_string())?;
            if let Err(error) = super::backfill::run(pool).await {
                tracing::error!("catalog backfill after sync failed: {error}");
            }
            Ok(count)
        }
        Err(message) => {
            tracing::error!("catalog sync failed: {message}");
            fail_state(pool, state, &message)
                .await
                .map_err(|error| error.to_string())?;
            Err(message)
        }
    }
}
