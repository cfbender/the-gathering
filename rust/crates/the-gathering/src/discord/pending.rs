//! Durable staging for winnerless Discord reports (`PendingGame`, `StageReport`,
//! `ResolvePendingGame`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::SqliteConnection;

use crate::db::{self, Pool, UtcDateTime};
use crate::error::Errors;

use super::report::{GameReport, ReportPlayer};
use super::sink::{Sink, SinkError};

/// How long staged games are kept (`@pending_retention_days`).
pub const RETENTION_DAYS: i64 = 30;

/// A staged game (`pending_discord_games`).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingGame {
    /// Primary key.
    pub id: i64,
    /// `spellbot:SB12345`.
    pub external_id: String,
    /// Guild.
    pub guild_id: String,
    /// Channel.
    pub channel_id: String,
    /// Start time.
    pub played_at: UtcDateTime,
    /// The stored `players` map (`{"seats": [...]}`), as JSON text.
    pub players_json: String,
    /// Provenance.
    pub raw: Map<String, Value>,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated (pruning age).
    pub updated_at: UtcDateTime,
}

/// The `players` column.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StagedPlayers {
    /// Players in seat order.
    #[serde(default)]
    pub seats: Vec<StagedPlayer>,
}

/// A stored player; fields are nullable because the column is a free-form map.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StagedPlayer {
    /// Discord id.
    #[serde(default)]
    pub discord_id: Option<String>,
    /// Display name.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Commander.
    #[serde(default)]
    pub commander_name: Option<String>,
}

impl PendingGame {
    /// The players in seat order (`ResolvePendingGame.decode_players/1`).
    pub fn players(&self) -> Vec<ReportPlayer> {
        serde_json::from_str::<StagedPlayers>(&self.players_json)
            .unwrap_or_default()
            .seats
            .into_iter()
            .map(|player| ReportPlayer {
                discord_id: player.discord_id.unwrap_or_default(),
                display_name: player.display_name.unwrap_or_default(),
                commander_name: player.commander_name,
            })
            .collect()
    }

    /// `Discord.pending_report/1`.
    pub fn report(&self) -> GameReport {
        GameReport {
            external_id: self.external_id.clone(),
            source: "discord".into(),
            played_at: self.played_at,
            guild_id: self.guild_id.clone(),
            channel_id: self.channel_id.clone(),
            players: self.players(),
            winner_discord_ids: Vec::new(),
            raw: self.raw.clone(),
            details: None,
        }
    }

    /// Whether `discord_id` played.
    pub fn has_player(&self, discord_id: &str) -> bool {
        self.players()
            .iter()
            .any(|player| player.discord_id == discord_id)
    }

    /// A digest of the roster and identity a draft was opened against. Replaces Elixir's
    /// SHA-256 of `:erlang.term_to_binary/1`, so drafts written by the Elixir server never
    /// match and read as expired.
    pub fn snapshot(&self) -> Vec<u8> {
        use sha2::Digest;
        let players: Value = serde_json::from_str(&self.players_json).unwrap_or(Value::Null);
        let canonical = serde_json::json!([
            players,
            self.played_at.to_ecto_string(),
            self.guild_id,
            self.channel_id
        ]);
        sha2::Sha256::digest(canonical.to_string().as_bytes()).to_vec()
    }
}

struct Row {
    id: i64,
    external_id: String,
    guild_id: String,
    channel_id: String,
    played_at: UtcDateTime,
    players: String,
    raw: String,
    inserted_at: UtcDateTime,
    updated_at: UtcDateTime,
}

impl From<Row> for PendingGame {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            external_id: row.external_id,
            guild_id: row.guild_id,
            channel_id: row.channel_id,
            played_at: row.played_at,
            players_json: row.players,
            raw: serde_json::from_str(&row.raw).unwrap_or_default(),
            inserted_at: row.inserted_at,
            updated_at: row.updated_at,
        }
    }
}

macro_rules! select_pending {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            Row,
            r#"SELECT id AS "id!: i64", external_id, guild_id, channel_id,
                      played_at AS "played_at: UtcDateTime", players, raw,
                      inserted_at AS "inserted_at: UtcDateTime", updated_at AS "updated_at: UtcDateTime"
               FROM pending_discord_games AS pending "# + $tail
            $(, $arg)*
        )
    };
}

