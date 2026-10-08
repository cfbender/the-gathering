//! Signing in with Discord.

use crate::db::{self, UtcDateTime};
use crate::games::resolve_player::{self, ResolveError};
use crate::regex::compile;
use crate::validation::ValidationError;
use crate::validation::{TAKEN, Validator};

use super::user::{normalize_username, validate_account_fields};
use super::{Accounts, User, UserRow, select_users};

/// The Discord profile, normalized to OpenID-style claims.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscordClaims {
    /// Discord user id (`sub`).
    pub sub: String,
    /// Discord username (`preferred_username`).
    pub preferred_username: Option<String>,
    /// Avatar URL, when the account has an avatar.
    pub picture: Option<String>,
}

impl DiscordClaims {
    /// From Discord's `/users/@me` response.
    ///
    /// The avatar URL is `https://cdn.discordapp.com/avatars/<id>/<avatar>`; a `null` avatar
    /// is `None` rather than a broken URL ending in `/`.
    pub fn from_discord_user(user: &serde_json::Value) -> Option<Self> {
        let sub = match user.get("id")? {
            serde_json::Value::String(id) => id.clone(),
            serde_json::Value::Number(id) => id.to_string(),
            _ => return None,
        };
        let picture = user
            .get("avatar")
            .and_then(serde_json::Value::as_str)
            .filter(|avatar| !avatar.is_empty())
            .map(|avatar| format!("https://cdn.discordapp.com/avatars/{sub}/{avatar}"));
        Some(Self {
            preferred_username: user
                .get("username")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            picture,
            sub,
        })
    }
}

/// Why Discord sign-in failed.
#[derive(Debug)]
pub enum SignInError {
    /// The account is disabled.
    Disabled,
    /// Unknown Discord account and registration is closed.
    RegistrationClosed,
    /// The player for this account is linked to someone else.
    DiscordIdentityConflict,
    /// The new account was invalid.
    InvalidUser(ValidationError),
    /// Database error.
    Database(sqlx::Error),
}

impl From<sqlx::Error> for SignInError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<ResolveError> for SignInError {
    fn from(error: ResolveError) -> Self {
        match error {
            ResolveError::DiscordIdentityConflict => Self::DiscordIdentityConflict,
            ResolveError::Invalid(errors) => Self::InvalidUser(errors),
            ResolveError::Database(error) => Self::Database(error),
        }
    }
}

/// Usernames must start with a letter or digit; Discord handles may start with a dot, so
/// punctuation is stripped from the edges and taken names get a numeric suffix.
async fn available_username(
    tx: &mut db::Tx,
    preferred: Option<&str>,
    discord_id: &str,
) -> Result<String, sqlx::Error> {
    let invalid = compile(r"[^a-z0-9_.-]");
    let edges = compile(r"^[_.-]+|[_.-]+$");
    let lowered = preferred.unwrap_or_default().to_lowercase();
    let replaced = invalid.replace_all(&lowered, "_");
    let sliced: String = replaced.chars().take(32).collect();
    let mut base = edges.replace_all(&sliced, "").into_owned();
    if base.chars().count() < 3 {
        "discord".clone_into(&mut base);
    }
    let prefix: String = base.chars().take(38).collect();
    let mut candidates = vec![base.clone()];
    candidates.extend((2..=9).map(|n| format!("{prefix}{n}")));
    for candidate in candidates {
        let taken = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM users WHERE username = ?) AS "taken!: bool""#,
            candidate
        )
        .fetch_one(&mut **tx)
        .await?;
        if !taken {
            return Ok(candidate);
        }
    }
    let short: String = base.chars().take(19).collect();
    let id: String = discord_id.chars().take(20).collect();
    Ok(format!("{short}_{id}"))
}

impl Accounts {
    /// Finds or creates the Discord account and its player, all in one transaction.
    ///
    /// New accounts need an existing administrator plus open registration or a valid
    /// invitation digest, checked inside the write transaction so rotating the invitation
    /// also revokes invitations on their way back from Discord.
    pub async fn sign_in_with_discord(
        &self,
        claims: &DiscordClaims,
        invite_hash: Option<&[u8]>,
    ) -> Result<User, SignInError> {
        let mut tx = db::begin(&self.pool).await?;
        let existing = select_users!("WHERE discord_id = ?", claims.sub)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| row.into_user(&self.secret_key));
        let user = match existing {
            Some(user) if user.disabled_at.is_some() => return Err(SignInError::Disabled),
            Some(user) => {
                let now = UtcDateTime::now();
                if user.avatar_url != claims.picture {
                    sqlx::query!(
                        "UPDATE users SET avatar_url = ?, updated_at = ? WHERE id = ?",
                        claims.picture,
                        now,
                        user.id
                    )
                    .execute(&mut *tx)
                    .await?;
                }
                User {
                    avatar_url: claims.picture.clone(),
                    ..user
                }
            }
            None => {
                self.create_discord_user(&mut tx, claims, invite_hash)
                    .await?
            }
        };
        resolve_player::run(
            &mut tx,
            &user.display_name,
            user.discord_id.as_deref(),
            Some(user.id),
        )
        .await?;
        tx.commit().await?;
        Ok(user)
    }

    async fn create_discord_user(
        &self,
        tx: &mut db::Tx,
        claims: &DiscordClaims,
        invite_hash: Option<&[u8]>,
    ) -> Result<User, SignInError> {
        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!: i64" FROM users"#)
            .fetch_one(&mut **tx)
            .await?;
        let settings = sqlx::query!(
            r#"SELECT registration_enabled AS "enabled: bool", registration_invite_hash FROM server_settings WHERE id = 1"#
        )
        .fetch_one(&mut **tx)
        .await?;
        let invited = match (
            invite_hash.filter(|hash| hash.len() == 32),
            &settings.registration_invite_hash,
        ) {
            (Some(hash), Some(current)) => crate::crypto::secure_compare(current, hash),
            _ => false,
        };
        if count == 0 || !(settings.enabled || invited) {
            return Err(SignInError::RegistrationClosed);
        }

        let username =
            available_username(tx, claims.preferred_username.as_deref(), &claims.sub).await?;
        let username = normalize_username(Some(username));
        let display_name = claims
            .preferred_username
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| username.clone())
            .map(|name| name.trim().to_owned());
        let mut cs = Validator::new();
        validate_account_fields(
            &mut cs,
            username.as_deref(),
            display_name.as_deref(),
            Some("member"),
        );
        let taken = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM users WHERE discord_id = ?) AS "taken!: bool""#,
            claims.sub
        )
        .fetch_one(&mut **tx)
        .await?;
        if taken {
            cs.add_error("discord_id", TAKEN);
        }
        cs.finish().map_err(SignInError::InvalidUser)?;

        let now = UtcDateTime::now();
        let id = sqlx::query_scalar!(
            r#"INSERT INTO users (username, display_name, role, discord_id, avatar_url, inserted_at, updated_at)
               VALUES (?, ?, 'member', ?, ?, ?, ?) RETURNING id AS "id!: i64""#,
            username,
            display_name,
            claims.sub,
            claims.picture,
            now,
            now
        )
        .fetch_one(&mut **tx)
        .await?;
        let row: UserRow = select_users!("WHERE id = ?", id)
            .fetch_one(&mut **tx)
            .await?;
        Ok(row.into_user(&self.secret_key))
    }
}
