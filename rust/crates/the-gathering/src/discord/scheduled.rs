//! `/newgame` queues (`discord_scheduled_games`) and their durable transitions
//! (`ScheduledGame`, `ScheduledGames`). SQLite immediate transactions serialize roster
//! changes; status changes are compare-and-set so a stale row never starts twice.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::db::{self, Pool, UtcDateTime};
use crate::state::AppState;

use super::configured_guild;

/// How long an underfilled game stays open after pinging its maybe list.
pub const MAYBE_GRACE_SECONDS: i64 = 15 * 60;

/// Most players (and maybes) a queue lists.
pub const MAX_LISTED: usize = 10;

/// Queue status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Taking players.
    Open,
    /// Filled; the lobby link was posted.
    Started,
    /// Did not fill in time.
    Expired,
    /// Cancelled by the host or an administrator.
    Cancelled,
}

impl Status {
    /// The stored value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Started => "started",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "started" => Self::Started,
            "expired" => Self::Expired,
            "cancelled" => Self::Cancelled,
            _ => Self::Open,
        }
    }
}

/// A roster or maybe-list entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// When they first joined this list (ISO 8601).
    #[serde(default)]
    pub joined_at: String,
    /// Their name when they last clicked.
    #[serde(default)]
    pub display_name: String,
}

/// A list keyed by Discord id.
pub type Roster = BTreeMap<String, Entry>;

/// A queue (`discord_scheduled_games`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledGame {
    /// Primary key.
    pub id: i64,
    /// Guild.
    pub guild_id: String,
    /// Channel.
    pub channel_id: String,
    /// The public queue message, once posted.
    pub message_id: Option<String>,
    /// Who ran `/newgame`.
    pub host_discord_id: String,
    /// Title.
    pub title: String,
    /// Format (Commander when absent).
    pub format: Option<String>,
    /// Start time; `None` starts when filled.
    pub start_at: Option<UtcDateTime>,
    /// Players needed.
    pub min_players: i64,
    /// Status.
    pub status: Status,
    /// Webcam table id, once started.
    pub room_id: Option<String>,
    /// Roster.
    pub players: Roster,
    /// Maybe list (never counted).
    pub maybe: Roster,
    /// When the maybe list was pinged.
    pub maybe_pinged_at: Option<UtcDateTime>,
    /// The ping message.
    pub maybe_ping_id: Option<String>,
    /// The "ready" announcement.
    pub announcement_id: Option<String>,
    /// The public message needs an edit.
    pub message_dirty: bool,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
}

struct Row {
    id: i64,
    guild_id: String,
    channel_id: String,
    message_id: Option<String>,
    host_discord_id: String,
    title: String,
    format: Option<String>,
    start_at: Option<UtcDateTime>,
    min_players: i64,
    status: String,
    room_id: Option<String>,
    players: String,
    maybe: String,
    maybe_pinged_at: Option<UtcDateTime>,
    maybe_ping_id: Option<String>,
    announcement_id: Option<String>,
    message_dirty: bool,
    inserted_at: UtcDateTime,
    updated_at: UtcDateTime,
}

impl From<Row> for ScheduledGame {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            guild_id: row.guild_id,
            channel_id: row.channel_id,
            message_id: row.message_id,
            host_discord_id: row.host_discord_id,
            title: row.title,
            format: row.format,
            start_at: row.start_at,
            min_players: row.min_players,
            status: Status::parse(&row.status),
            room_id: row.room_id,
            players: serde_json::from_str(&row.players).unwrap_or_default(),
            maybe: serde_json::from_str(&row.maybe).unwrap_or_default(),
            maybe_pinged_at: row.maybe_pinged_at,
            maybe_ping_id: row.maybe_ping_id,
            announcement_id: row.announcement_id,
            message_dirty: row.message_dirty,
            inserted_at: row.inserted_at,
            updated_at: row.updated_at,
        }
    }
}

/// Who clicked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueActor {
    /// Discord id.
    pub discord_id: String,
    /// Guild.
    pub guild_id: String,
    /// Channel.
    pub channel_id: String,
    /// The message the button is on.
    pub message_id: String,
    /// Nickname, display name, or username.
    pub display_name: String,
    /// Holds the Administrator permission.
    pub admin: bool,
}

