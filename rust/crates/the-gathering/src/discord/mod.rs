//! The optional Discord bot and staged Discord reports.
//!
//! - SpellBot's "game ready" embeds are parsed ([`spellbot`]) and staged as pending games
//!   ([`pending`], [`tracker`]); a winner completes them through a [`sink::Sink`].
//! - `/log` hands a staged game to the web form ([`log_command`], [`web_draft`]); the
//!   legacy `/won` form ([`won`], [`won_form`], [`won_report`]) finishes it in Discord.
//! - `/summary` posts a rendered recap ([`summary`]).
//! - `/newgame` gathers players for a webcam table ([`new_game`], [`scheduled`],
//!   [`scheduler`]).
//!
//! The gateway is `twilight-gateway`, with `twilight-model` types for inbound events.
//! Outbound calls go through the [`api::DiscordApi`] trait ([`rest::RestApi`] in
//! production), so every handler runs in tests without a gateway.

pub mod api;
pub mod bot;
pub mod card_choice;
pub mod command;
pub mod draft;
pub mod interaction;
pub mod log_command;
pub mod new_game;
pub mod new_game_message;
pub mod pending;
pub mod report;
pub mod rest;
pub mod scheduled;
pub mod scheduler;
pub mod sink;
pub mod spellbot;
pub mod start_time;
pub mod summary;
pub mod tracker;
pub mod web_draft;
pub mod won;
pub mod won_form;
pub mod won_report;

use sqlx::SqliteConnection;

use crate::db::{self, Pool, UtcDateTime};
use crate::state::AppState;

pub use self::bot::{Bot, start};
pub use self::pending::{PendingGame, ResolveError, StageError};
pub use self::report::{GameReport, ReportPlayer};
pub use self::sink::{GamesSink, Sink, SinkError};

/// Who invoked a command, as strings (`""` when absent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Actor {
    /// Discord id.
    pub discord_id: String,
    /// Guild.
    pub guild_id: String,
    /// Channel.
    pub channel_id: String,
}

/// The guild commands are restricted to (`DISCORD_GUILD_ID`), if any.
pub fn configured_guild(state: &AppState) -> Option<String> {
    state
        .config
        .discord_bot
        .as_ref()
        .and_then(|bot| bot.guild_id.clone())
        .filter(|guild| !guild.is_empty())
}

/// Whether the account linked to `discord_id` is disabled (`None` without an account).
pub async fn account_disabled(
    conn: &mut SqliteConnection,
    discord_id: &str,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT disabled_at IS NOT NULL AS "disabled!: bool" FROM users WHERE discord_id = ?"#,
        discord_id
    )
    .fetch_optional(&mut *conn)
    .await
}

/// `Discord.stage_report/1`.
pub async fn stage_report(pool: &Pool, report: &GameReport) -> Result<PendingGame, StageError> {
    let mut tx = db::begin(pool).await?;
    let pending = pending::stage(&mut tx, report).await?;
    tx.commit().await?;
    Ok(pending)
}

/// `Discord.list_pending/0`: winnerless staged games, newest first (never prunes).
pub async fn list_pending(pool: &Pool) -> Result<Vec<PendingGame>, sqlx::Error> {
    pending::list(&mut *pool.acquire().await?).await
}

/// `Discord.get_pending_by_external_id/1`.
pub async fn get_pending_by_external_id(
    pool: &Pool,
    external_id: &str,
) -> Result<Option<PendingGame>, sqlx::Error> {
    pending::by_external_id(&mut *pool.acquire().await?, external_id).await
}

/// `Discord.latest_pending_in_channel/1`.
pub async fn latest_pending_in_channel(
    pool: &Pool,
    channel_id: &str,
) -> Result<Option<PendingGame>, sqlx::Error> {
    pending::latest_in_channel(&mut *pool.acquire().await?, channel_id).await
}

/// `Discord.resolve_pending/3` by id.
pub async fn resolve_pending(
    pool: &Pool,
    id: i64,
    discord_id: &str,
    sink: &dyn Sink,
) -> Result<GameReport, ResolveError> {
    let found = pending::get(&mut *pool.acquire().await?, id).await?;
    let pending = found.ok_or(ResolveError::UnknownGame)?;
    pending::resolve(pool, &pending, discord_id, sink).await
}

/// `Discord.discard_pending/1`: `false` when there is no such game.
pub async fn discard_pending(pool: &Pool, id: i64) -> Result<bool, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    if pending::get(&mut conn, id).await?.is_none() {
        return Ok(false);
    }
    pending::delete(&mut conn, id).await?;
    Ok(true)
}

/// `Discord.prune_pending/1`: drops staged games older than 30 days.
pub async fn prune_pending(pool: &Pool, now: UtcDateTime) -> Result<(), sqlx::Error> {
    pending::prune(&mut *pool.acquire().await?, now).await
}
