//! Resolving a player by Discord identity or name (`TheGathering.Games.ResolvePlayer`).
//!
//! Discord identities match only by `discord_id`; when none matches, a distinct name is
//! chosen. Name matching is used only when `discord_id` is absent.

use std::collections::HashSet;

use crate::changeset::Changeset;
use sqlx::SqliteConnection;

use crate::db::UtcDateTime;
use crate::error::Errors;

use super::fold_name;

/// Why resolving failed.
#[derive(Debug)]
pub enum ResolveError {
    /// The Discord identity belongs to a player linked to another account.
    DiscordIdentityConflict,
    /// The new player is invalid.
    Invalid(Errors),
    /// Database error.
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ResolveError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// A player row's identity columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerIdentity {
    /// Primary key.
    pub id: i64,
    /// Display name.
    pub name: String,
    /// Linked account.
    pub user_id: Option<i64>,
    /// Discord snowflake.
    pub discord_id: Option<String>,
}

fn normalize_discord_id(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn normalize_name(name: &str) -> String {
    name.trim().chars().take(100).collect()
}

async fn find_player(conn: &mut SqliteConnection, name: &str, discord_id: Option<&str>) -> Result<Option<PlayerIdentity>, sqlx::Error> {
    match discord_id {
        Some(discord_id) => {
            sqlx::query_as!(
                PlayerIdentity,
                r#"SELECT id AS "id!", name, user_id, discord_id FROM players WHERE discord_id = ?"#,
                discord_id
            )
            .fetch_optional(&mut *conn)
            .await
        }
        None => {
            let folded = fold_name(name);
            sqlx::query_as!(
                PlayerIdentity,
                r#"SELECT id AS "id!", name, user_id, discord_id FROM players WHERE lower(name) = ?"#,
                folded
            )
            .fetch_optional(&mut *conn)
            .await
        }
    }
}

async fn name_taken(conn: &mut SqliteConnection, folded: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM players WHERE lower(name) = ?) AS "taken!: bool""#, folded)
        .fetch_one(&mut *conn)
        .await
}

fn with_suffix(base: &str, suffix: &str) -> String {
    let keep = 100usize.saturating_sub(suffix.chars().count());
    let mut name: String = base.chars().take(keep).collect();
    name.push_str(suffix);
    name
}

async fn available_name(
    conn: &mut SqliteConnection,
    name: &str,
    discord_id: Option<&str>,
    reserved: &HashSet<String>,
) -> Result<String, sqlx::Error> {
    let base = normalize_name(name);
    if discord_id.is_none() {
        return Ok(base);
    }
    let mut candidate = base.clone();
    let mut suffix = 2u32;
    loop {
        let folded = fold_name(&candidate);
        if !reserved.contains(&folded) && !name_taken(conn, &folded).await? {
            return Ok(candidate);
        }
        candidate = with_suffix(&base, &format!(" ({suffix})"));
        suffix = suffix.saturating_add(1);
    }
}

async fn user_exists(conn: &mut SqliteConnection, user_id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = ?) AS "exists!: bool""#, user_id)
        .fetch_one(&mut *conn)
        .await
}

/// `Games.resolve_player/3` inside a transaction.
pub async fn run(
    conn: &mut SqliteConnection,
    name: &str,
    discord_id: Option<&str>,
    user_id: Option<i64>,
) -> Result<PlayerIdentity, ResolveError> {
    let discord_id = normalize_discord_id(discord_id);
    if let Some(player) = find_player(conn, name, discord_id).await? {
        return match (player.user_id, user_id) {
            (_, None) => Ok(player),
            (Some(current), Some(wanted)) if current == wanted => Ok(player),
            (None, Some(wanted)) => {
                if !user_exists(conn, wanted).await? {
                    return Err(ResolveError::Invalid(Errors::single("user_id", "does not exist")));
                }
                let now = UtcDateTime::now();
                sqlx::query!("UPDATE players SET user_id = ?, updated_at = ? WHERE id = ?", wanted, now, player.id)
                    .execute(&mut *conn)
                    .await
                    .map_err(|error| unique_error(error, "user_id"))?;
                Ok(PlayerIdentity { user_id: Some(wanted), ..player })
            }
            (Some(_), Some(_)) => Err(ResolveError::DiscordIdentityConflict),
        };
    }
    let name = available_name(conn, name, discord_id, &HashSet::new()).await?;
    insert_player(conn, &name, discord_id, user_id).await
}

fn unique_error(error: sqlx::Error, field: &str) -> ResolveError {
    if crate::db::is_unique_violation(&error, &[]) {
        ResolveError::Invalid(Errors::single(field, crate::changeset::TAKEN))
    } else {
        ResolveError::Database(error)
    }
}

/// Inserts a player after `Player.changeset/2` validation.
pub async fn insert_player(
    conn: &mut SqliteConnection,
    name: &str,
    discord_id: Option<&str>,
    user_id: Option<i64>,
) -> Result<PlayerIdentity, ResolveError> {
    let name = name.trim().to_owned();
    let mut cs = Changeset::empty();
    cs.required("name", Some(&name));
    cs.length("name", Some(&name), Some(1), Some(100));
    if let Some(user_id) = user_id
        && !user_exists(conn, user_id).await?
    {
        cs.add_error("user_id", "does not exist");
    }
    cs.finish().map_err(ResolveError::Invalid)?;
    let now = UtcDateTime::now();
    let id = sqlx::query_scalar!(
        r#"INSERT INTO players (name, user_id, discord_id, inserted_at, updated_at) VALUES (?, ?, ?, ?, ?)
           RETURNING id AS "id!: i64""#,
        name,
        user_id,
        discord_id,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|error| {
        if crate::db::is_unique_violation(&error, &["players.user_id"]) {
            ResolveError::Invalid(Errors::single("user_id", crate::changeset::TAKEN))
        } else if crate::db::is_unique_violation(&error, &["players.discord_id"]) {
            ResolveError::Invalid(Errors::single("discord_id", crate::changeset::TAKEN))
        } else {
            unique_error(error, "name")
        }
    })?;
    Ok(PlayerIdentity { id, name, user_id, discord_id: discord_id.map(str::to_owned) })
}

/// How an identity would resolve, without writing (`ResolvePlayer.preview/1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// An existing player.
    Matched(PlayerIdentity),
    /// A new player with this name.
    Create(String),
}

/// `Games.preview_player_resolutions/1`: later identities see names earlier ones reserved.
pub async fn preview(conn: &mut SqliteConnection, identities: &[(String, Option<String>)]) -> Result<Vec<Resolution>, sqlx::Error> {
    let mut reserved = HashSet::new();
    let mut resolutions = Vec::with_capacity(identities.len());
    for (name, discord_id) in identities {
        let discord_id = normalize_discord_id(discord_id.as_deref());
        match find_player(conn, name, discord_id).await? {
            Some(player) => resolutions.push(Resolution::Matched(player)),
            None => {
                let available = available_name(conn, name, discord_id, &reserved).await?;
                reserved.insert(fold_name(&available));
                resolutions.push(Resolution::Create(available));
            }
        }
    }
    Ok(resolutions)
}
