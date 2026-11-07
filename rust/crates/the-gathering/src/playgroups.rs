//! Playgroups: the circles of players a server hosts.
//!
//! Every player and game belongs to exactly one playgroup, and decks follow their player.
//! Accounts belong to any number of playgroups (`playgroup_members`) and see only those;
//! they view one at a time (`users.playgroup_id`, the one they last switched to), falling
//! back to their oldest membership when they have not chosen or lost that membership. Each
//! playgroup has its own reusable sign-up invitation, so an invite link joins the playgroup
//! it was created for. The Discord bot records a guild's games into the playgroup that
//! claims the guild, or the default one.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqliteConnection;

use crate::accounts::User;
use crate::crypto;
use crate::db::{self, Pool, UtcDateTime};
use crate::error::ApiError;
use crate::games::resolve_player::{self, ResolveError};
use crate::patch::Patch;
use crate::validation::{TAKEN, ValidationError, Validator};

/// A playgroup (`playgroups`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Playgroup {
    /// Primary key.
    pub id: i64,
    /// Name, unique case-insensitively.
    pub name: String,
    /// The Discord guild whose games the bot records here.
    pub discord_guild_id: Option<String>,
    /// Whether a sign-up invitation link is active.
    pub invite_enabled: bool,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
}

/// `POST /api/admin/playgroups` and `PATCH /api/admin/playgroups/:id` bodies.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PlaygroupInput {
    /// Name.
    #[serde(default)]
    pub name: Patch<String>,
    /// Discord guild id (snowflake); blank clears it.
    #[serde(default)]
    pub discord_guild_id: Patch<String>,
}

/// A member of a playgroup, as the administrators' list shows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Member {
    /// Account id.
    pub id: i64,
    /// Username.
    pub username: String,
    /// Display name.
    pub display_name: String,
    /// Role.
    pub role: String,
}

/// Selects playgroup rows: `select_playgroups!("WHERE id = ?", id)`.
macro_rules! select_playgroups {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            Playgroup,
            r#"SELECT id AS "id!", name, discord_guild_id,
                      registration_invite_hash IS NOT NULL AS "invite_enabled!: bool",
                      inserted_at AS "inserted_at: UtcDateTime", updated_at AS "updated_at: UtcDateTime"
               FROM playgroups "# + $tail
            $(, $arg)*
        )
    };
}

/// The 403 for an account that belongs to no playgroup.
pub fn none_error() -> ApiError {
    ApiError::Custom(
        StatusCode::FORBIDDEN,
        json!({ "errors": { "code": "no_playgroup", "detail": "You are not in a playgroup yet" } }),
    )
}

/// The default playgroup: the oldest one, which the migration that introduced playgroups
/// filled with the server's existing players and games.
pub async fn default_id(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT min(id) AS "id!: i64" FROM playgroups"#)
        .fetch_one(&mut *conn)
        .await
}

/// The playgroup that claims `guild_id`, or the default one.
pub async fn for_discord_guild(
    conn: &mut SqliteConnection,
    guild_id: &str,
) -> Result<i64, sqlx::Error> {
    let claimed = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM playgroups WHERE discord_guild_id = ?"#,
        guild_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    match claimed {
        Some(id) => Ok(id),
        None => default_id(conn).await,
    }
}

/// Whether a playgroup exists.
pub async fn exists(conn: &mut SqliteConnection, id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM playgroups WHERE id = ?) AS "e!: bool""#,
        id
    )
    .fetch_one(&mut *conn)
    .await
}

/// A playgroup by id.
pub async fn get(conn: &mut SqliteConnection, id: i64) -> Result<Option<Playgroup>, sqlx::Error> {
    select_playgroups!("WHERE id = ?", id)
        .fetch_optional(&mut *conn)
        .await
}

/// Every playgroup, oldest first (so the default comes first).
pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<Playgroup>, sqlx::Error> {
    select_playgroups!("ORDER BY id")
        .fetch_all(&mut *conn)
        .await
}

