//! Streams a Scryfall generation into the database, recording each run in `catalog_syncs`.
//! The catalog's cards and the rulings are staged and published atomically; every printing is
//! stored as it streams, and the printings a complete run no longer lists are pruned.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use lotus::scryfall::{BulkError, JsonLines, Ruling, ScryfallCard};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use super::card_data::{self, CardData};
use super::printings::{self, BulkPrinting};
use super::scryfall::Scryfall;
use crate::db::{self, IsoDate, Pool, UtcDateTime};

const BATCH_SIZE: usize = 250;
const MAX_ERROR_CHARS: usize = 4_000;

/// Where a generation comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Download Scryfall's `default_cards` and `rulings` bulk files.
    Scryfall,
    /// An uncompressed JSON Lines file of cards; the stored rulings stay as they are.
    File(PathBuf),
    /// A gzip-compressed JSON Lines file of cards; the stored rulings stay as they are.
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

/// One record of the cards file: its printing, and its catalog row when it describes a card.
struct CardRecord {
    printing: BulkPrinting,
    card: Option<CardData>,
}

fn card_record(value: Value) -> Option<CardRecord> {
    let printing = printings::from_bulk(&value)?;
    let card = serde_json::from_value::<ScryfallCard>(value)
        .ok()
        .as_ref()
        .and_then(card_data::from_scryfall);
    Some(CardRecord { printing, card })
}

/// One record of the rulings file.
#[derive(Deserialize)]
struct BulkRuling {
    oracle_id: String,
    #[serde(flatten)]
    ruling: Ruling,
}

fn ruling_record(value: Value) -> Option<BulkRuling> {
    serde_json::from_value(value).ok()
}

type Batch<T> = Result<Vec<T>, String>;

/// Reads records on a blocking thread and hands them over in batches. A line that is not
/// a JSON object fails the run; an object `record` rejects (a card without an id, a ruling
/// without a comment) is skipped.
fn read_batches<R: std::io::BufRead, T>(
    lines: JsonLines<R, Value>,
    sender: &mpsc::Sender<Batch<T>>,
    record: fn(Value) -> Option<T>,
) {
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for line in lines {
        let value = match line {
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
        let Some(row) = record(value) else {
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

fn spawn_reader<T: Send + 'static>(
    path: &Path,
    compressed: bool,
    record: fn(Value) -> Option<T>,
) -> Result<mpsc::Receiver<Batch<T>>, String> {
    let file = File::open(path)
        .map_err(|error| format!("could not stream {}: {error}", path.display()))?;
    let (sender, receiver) = mpsc::channel(4);
    tokio::task::spawn_blocking(move || {
        if compressed {
            read_batches(JsonLines::gzip(file), &sender, record);
        } else {
            read_batches(JsonLines::new(BufReader::new(file)), &sender, record);
        }
    });
    Ok(receiver)
}

/// Stores the batch's printings for sync `sync`, and the best printing of each card in the
/// batch replaces the staged one when its selection key is greater.
async fn stage_batch(pool: &Pool, records: Vec<CardRecord>, sync: i64) -> Result<(), sqlx::Error> {
    let mut candidates: HashMap<String, CardData> = HashMap::new();
    let mut tx = db::begin(pool).await?;
    for record in records {
        printings::store(&mut tx, &record.printing, sync).await?;
        let Some(row) = record.card else {
            continue;
        };
        match candidates.get(&row.oracle_id) {
            Some(best) if best.selection_key >= row.selection_key => {}
            _ => {
                candidates.insert(row.oracle_id.clone(), row);
            }
        }
    }
    let oracle_ids: Vec<&String> = candidates.keys().collect();
    let ids_json = serde_json::to_string(&oracle_ids).unwrap_or_else(|_| "[]".to_owned());
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

/// Replaces `cards` with the staged generation, prunes the printings sync `sync` did not
/// list, and, when `rulings` were staged, replaces the rulings, in one transaction.
async fn publish(pool: &Pool, sync: i64, rulings: bool) -> Result<(), sqlx::Error> {
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
    printings::prune(&mut tx, sync).await?;
    if rulings {
        sqlx::query!("DELETE FROM card_rulings")
            .execute(&mut *tx)
            .await?;
        sqlx::query!(
            "INSERT INTO card_rulings (id, oracle_id, source, published_at, comment)
             SELECT id, oracle_id, source, published_at, comment FROM card_rulings_staging"
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Stages the cards file's catalog rows and stores its printings, returning how many cards
/// were staged.
async fn stage_cards(pool: &Pool, path: &Path, compressed: bool, sync: i64) -> Result<i64, String> {
    let mut batches = spawn_reader(path, compressed, card_record)?;
    sqlx::query!("DELETE FROM catalog_cards_staging")
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    while let Some(batch) = batches.recv().await {
        stage_batch(pool, batch?, sync)
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
    Ok(count)
}

async fn stage_ruling_batch(pool: &Pool, rulings: Vec<BulkRuling>) -> Result<(), sqlx::Error> {
    let mut tx = db::begin(pool).await?;
    for BulkRuling { oracle_id, ruling } in rulings {
        let published_at = ruling.published_at.as_deref().and_then(IsoDate::parse);
        sqlx::query!(
            "INSERT INTO card_rulings_staging (oracle_id, source, published_at, comment) VALUES (?, ?, ?, ?)",
            oracle_id,
            ruling.source,
            published_at,
            ruling.comment
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Stages the gzip-compressed rulings file in file order.
async fn stage_rulings(pool: &Pool, path: &Path) -> Result<(), String> {
    let mut batches = spawn_reader(path, true, ruling_record)?;
    sqlx::query!("DELETE FROM card_rulings_staging")
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    while let Some(batch) = batches.recv().await {
        stage_ruling_batch(pool, batch?)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
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
/// records the run, and links catalog references afterwards. Printings are stored as they
/// stream, so a failed run may refresh some of them, but only a complete run removes the
/// printings it no longer lists.
/// Returns the number of cards published, or the failure message. A failure never touches
/// the published cards or rulings.
pub async fn run(pool: &Pool, scryfall: &Scryfall, source: Source) -> Result<i64, String> {
    let state = start_state(pool).await.map_err(|error| error.to_string())?;
    let mut temporary = Vec::new();
    let result = async {
        let (cards, compressed, rulings, updated_at) = match &source {
            Source::Scryfall => {
                let dir = std::env::temp_dir();
                let (cards, updated_at) = scryfall
                    .download_bulk(&dir, "default_cards")
                    .await
                    .map_err(|error| format!("{error:#}"))?;
                temporary.push(cards.clone());
                let (rulings, _) = scryfall
                    .download_bulk(&dir, "rulings")
                    .await
                    .map_err(|error| format!("{error:#}"))?;
                temporary.push(rulings.clone());
                (cards, true, Some(rulings), updated_at)
            }
            Source::File(path) => (path.clone(), false, None, None),
            Source::GzipFile(path) => (path.clone(), true, None, None),
        };
        let count = stage_cards(pool, &cards, compressed, state).await?;
        if let Some(rulings) = &rulings {
            stage_rulings(pool, rulings).await?;
        }
        publish(pool, state, rulings.is_some())
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((count, updated_at))
    }
    .await;
    for path in temporary {
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