/// A queue action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueAction {
    /// Join the roster.
    Join,
    /// Join the maybe list.
    Maybe,
    /// Leave both lists.
    Leave,
    /// Cancel (host or administrator).
    Cancel,
    /// Change the start time (host or administrator).
    Time(Option<UtcDateTime>),
}

/// Why a queue action failed.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// Wrong server, channel, or message, or not allowed.
    #[error("forbidden")]
    Forbidden,
    /// Ten players already.
    #[error("full")]
    Full,
    /// Ten maybes already.
    #[error("maybe_full")]
    MaybeFull,
    /// Invalid title, format, or minimum.
    #[error("invalid")]
    Invalid,
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The options `/newgame` was given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewQueue {
    /// Title (default "Commander game").
    pub title: Option<String>,
    /// Format.
    pub format: Option<String>,
    /// Start time.
    pub start_at: Option<UtcDateTime>,
    /// Minimum players (default 3).
    pub min_players: Option<i64>,
}

macro_rules! select_games {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            Row,
            r#"SELECT id AS "id!: i64", guild_id, channel_id, message_id, host_discord_id, title, format,
                      start_at AS "start_at: UtcDateTime", min_players, status, room_id, players, maybe,
                      maybe_pinged_at AS "maybe_pinged_at: UtcDateTime", maybe_ping_id, announcement_id,
                      message_dirty AS "message_dirty: bool",
                      inserted_at AS "inserted_at: UtcDateTime", updated_at AS "updated_at: UtcDateTime"
               FROM discord_scheduled_games "# + $tail
            $(, $arg)*
        )
    };
}

/// A queue by id.
pub async fn get(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<ScheduledGame>, sqlx::Error> {
    Ok(select_games!("WHERE id = ?", id)
        .fetch_optional(&mut *conn)
        .await?
        .map(ScheduledGame::from))
}

async fn get_required(conn: &mut SqliteConnection, id: i64) -> Result<ScheduledGame, sqlx::Error> {
    get(conn, id).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Every queue (tests and diagnostics).
pub async fn all(pool: &Pool) -> Result<Vec<ScheduledGame>, sqlx::Error> {
    Ok(select_games!("ORDER BY id")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(ScheduledGame::from)
        .collect())
}

fn authorize(state: &AppState, actor: &QueueActor) -> Result<(), QueueError> {
    let allowed = !actor.guild_id.is_empty()
        && !actor.discord_id.is_empty()
        && configured_guild(state).is_none_or(|guild| guild == actor.guild_id);
    if allowed {
        Ok(())
    } else {
        Err(QueueError::Forbidden)
    }
}

/// `ScheduledGames.create/2`: a queue hosted by `actor` (not yet published).
pub async fn create(
    state: &AppState,
    queue: &NewQueue,
    actor: &QueueActor,
) -> Result<ScheduledGame, QueueError> {
    authorize(state, actor)?;
    // Ecto casts "" to nil, so a blank title fails `validate_required`.
    let title = match &queue.title {
        None => "Commander game".to_owned(),
        Some(title) if title.trim().is_empty() => return Err(QueueError::Invalid),
        Some(title) => title.clone(),
    };
    let format = queue
        .format
        .clone()
        .filter(|format| !format.trim().is_empty());
    let min_players = queue.min_players.unwrap_or(3);
    if title.chars().count() > 100
        || format
            .as_ref()
            .is_some_and(|format| format.chars().count() > 100)
        || !(2..=10).contains(&min_players)
    {
        return Err(QueueError::Invalid);
    }
    let now = UtcDateTime::now();
    let mut conn = state.pool.acquire().await?;
    let id = sqlx::query_scalar!(
        r#"INSERT INTO discord_scheduled_games (guild_id, channel_id, host_discord_id, title, format, start_at,
             min_players, status, players, maybe, message_dirty, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, 'open', '{}', '{}', 1, ?, ?) RETURNING id AS "id!: i64""#,
        actor.guild_id,
        actor.channel_id,
        actor.discord_id,
        title,
        format,
        queue.start_at,
        min_players,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(get_required(&mut conn, id).await?)
}

/// `ScheduledGames.attach_message/2`.
pub async fn attach_message(
    pool: &Pool,
    id: i64,
    message_id: &str,
) -> Result<ScheduledGame, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE discord_scheduled_games SET message_id = ?, updated_at = ? WHERE id = ?",
        message_id,
        now,
        id
    )
    .execute(&mut *conn)
    .await?;
    get_required(&mut conn, id).await
}