/// The playgroups an account belongs to, oldest first.
pub async fn list_for_user(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<Playgroup>, sqlx::Error> {
    select_playgroups!(
        "WHERE id IN (SELECT playgroup_id FROM playgroup_members WHERE user_id = ?) ORDER BY id",
        user_id
    )
    .fetch_all(&mut *conn)
    .await
}

/// The ids of the playgroups an account belongs to, oldest first.
pub async fn ids_for_user(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT playgroup_id AS "id!: i64" FROM playgroup_members WHERE user_id = ? ORDER BY playgroup_id"#,
        user_id
    )
    .fetch_all(&mut *conn)
    .await
}

/// Whether an account belongs to a playgroup.
pub async fn is_member(
    conn: &mut SqliteConnection,
    playgroup_id: i64,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM playgroup_members WHERE playgroup_id = ? AND user_id = ?) AS "e!: bool""#,
        playgroup_id,
        user_id
    )
    .fetch_one(&mut *conn)
    .await
}

/// Adds an account to a playgroup; returns whether it was not a member already.
pub async fn add_member(
    conn: &mut SqliteConnection,
    playgroup_id: i64,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    let now = UtcDateTime::now();
    let result = sqlx::query!(
        "INSERT OR IGNORE INTO playgroup_members (playgroup_id, user_id, inserted_at) VALUES (?, ?, ?)",
        playgroup_id,
        user_id,
        now
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Removes an account from a playgroup; returns whether it was a member.
pub async fn remove_member(
    conn: &mut SqliteConnection,
    playgroup_id: i64,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM playgroup_members WHERE playgroup_id = ? AND user_id = ?",
        playgroup_id,
        user_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// A playgroup's members, by username.
pub async fn members(
    conn: &mut SqliteConnection,
    playgroup_id: i64,
) -> Result<Vec<Member>, sqlx::Error> {
    sqlx::query_as!(
        Member,
        r#"SELECT u.id AS "id!", u.username, u.display_name, u.role
           FROM playgroup_members m JOIN users u ON u.id = m.user_id
           WHERE m.playgroup_id = ? ORDER BY u.username"#,
        playgroup_id
    )
    .fetch_all(&mut *conn)
    .await
}

/// The playgroup whose sign-up invitation has the digest `hash`, if any.
pub async fn for_invite_hash(
    conn: &mut SqliteConnection,
    hash: Option<&[u8]>,
) -> Result<Option<i64>, sqlx::Error> {
    let Some(hash) = hash.filter(|hash| hash.len() == 32) else {
        return Ok(None);
    };
    let rows = sqlx::query!(
        r#"SELECT id AS "id!: i64", registration_invite_hash AS "hash!: Vec<u8>"
           FROM playgroups WHERE registration_invite_hash IS NOT NULL ORDER BY id"#
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .find(|row| crypto::secure_compare(&row.hash, hash))
        .map(|row| row.id))
}

fn unique_error(error: sqlx::Error) -> ApiError {
    if db::is_unique_violation(&error, &["playgroups.discord_guild_id"]) {
        ValidationError::single("discord_guild_id", TAKEN).into()
    } else if db::is_unique_violation(&error, &[]) {
        ValidationError::single("name", TAKEN).into()
    } else {
        error.into()
    }
}

fn validate(
    input: &PlaygroupInput,
    current: Option<&Playgroup>,
) -> Result<(String, Option<String>), ValidationError> {
    let mut cs = Validator::new();
    let name = input
        .name
        .clone()
        .trimmed()
        .or(current.map(|current| current.name.clone()));
    let discord_guild_id = input
        .discord_guild_id
        .clone()
        .trimmed()
        .or(current.and_then(|current| current.discord_guild_id.clone()));
    cs.required("name", name.as_ref());
    cs.length("name", name.as_deref(), Some(1), Some(100));
    cs.length(
        "discord_guild_id",
        discord_guild_id.as_deref(),
        Some(1),
        Some(32),
    );
    if discord_guild_id
        .as_deref()
        .is_some_and(|guild| !guild.bytes().all(|byte| byte.is_ascii_digit()))
    {
        cs.add_error(
            "discord_guild_id",
            "must be a Discord guild id (digits only)",
        );
    }
    cs.finish()?;
    Ok((name.unwrap_or_default(), discord_guild_id))
}

/// Gives a new member who signed in with Discord a player in the playgroup, keyed by their
/// Discord identity like a Discord sign-in does, unless one is linked to them already (a
/// member who was removed and re-added). Password accounts get no player automatically;
/// an administrator links one, as before playgroups existed. A player with the member's
/// Discord identity that is linked to another account is a validation error.
async fn resolve_member_player(
    tx: &mut db::Tx,
    playgroup_id: i64,
    user: &User,
) -> Result<(), ApiError> {
    let Some(discord_id) = user.discord_id.as_deref().filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let linked = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM players WHERE playgroup_id = ? AND user_id = ?) AS "e!: bool""#,
        playgroup_id,
        user.id
    )
    .fetch_one(&mut **tx)
    .await?;
    if linked {
        return Ok(());
    }
    resolve_player::run(
        tx,
        playgroup_id,
        &user.display_name,
        Some(discord_id),
        Some(user.id),
    )
    .await
    .map(|_| ())
    .map_err(|error| match error {
        ResolveError::DiscordIdentityConflict => ValidationError::single(
            "user_id",
            "matches a player linked to another account in this playgroup",
        )
        .into(),
        ResolveError::Invalid(errors) => errors.into(),
        ResolveError::Database(error) => error.into(),
    })
}

/// The pool-level playgroups API (`state.playgroups`).
#[derive(Clone, Debug)]
pub struct Playgroups {
    /// Database.
    pub pool: Pool,
}

impl Playgroups {
    /// Every playgroup, oldest first.
    pub async fn list(&self) -> Result<Vec<Playgroup>, sqlx::Error> {
        list(&mut *self.pool.acquire().await?).await
    }

    /// A playgroup by id.
    pub async fn get(&self, id: i64) -> Result<Option<Playgroup>, sqlx::Error> {
        get(&mut *self.pool.acquire().await?, id).await
    }

    /// The default playgroup's id.
    pub async fn default_id(&self) -> Result<i64, sqlx::Error> {
        default_id(&mut *self.pool.acquire().await?).await
    }

    /// The playgroup that claims a Discord guild, or the default one.
    pub async fn for_discord_guild(&self, guild_id: &str) -> Result<i64, sqlx::Error> {
        for_discord_guild(&mut *self.pool.acquire().await?, guild_id).await
    }

    /// The playgroups an account belongs to, oldest first.
    pub async fn list_for_user(&self, user_id: i64) -> Result<Vec<Playgroup>, sqlx::Error> {
        list_for_user(&mut *self.pool.acquire().await?, user_id).await
    }

    /// A playgroup's members.
    pub async fn members(&self, playgroup_id: i64) -> Result<Vec<Member>, sqlx::Error> {
        members(&mut *self.pool.acquire().await?, playgroup_id).await
    }

    /// Adds an account to a playgroup; a Discord account also gets a player there. Already
    /// a member: no change.
    pub async fn add_member(&self, playgroup: &Playgroup, user: &User) -> Result<(), ApiError> {
        let mut tx = db::begin(&self.pool).await?;
        if add_member(&mut tx, playgroup.id, user.id).await? {
            resolve_member_player(&mut tx, playgroup.id, user).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Removes an account from a playgroup. Its player and games stay; the account falls
    /// back to another of its playgroups when it was viewing this one.
    pub async fn remove_member(&self, playgroup: &Playgroup, user_id: i64) -> Result<(), ApiError> {
        let mut tx = db::begin(&self.pool).await?;
        if !remove_member(&mut tx, playgroup.id, user_id).await? {
            return Err(ApiError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Generates a new reusable invitation for a playgroup, storing only its digest, and
    /// returns the secret. The previous link stops working.
    pub async fn rotate_invite(&self, playgroup: &Playgroup) -> Result<String, sqlx::Error> {
        let token = crypto::url_encode64_unpadded(&crypto::random_bytes::<32>());
        let hash = crypto::sha256(token.as_bytes());
        let now = UtcDateTime::now();
        let mut tx = db::begin(&self.pool).await?;
        sqlx::query!(
            "UPDATE playgroups SET registration_invite_hash = ?, updated_at = ? WHERE id = ?",
            hash,
            now,
            playgroup.id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(token)
    }

    /// The playgroup whose invitation has the digest `hash`, if any.
    pub async fn for_invite_hash(
        &self,
        hash: Option<&[u8]>,
    ) -> Result<Option<Playgroup>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        match for_invite_hash(&mut conn, hash).await? {
            Some(id) => get(&mut conn, id).await,
            None => Ok(None),
        }
    }

    /// Creates a playgroup with `creator` as its first member (and, for a Discord account,
    /// its first player).
    pub async fn create(
        &self,
        input: &PlaygroupInput,
        creator: &User,
    ) -> Result<Playgroup, ApiError> {
        let (name, discord_guild_id) = validate(input, None)?;
        let now = UtcDateTime::now();
        let mut tx = db::begin(&self.pool).await?;
        let id = sqlx::query_scalar!(
            r#"INSERT INTO playgroups (name, discord_guild_id, inserted_at, updated_at)
               VALUES (?, ?, ?, ?) RETURNING id AS "id!: i64""#,
            name,
            discord_guild_id,
            now,
            now
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(unique_error)?;
        add_member(&mut tx, id, creator.id).await?;
        resolve_member_player(&mut tx, id, creator).await?;
        tx.commit().await?;
        Ok(Playgroup {
            id,
            name,
            discord_guild_id,
            invite_enabled: false,
            inserted_at: now,
            updated_at: now,
        })
    }

    /// Renames a playgroup or changes its Discord guild.
    pub async fn update(
        &self,
        playgroup: &Playgroup,
        input: &PlaygroupInput,
    ) -> Result<Playgroup, ApiError> {
        let (name, discord_guild_id) = validate(input, Some(playgroup))?;
        if name == playgroup.name && discord_guild_id == playgroup.discord_guild_id {
            return Ok(playgroup.clone());
        }
        let now = UtcDateTime::now();
        let mut tx = db::begin(&self.pool).await?;
        sqlx::query!(
            "UPDATE playgroups SET name = ?, discord_guild_id = ?, updated_at = ? WHERE id = ?",
            name,
            discord_guild_id,
            now,
            playgroup.id
        )
        .execute(&mut *tx)
        .await
        .map_err(unique_error)?;
        tx.commit().await?;
        Ok(Playgroup {
            name,
            discord_guild_id,
            updated_at: now,
            ..playgroup.clone()
        })
    }

    /// Deletes a playgroup that has no history: its memberships and its players (Discord
    /// members get one on joining) go with it. One that still holds games or decks, or the
    /// last playgroup, is a validation error; accounts that had switched to it fall back to
    /// another of their playgroups.
    pub async fn delete(&self, playgroup: &Playgroup) -> Result<(), ApiError> {
        let mut tx = db::begin(&self.pool).await?;
        let refs = sqlx::query!(
            r#"SELECT EXISTS(SELECT 1 FROM games WHERE playgroup_id = ?1) AS "games!: bool",
                      EXISTS(SELECT 1 FROM decks JOIN players ON players.id = decks.player_id
                             WHERE players.playgroup_id = ?1) AS "decks!: bool",
                      (SELECT count(*) FROM playgroups) AS "playgroups!: i64""#,
            playgroup.id
        )
        .fetch_one(&mut *tx)
        .await?;
        if refs.playgroups <= 1 {
            return Err(ValidationError::single("id", "is the last playgroup").into());
        }
        if refs.games {
            return Err(
                ValidationError::single("games", "are still associated with this entry").into(),
            );
        }
        if refs.decks {
            return Err(
                ValidationError::single("decks", "are still associated with this entry").into(),
            );
        }
        sqlx::query!("DELETE FROM players WHERE playgroup_id = ?", playgroup.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!("DELETE FROM playgroups WHERE id = ?", playgroup.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