/// `Discord.list_pending/0`: winnerless games, newest first.
pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<PendingGame>, sqlx::Error> {
    Ok(select_pending!(
        "WHERE NOT EXISTS (SELECT 1 FROM games WHERE games.source = 'discord' AND games.external_id = pending.external_id)
         ORDER BY played_at DESC, id DESC"
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(PendingGame::from)
    .collect())
}

/// `Repo.get(PendingGame, id)`.
pub async fn get(conn: &mut SqliteConnection, id: i64) -> Result<Option<PendingGame>, sqlx::Error> {
    Ok(select_pending!("WHERE id = ?", id)
        .fetch_optional(&mut *conn)
        .await?
        .map(PendingGame::from))
}

/// `Discord.get_pending_by_external_id/1` (recorded games included).
pub async fn by_external_id(
    conn: &mut SqliteConnection,
    external_id: &str,
) -> Result<Option<PendingGame>, sqlx::Error> {
    Ok(select_pending!("WHERE external_id = ?", external_id)
        .fetch_optional(&mut *conn)
        .await?
        .map(PendingGame::from))
}

/// `Discord.latest_pending_in_channel/1`: the most recently started winnerless game.
pub async fn latest_in_channel(
    conn: &mut SqliteConnection,
    channel_id: &str,
) -> Result<Option<PendingGame>, sqlx::Error> {
    Ok(select_pending!(
        "WHERE channel_id = ?
           AND NOT EXISTS (SELECT 1 FROM games WHERE games.source = 'discord' AND games.external_id = pending.external_id)
         ORDER BY played_at DESC, id DESC LIMIT 1",
        channel_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(PendingGame::from))
}

/// Whether the game was already recorded from Discord.
pub async fn recorded(conn: &mut SqliteConnection, external_id: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM games WHERE source = 'discord' AND external_id = ?) AS "e!: bool""#,
        external_id
    )
    .fetch_one(&mut *conn)
    .await
}

/// Why staging failed.
#[derive(Debug, thiserror::Error)]
pub enum StageError {
    /// A required field is blank (`validate_required`).
    #[error("invalid pending game")]
    Invalid(Errors),
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// `StageReport.run/1`: stages a report, replacing the normalized data when SpellBot edits
/// the same game.
pub async fn stage(
    conn: &mut SqliteConnection,
    report: &GameReport,
) -> Result<PendingGame, StageError> {
    let mut errors = Errors::new();
    for (field, value) in [
        ("external_id", &report.external_id),
        ("guild_id", &report.guild_id),
        ("channel_id", &report.channel_id),
    ] {
        if value.trim().is_empty() {
            errors.add(field, "can't be blank");
        }
    }
    errors.into_result().map_err(StageError::Invalid)?;
    let players = serde_json::json!({ "seats": report.players }).to_string();
    let raw = Value::Object(report.raw.clone()).to_string();
    let now = UtcDateTime::now();
    let id = sqlx::query_scalar!(
        r#"INSERT INTO pending_discord_games (external_id, guild_id, channel_id, played_at, players, raw, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT (external_id) DO UPDATE SET guild_id = excluded.guild_id, channel_id = excluded.channel_id,
             played_at = excluded.played_at, players = excluded.players, raw = excluded.raw, updated_at = excluded.updated_at
           RETURNING id AS "id!: i64""#,
        report.external_id,
        report.guild_id,
        report.channel_id,
        report.played_at,
        players,
        raw,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await?;
    get(conn, id)
        .await?
        .ok_or(StageError::Database(sqlx::Error::RowNotFound))
}

/// Deletes a staged game (its drafts cascade).
pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM pending_discord_games WHERE id = ?", id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// `Discord.prune_pending/1`: drops games not updated for [`RETENTION_DAYS`].
pub async fn prune(conn: &mut SqliteConnection, now: UtcDateTime) -> Result<(), sqlx::Error> {
    let cutoff = now.plus(time::Duration::days(-RETENTION_DAYS));
    sqlx::query!(
        "DELETE FROM pending_discord_games WHERE updated_at < ?",
        cutoff
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Why resolving a staged game failed.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// No such staged game.
    #[error("unknown game")]
    UnknownGame,
    /// No winnerless game staged in the channel.
    #[error("no game in channel")]
    NoGameInChannel,
    /// The reported winner did not play.
    #[error("not a player")]
    NotAPlayer,
    /// The sink rejected the completed report; nothing was written.
    #[error("sink failed: {0}")]
    SinkFailed(SinkError),
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// `ResolvePendingGame.run/3`: records `discord_id` as the winner and consumes the staged
/// game, atomically.
pub async fn resolve(
    pool: &Pool,
    pending: &PendingGame,
    discord_id: &str,
    sink: &dyn Sink,
) -> Result<GameReport, ResolveError> {
    if !pending.has_player(discord_id) {
        return Err(ResolveError::NotAPlayer);
    }
    let mut completed = pending.report();
    completed.winner_discord_ids = vec![discord_id.to_owned()];
    completed.raw.insert(
        "winner_reported_by".into(),
        Value::String(discord_id.to_owned()),
    );
    let mut tx = db::begin(pool).await?;
    match sink.handle_report(&mut tx, &completed).await {
        Ok(()) => {
            delete(&mut tx, pending.id).await?;
            tx.commit().await?;
            Ok(completed)
        }
        Err(error) => {
            tx.rollback().await?;
            Err(ResolveError::SinkFailed(error))
        }
    }
}