/// `ScheduledGames.cancel_unpublished/1`.
pub async fn cancel_unpublished(pool: &Pool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE discord_scheduled_games SET status = 'cancelled', message_dirty = 0 WHERE id = ? AND message_id IS NULL",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn same_message(game: &ScheduledGame, actor: &QueueActor) -> bool {
    game.guild_id == actor.guild_id
        && game.channel_id == actor.channel_id
        && game.message_id.as_deref() == Some(actor.message_id.as_str())
}

fn host_or_admin(game: &ScheduledGame, actor: &QueueActor) -> bool {
    actor.discord_id == game.host_discord_id || actor.admin
}

/// `ScheduledGames.manageable?/2`: read-only check before opening the change-time modal;
/// [`act`] re-checks on submit.
pub async fn manageable(
    state: &AppState,
    id: i64,
    actor: &QueueActor,
) -> Result<bool, sqlx::Error> {
    if authorize(state, actor).is_err() {
        return Ok(false);
    }
    let game = get(&mut *state.pool.acquire().await?, id).await?;
    Ok(game.is_some_and(|game| {
        game.status == Status::Open && same_message(&game, actor) && host_or_admin(&game, actor)
    }))
}

/// `ScheduledGames.act/4`: applies an action and settles the queue. A queue that is no
/// longer open is returned unchanged.
pub async fn act(
    state: &AppState,
    id: i64,
    action: &QueueAction,
    actor: &QueueActor,
    now: UtcDateTime,
) -> Result<ScheduledGame, QueueError> {
    authorize(state, actor)?;
    let mut tx = db::begin(&state.pool).await?;
    let result = act_in(&mut tx, id, action, actor, now).await;
    match result {
        Ok(game) => {
            tx.commit().await?;
            Ok(game)
        }
        Err(error) => {
            tx.rollback().await?;
            Err(error)
        }
    }
}

async fn act_in(
    conn: &mut SqliteConnection,
    id: i64,
    action: &QueueAction,
    actor: &QueueActor,
    now: UtcDateTime,
) -> Result<ScheduledGame, QueueError> {
    let game = get(conn, id).await?.ok_or(QueueError::Forbidden)?;
    if !same_message(&game, actor) {
        return Err(QueueError::Forbidden);
    }
    let mut game = settle(conn, game, now).await?;
    if game.status != Status::Open {
        return Ok(game);
    }
    match action {
        QueueAction::Join => {
            if full(&game.players, actor) {
                return Err(QueueError::Full);
            }
            put_entry(&mut game.players, actor, now);
            game.maybe.remove(&actor.discord_id);
        }
        QueueAction::Maybe => {
            if full(&game.maybe, actor) {
                return Err(QueueError::MaybeFull);
            }
            put_entry(&mut game.maybe, actor, now);
            game.players.remove(&actor.discord_id);
        }
        QueueAction::Leave => {
            game.players.remove(&actor.discord_id);
            game.maybe.remove(&actor.discord_id);
        }
        QueueAction::Cancel => {
            if !host_or_admin(&game, actor) {
                return Err(QueueError::Forbidden);
            }
            game.status = Status::Cancelled;
        }
        QueueAction::Time(start_at) => {
            if !host_or_admin(&game, actor) {
                return Err(QueueError::Forbidden);
            }
            game.start_at = *start_at;
            game.maybe_pinged_at = None;
            game.maybe_ping_id = None;
        }
    }
    let players = serde_json::to_string(&game.players).unwrap_or_else(|_| "{}".into());
    let maybe = serde_json::to_string(&game.maybe).unwrap_or_else(|_| "{}".into());
    let status = game.status.as_str();
    let updated_at = UtcDateTime::now();
    sqlx::query!(
        "UPDATE discord_scheduled_games SET players = ?, maybe = ?, status = ?, start_at = ?, maybe_pinged_at = ?,
           maybe_ping_id = ?, message_dirty = 1, updated_at = ? WHERE id = ?",
        players,
        maybe,
        status,
        game.start_at,
        game.maybe_pinged_at,
        game.maybe_ping_id,
        updated_at,
        game.id
    )
    .execute(&mut *conn)
    .await?;
    let game = get_required(conn, id).await?;
    Ok(settle(conn, game, now).await?)
}

fn full(list: &Roster, actor: &QueueActor) -> bool {
    list.len() >= MAX_LISTED && !list.contains_key(&actor.discord_id)
}

fn put_entry(list: &mut Roster, actor: &QueueActor, now: UtcDateTime) {
    let entry = list
        .entry(actor.discord_id.clone())
        .or_insert_with(|| Entry {
            joined_at: now.to_ecto_string(),
            display_name: String::new(),
        });
    entry.display_name.clone_from(&actor.display_name);
}

/// `ScheduledGames.settle/2` for an id, in its own transaction.
pub async fn settle_id(
    pool: &Pool,
    id: i64,
    now: UtcDateTime,
) -> Result<ScheduledGame, sqlx::Error> {
    let mut tx = db::begin(pool).await?;
    let game = get_required(&mut tx, id).await?;
    let game = settle(&mut tx, game, now).await?;
    tx.commit().await?;
    Ok(game)
}

/// `ScheduledGames.settle/2`: starts, pings the maybe list, or expires a published open
/// queue as its roster and start time require.
pub async fn settle(
    conn: &mut SqliteConnection,
    game: ScheduledGame,
    now: UtcDateTime,
) -> Result<ScheduledGame, sqlx::Error> {
    if game.status != Status::Open || game.message_id.is_none() {
        return Ok(game);
    }
    let enough = i64::try_from(game.players.len()).unwrap_or(i64::MAX) >= game.min_players;
    let due = game.start_at.is_some_and(|start| start <= now);
    if due {
        // Maybes never count toward the minimum. An underfilled game pings them once at
        // its start time and stays open for the grace period, starting as soon as it fills.
        if enough {
            transition(conn, &game, Status::Started, now).await
        } else if let Some(pinged) = game.maybe_pinged_at {
            let deadline = pinged.plus(time::Duration::seconds(MAYBE_GRACE_SECONDS));
            if deadline > now {
                Ok(game)
            } else {
                transition(conn, &game, Status::Expired, now).await
            }
        } else if !game.maybe.is_empty() {
            sqlx::query!(
                "UPDATE discord_scheduled_games SET maybe_pinged_at = ?, message_dirty = 1, updated_at = ?
                 WHERE id = ? AND status = 'open' AND maybe_pinged_at IS NULL",
                now,
                now,
                game.id
            )
            .execute(&mut *conn)
            .await?;
            get_required(conn, game.id).await
        } else {
            transition(conn, &game, Status::Expired, now).await
        }
    } else if game.start_at.is_none() && enough {
        transition(conn, &game, Status::Started, now).await
    } else {
        Ok(game)
    }
}

/// The compare-and-set status change: the final guard even if two workers settle the same
/// stale row.
async fn transition(
    conn: &mut SqliteConnection,
    game: &ScheduledGame,
    status: Status,
    now: UtcDateTime,
) -> Result<ScheduledGame, sqlx::Error> {
    let room_id = (status == Status::Started).then(|| uuid::Uuid::new_v4().to_string());
    let status = status.as_str();
    sqlx::query!(
        "UPDATE discord_scheduled_games SET status = ?, room_id = ?, message_dirty = 1, updated_at = ?
         WHERE id = ? AND status = 'open'",
        status,
        room_id,
        now,
        game.id
    )
    .execute(&mut *conn)
    .await?;
    get_required(conn, game.id).await
}

/// `ScheduledGames.pending_ids/2`: published queues needing an edit or due to settle.
pub async fn pending_ids(
    pool: &Pool,
    now: UtcDateTime,
    after_id: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM discord_scheduled_games
           WHERE id > ? AND message_id IS NOT NULL AND (message_dirty OR (status = 'open' AND start_at <= ?))
           ORDER BY id LIMIT 100"#,
        after_id,
        now
    )
    .fetch_all(pool)
    .await
}

/// Records the announcement message.
pub async fn set_announcement(pool: &Pool, id: i64, message_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE discord_scheduled_games SET announcement_id = ? WHERE id = ?",
        message_id,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Records the maybe-ping message.
pub async fn set_maybe_ping(pool: &Pool, id: i64, message_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE discord_scheduled_games SET maybe_ping_id = ? WHERE id = ?",
        message_id,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Marks the public message up to date.
pub async fn mark_clean(pool: &Pool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE discord_scheduled_games SET message_dirty = 0 WHERE id = ?",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}
