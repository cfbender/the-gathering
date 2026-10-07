//! User accounts, authentication, and server registration settings.

pub mod discord;
pub mod user;

use serde_json::{Value, json};
use time::Duration;

use crate::changeset::{Change, Changeset, TAKEN};
use crate::crypto;
use crate::db::{self, IsoDate, Pool, UtcDateTime};
use crate::error::{ApiError, Errors};

pub use self::user::User;
use self::user::{
    ENCRYPTED_STRING_SALT, default_display_name, normalize_username, validate_account_fields,
    validate_password,
};

/// Days a cookie session stays valid.
pub const SESSION_VALIDITY_DAYS: i64 = 14;

/// A `users` row as stored. `Debug` redacts the credentials.
pub struct UserRow {
    id: i64,
    username: String,
    display_name: String,
    role: String,
    disabled_at: Option<UtcDateTime>,
    hashed_password: Option<String>,
    discord_id: Option<String>,
    avatar_url: Option<String>,
    moxfield_username: Option<String>,
    archidekt_username: Option<String>,
    manavault_url: Option<String>,
    manavault_api_key: Option<String>,
    palette: String,
    theme_style: String,
    inserted_at: UtcDateTime,
    updated_at: UtcDateTime,
}

impl std::fmt::Debug for UserRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserRow")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("role", &self.role)
            .field(
                "hashed_password",
                &user::redacted(self.hashed_password.as_ref()),
            )
            .field(
                "manavault_api_key",
                &user::redacted(self.manavault_api_key.as_ref()),
            )
            .finish_non_exhaustive()
    }
}

impl UserRow {
    /// Decrypts stored credentials with `secret_key_base`.
    pub fn into_user(self, secret_key_base: &str) -> User {
        User {
            id: self.id,
            username: self.username,
            display_name: self.display_name,
            role: self.role,
            disabled_at: self.disabled_at,
            hashed_password: self.hashed_password,
            discord_id: self.discord_id,
            avatar_url: self.avatar_url,
            moxfield_username: self.moxfield_username,
            archidekt_username: self.archidekt_username,
            manavault_url: self.manavault_url,
            manavault_api_key: self.manavault_api_key.and_then(|stored| {
                crypto::decrypt(
                    secret_key_base,
                    ENCRYPTED_STRING_SALT,
                    &stored,
                    Some(i64::MAX),
                )
                .and_then(|plain| String::from_utf8(plain).ok())
            }),
            palette: self.palette,
            theme_style: self.theme_style,
            inserted_at: self.inserted_at,
            updated_at: self.updated_at,
            authenticated_at: None,
        }
    }
}

/// Selects `UserRow`s: `select_users!("WHERE id = ?", id)`.
macro_rules! select_users {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            UserRow,
            r#"SELECT id AS "id!", username, display_name, role,
                disabled_at AS "disabled_at: UtcDateTime", hashed_password, discord_id, avatar_url,
                moxfield_username, archidekt_username, manavault_url, manavault_api_key,
                palette, theme_style, inserted_at AS "inserted_at: UtcDateTime",
                updated_at AS "updated_at: UtcDateTime"
               FROM users "# + $tail
            $(, $arg)*
        )
    };
}
pub(crate) use select_users;

/// Accounts operations need the pool and the secret that encrypts stored credentials.
#[derive(Clone, Debug)]
pub struct Accounts {
    /// Database.
    pub pool: Pool,
    /// `secret_key_base`.
    pub secret_key_base: String,
    /// bcrypt cost.
    pub bcrypt_cost: u32,
}

/// `registration_status/0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistrationStatus {
    /// Whether new accounts may be created.
    pub allowed: bool,
    /// No account exists yet; the first one becomes the administrator.
    pub bootstrap: bool,
}

/// Why registration failed.
#[derive(Debug)]
pub enum RegisterError {
    /// Registration is closed.
    Closed,
    /// Validation failed.
    Invalid(Errors),
    /// Database error.
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RegisterError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// Server settings (`server_settings` row 1). `Debug` redacts the invitation digest.
#[derive(Clone, PartialEq, Eq)]
pub struct ServerSettings {
    /// Anyone may sign in with Discord and become a member.
    pub registration_enabled: bool,
    /// Games before this date count toward records only.
    pub detailed_stats_from: Option<IsoDate>,
    /// SHA-256 of the reusable invitation secret.
    pub registration_invite_hash: Option<Vec<u8>>,
}

impl std::fmt::Debug for ServerSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSettings")
            .field("registration_enabled", &self.registration_enabled)
            .field("detailed_stats_from", &self.detailed_stats_from)
            .field(
                "registration_invite_hash",
                &user::redacted(self.registration_invite_hash.as_ref()),
            )
            .finish()
    }
}

/// A personal API key (without its secret).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiKey {
    /// Primary key.
    pub id: i64,
    /// Owner.
    pub user_id: i64,
    /// Label.
    pub name: String,
    /// First characters, to tell keys apart.
    pub prefix: String,
    /// Last use, at most once a minute.
    pub last_used_at: Option<UtcDateTime>,
    /// Creation.
    pub inserted_at: UtcDateTime,
}

impl ApiKey {
    /// `ApiKeyJSON.data/1`.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "prefix": self.prefix,
            "last_used_at": self.last_used_at,
            "inserted_at": self.inserted_at,
        })
    }
}

/// Digest of an API key; `None` for values that cannot be keys.
pub fn api_key_hash(token: &str) -> Option<Vec<u8>> {
    token
        .starts_with("tg_")
        .then(|| crypto::sha256(token.as_bytes()))
}

/// Digest of a registration invitation; `None` unless it has the generated length.
pub fn registration_invite_hash(token: &str) -> Option<Vec<u8>> {
    (token.len() == 43).then(|| crypto::sha256(token.as_bytes()))
}

fn bcrypt_verify(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Spends the same time as a failed verification (`Bcrypt.no_user_verify/0`).
fn no_user_verify(cost: u32) {
    let _ = bcrypt::hash("no user verify", cost);
}

impl Accounts {
    /// Loads a user by id.
    pub async fn get_user(&self, id: i64) -> Result<Option<User>, sqlx::Error> {
        Ok(select_users!("WHERE id = ?", id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| row.into_user(&self.secret_key_base)))
    }

    /// By Discord id.
    pub async fn get_user_by_discord_id(
        &self,
        discord_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        Ok(select_users!("WHERE discord_id = ?", discord_id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| row.into_user(&self.secret_key_base)))
    }

    /// By username (lowercased).
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        let username = username.to_lowercase();
        Ok(select_users!("WHERE username = ?", username)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| row.into_user(&self.secret_key_base)))
    }

    /// Every user by username.
    pub async fn list_users(&self) -> Result<Vec<User>, sqlx::Error> {
        Ok(select_users!("ORDER BY username ASC")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| row.into_user(&self.secret_key_base))
            .collect())
    }

    /// `registration_status/0` (without the Discord flag, which the caller adds).
    pub async fn registration_status(&self) -> Result<RegistrationStatus, sqlx::Error> {
        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!: i64" FROM users"#)
            .fetch_one(&self.pool)
            .await?;
        let settings = self.get_settings().await?;
        Ok(RegistrationStatus {
            allowed: count == 0 || settings.registration_enabled,
            bootstrap: count == 0,
        })
    }

    /// Hashes a password.
    pub fn hash_password(&self, password: &str) -> Result<String, ApiError> {
        bcrypt::hash(password, self.bcrypt_cost).map_err(|error| ApiError::Internal(error.into()))
    }

    /// `register_user/1`: only the bootstrap administrator registers with a password.
    pub async fn register_user(&self, attrs: &Value) -> Result<User, RegisterError> {
        let mut tx = db::begin(&self.pool).await?;
        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!: i64" FROM users"#)
            .fetch_one(&mut *tx)
            .await?;
        if count != 0 {
            return Err(RegisterError::Closed);
        }
        let user = self.insert_account(&mut tx, attrs, Some("admin")).await?;
        tx.commit().await?;
        Ok(user)
    }

    /// `create_user/1` (`admin_changeset`): an account with a password and a role from `attrs`.
    pub async fn create_user(&self, attrs: &Value) -> Result<User, RegisterError> {
        let mut tx = db::begin(&self.pool).await?;
        let user = self.insert_account(&mut tx, attrs, None).await?;
        tx.commit().await?;
        Ok(user)
    }

    /// `create_admin/1`.
    pub async fn create_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<User, RegisterError> {
        self.create_user(&json!({
            "username": username,
            "display_name": username,
            "password": password,
            "role": "admin",
        }))
        .await
    }

    async fn insert_account(
        &self,
        tx: &mut db::Tx,
        attrs: &Value,
        forced_role: Option<&str>,
    ) -> Result<User, RegisterError> {
        let mut cs = Changeset::new(attrs);
        let username = normalize_username(cs.string("username").or(None));
        let display_name =
            default_display_name(cs.string("display_name").or(None), username.as_ref());
        let password = cs.string("password").or(None);
        let role = match forced_role {
            Some(role) => Some(role.to_owned()),
            None => cs.string("role").or(Some("member".to_owned())),
        };
        validate_account_fields(
            &mut cs,
            username.as_deref(),
            display_name.as_deref(),
            role.as_deref(),
        );
        validate_password(&mut cs, password.as_deref());
        if let Some(name) = &username {
            let taken = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM users WHERE username = ?) AS "taken!: bool""#,
                name
            )
            .fetch_one(&mut **tx)
            .await?;
            if taken && cs.is_valid() {
                cs.add_error("username", TAKEN);
            }
        }
        cs.finish().map_err(RegisterError::Invalid)?;
        let (Some(username), Some(display_name), Some(password), Some(role)) =
            (username, display_name, password, role)
        else {
            return Err(RegisterError::Invalid(Errors::single(
                "username",
                "can't be blank",
            )));
        };
        let hashed = self
            .hash_password(&password)
            .map_err(|_| RegisterError::Invalid(Errors::single("password", "is invalid")))?;
        let now = UtcDateTime::now();
        let id = sqlx::query_scalar!(
            r#"INSERT INTO users (username, display_name, hashed_password, role, inserted_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?) RETURNING id AS "id!: i64""#,
            username,
            display_name,
            hashed,
            role,
            now,
            now
        )
        .fetch_one(&mut **tx)
        .await?;
        let row = select_users!("WHERE id = ?", id)
            .fetch_one(&mut **tx)
            .await?;
        Ok(row.into_user(&self.secret_key_base))
    }

    /// The first enabled administrator, creating a passwordless `dev` one if none exists.
    pub async fn get_or_create_dev_admin(&self) -> Result<User, sqlx::Error> {
        if let Some(row) =
            select_users!("WHERE role = 'admin' AND disabled_at IS NULL ORDER BY id ASC LIMIT 1")
                .fetch_optional(&self.pool)
                .await?
        {
            return Ok(row.into_user(&self.secret_key_base));
        }
        let now = UtcDateTime::now();
        let id = sqlx::query_scalar!(
            r#"INSERT INTO users (username, display_name, role, inserted_at, updated_at)
               VALUES ('dev', 'Developer', 'admin', ?, ?) RETURNING id AS "id!: i64""#,
            now,
            now
        )
        .fetch_one(&self.pool)
        .await?;
        self.get_user(id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// Password sign-in: only enabled administrators have passwords.
    pub async fn get_user_by_username_and_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        let username = username.trim().to_lowercase();
        let user = select_users!("WHERE username = ?", username)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| row.into_user(&self.secret_key_base));
        let cost = self.bcrypt_cost;
        let password = password.to_owned();
        let result = tokio::task::spawn_blocking(move || match user {
            Some(user) if user.is_admin() && user.disabled_at.is_none() => {
                match (&user.hashed_password, password.is_empty()) {
                    (Some(hash), false) if bcrypt_verify(&password, hash) => Some(user),
                    (Some(_), false) => None,
                    _ => {
                        no_user_verify(cost);
                        None
                    }
                }
            }
            _ => {
                no_user_verify(cost);
                None
            }
        })
        .await
        .unwrap_or(None);
        Ok(result)
    }

    /// `update_profile/2`: also renames the linked player.
    pub async fn update_profile(
        &self,
        user: &User,
        attrs: &Value,
        allow_insecure: impl Fn(&str) -> bool,
    ) -> Result<User, ApiError> {
        let changes = user::profile_changes(user, attrs, allow_insecure)?;
        let stored_key = self.stored_api_key(user.id).await?;
        let mut tx = db::begin(&self.pool).await?;
        let display_name = changes
            .display_name
            .clone()
            .or(Some(user.display_name.clone()))
            .unwrap_or_default();
        let moxfield = changes.moxfield_username.or(user.moxfield_username.clone());
        let archidekt = changes
            .archidekt_username
            .or(user.archidekt_username.clone());
        let manavault_url = changes.manavault_url.or(user.manavault_url.clone());
        let api_key: Option<String> = match changes.manavault_api_key {
            Change::Unchanged => stored_key,
            Change::Set(None) => None,
            Change::Set(Some(key)) => Some(crypto::encrypt(
                &self.secret_key_base,
                ENCRYPTED_STRING_SALT,
                key.as_bytes(),
                86_400,
            )),
        };
        let now = UtcDateTime::now();
        sqlx::query!(
            "UPDATE users SET display_name = ?, moxfield_username = ?, archidekt_username = ?,
             manavault_url = ?, manavault_api_key = ?, updated_at = ? WHERE id = ?",
            display_name,
            moxfield,
            archidekt,
            manavault_url,
            api_key,
            now,
            user.id
        )
        .execute(&mut *tx)
        .await?;
        if display_name != user.display_name {
            rename_linked_player(&mut tx, user.id, &display_name).await?;
        }
        tx.commit().await?;
        self.get_user(user.id).await?.ok_or(ApiError::NotFound)
    }

    async fn stored_api_key(&self, user_id: i64) -> Result<Option<String>, sqlx::Error> {
        Ok(
            sqlx::query_scalar!("SELECT manavault_api_key FROM users WHERE id = ?", user_id)
                .fetch_optional(&self.pool)
                .await?
                .flatten(),
        )
    }

    /// `update_appearance/2`.
    pub async fn update_appearance(&self, user: &User, attrs: &Value) -> Result<User, ApiError> {
        let mut cs = Changeset::new(attrs);
        let palette = cs.string("palette").or(Some(user.palette.clone()));
        let theme_style = cs.string("theme_style").or(Some(user.theme_style.clone()));
        cs.required("palette", palette.as_ref());
        cs.required("theme_style", theme_style.as_ref());
        cs.inclusion("palette", palette.as_deref(), &user::PALETTES);
        cs.inclusion("theme_style", theme_style.as_deref(), &user::THEME_STYLES);
        cs.finish()?;
        let now = UtcDateTime::now();
        sqlx::query!(
            "UPDATE users SET palette = ?, theme_style = ?, updated_at = ? WHERE id = ?",
            palette,
            theme_style,
            now,
            user.id
        )
        .execute(&self.pool)
        .await?;
        self.get_user(user.id).await?.ok_or(ApiError::NotFound)
    }

    /// `update_user_password/2`: administrators with a password only. Deletes every token.
    pub async fn update_user_password(&self, user: &User, attrs: &Value) -> Result<User, ApiError> {
        if !user.is_admin() || user.hashed_password.is_none() {
            return Err(ApiError::Forbidden);
        }
        let mut cs = Changeset::new(attrs);
        let password = cs.string("password").or(None);
        let confirmation = cs.string("password_confirmation");
        if let Change::Set(confirmation) = &confirmation
            && confirmation.as_deref() != password.as_deref()
        {
            cs.add_error("password_confirmation", "does not match password");
        }
        validate_password(&mut cs, password.as_deref());
        cs.finish()?;
        let hashed = self.hash_password(password.as_deref().unwrap_or_default())?;
        let mut tx = db::begin(&self.pool).await?;
        let now = UtcDateTime::now();
        sqlx::query!(
            "UPDATE users SET hashed_password = ?, updated_at = ? WHERE id = ?",
            hashed,
            now,
            user.id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!("DELETE FROM users_tokens WHERE user_id = ?", user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.get_user(user.id).await?.ok_or(ApiError::NotFound)
    }

    /// Creates a session token for `user`, pruning expired ones first.
    pub async fn generate_user_session_token(&self, user: &User) -> Result<Vec<u8>, sqlx::Error> {
        self.prune_expired_user_session_tokens().await?;
        let token = crypto::random_bytes::<32>().to_vec();
        let now = UtcDateTime::now();
        let authenticated_at = user.authenticated_at.unwrap_or(now);
        sqlx::query!(
            "INSERT INTO users_tokens (user_id, token, context, authenticated_at, inserted_at) VALUES (?, ?, 'session', ?, ?)",
            user.id,
            token,
            authenticated_at,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    /// Deletes expired sessions.
    pub async fn prune_expired_user_session_tokens(&self) -> Result<u64, sqlx::Error> {
        let cutoff = UtcDateTime::now().plus(Duration::days(-SESSION_VALIDITY_DAYS));
        Ok(sqlx::query!(
            "DELETE FROM users_tokens WHERE context = 'session' AND inserted_at <= ?",
            cutoff
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    /// The enabled user behind a valid session token and the token's creation time.
    pub async fn get_user_by_session_token(
        &self,
        token: &[u8],
    ) -> Result<Option<(User, UtcDateTime)>, sqlx::Error> {
        let cutoff = UtcDateTime::now().plus(Duration::days(-SESSION_VALIDITY_DAYS));
        let found = sqlx::query!(
            r#"SELECT user_id AS "user_id!: i64", authenticated_at AS "authenticated_at: UtcDateTime",
                      inserted_at AS "inserted_at!: UtcDateTime"
               FROM users_tokens WHERE token = ? AND context = 'session' AND inserted_at > ?"#,
            token,
            cutoff
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(found) = found else { return Ok(None) };
        let Some(mut user) = self.get_user(found.user_id).await? else {
            return Ok(None);
        };
        if user.disabled_at.is_some() {
            return Ok(None);
        }
        user.authenticated_at = found.authenticated_at;
        Ok(Some((user, found.inserted_at)))
    }

    /// Deletes one session token.
    pub async fn delete_user_session_token(&self, token: &[u8]) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM users_tokens WHERE token = ? AND context = 'session'",
            token
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Signs the user out everywhere.
    pub async fn revoke_all_sessions(&self, user_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM users_tokens WHERE user_id = ?", user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The user's API keys, newest first.
    pub async fn list_api_keys(&self, user_id: i64) -> Result<Vec<ApiKey>, sqlx::Error> {
        sqlx::query_as!(
            ApiKey,
            r#"SELECT id AS "id!", user_id, name, prefix, last_used_at AS "last_used_at: UtcDateTime",
                      inserted_at AS "inserted_at: UtcDateTime"
               FROM api_keys WHERE user_id = ? ORDER BY inserted_at DESC, id DESC"#,
            user_id
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Creates a key, returning the one-time secret with the stored key.
    pub async fn create_api_key(
        &self,
        user_id: i64,
        attrs: &Value,
    ) -> Result<(String, ApiKey), ApiError> {
        let mut cs = Changeset::new(attrs);
        let name = cs
            .string("name")
            .or(None)
            .map(|name| name.trim().to_owned());
        cs.required("name", name.as_ref());
        cs.length("name", name.as_deref(), None, Some(60));
        cs.finish()?;
        let token = format!(
            "tg_{}",
            crypto::url_encode64_unpadded(&crypto::random_bytes::<32>())
        );
        let hash = api_key_hash(&token).unwrap_or_default();
        let prefix: String = token.chars().take(10).collect();
        let now = UtcDateTime::now();
        let id = sqlx::query_scalar!(
            r#"INSERT INTO api_keys (user_id, name, token_hash, prefix, inserted_at) VALUES (?, ?, ?, ?, ?)
               RETURNING id AS "id!: i64""#,
            user_id,
            name,
            hash,
            prefix,
            now
        )
        .fetch_one(&self.pool)
        .await?;
        let key = ApiKey {
            id,
            user_id,
            name: name.unwrap_or_default(),
            prefix,
            last_used_at: None,
            inserted_at: now,
        };
        Ok((token, key))
    }

    /// Deletes one of the user's keys.
    pub async fn delete_api_key(&self, user_id: i64, id: i64) -> Result<(), ApiError> {
        let deleted = sqlx::query!(
            "DELETE FROM api_keys WHERE id = ? AND user_id = ?",
            id,
            user_id
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        if deleted == 0 {
            Err(ApiError::NotFound)
        } else {
            Ok(())
        }
    }

    /// The enabled owner of an API key, touching `last_used_at` at most once a minute.
    pub async fn authenticate_api_key(&self, token: &str) -> Result<Option<User>, sqlx::Error> {
        let Some(hash) = api_key_hash(token) else {
            return Ok(None);
        };
        let found = sqlx::query!(
            r#"SELECT k.id AS "id!: i64", k.user_id AS "user_id!: i64" FROM api_keys k
               JOIN users u ON u.id = k.user_id WHERE k.token_hash = ? AND u.disabled_at IS NULL"#,
            hash
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(found) = found else { return Ok(None) };
        let now = UtcDateTime::now();
        let stale = now.plus(Duration::seconds(-60));
        sqlx::query!(
            "UPDATE api_keys SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR last_used_at < ?)",
            now,
            found.id,
            stale
        )
        .execute(&self.pool)
        .await?;
        self.get_user(found.user_id).await
    }

    /// `update_user/2` (admin): username, display name, role, and disabled state.
    pub async fn update_user(&self, user: &User, attrs: &Value) -> Result<User, ApiError> {
        let mut cs = Changeset::new(attrs);
        let username = match cs.string("username") {
            Change::Unchanged => Some(user.username.clone()),
            Change::Set(value) => normalize_username(value),
        };
        let display_name = cs
            .string("display_name")
            .map(|name| name.trim().to_owned())
            .or(Some(user.display_name.clone()));
        let role = cs.string("role").or(Some(user.role.clone()));
        let disabled_at = cs.datetime("disabled_at").or(user.disabled_at);
        validate_account_fields(
            &mut cs,
            username.as_deref(),
            display_name.as_deref(),
            role.as_deref(),
        );
        if let Some(name) = username.as_deref().filter(|name| *name != user.username) {
            let taken = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM users WHERE username = ? AND id != ?) AS "taken!: bool""#,
                name,
                user.id
            )
            .fetch_one(&self.pool)
            .await?;
            if taken && cs.is_valid() {
                cs.add_error("username", TAKEN);
            }
        }
        cs.finish()?;

        let mut tx = db::begin(&self.pool).await?;
        let becoming_inactive = user.is_admin()
            && user.disabled_at.is_none()
            && (role.as_deref() != Some("admin") || disabled_at.is_some());
        if becoming_inactive {
            let other_admin = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM users WHERE id != ? AND role = 'admin' AND disabled_at IS NULL) AS "exists!: bool""#,
                user.id
            )
            .fetch_one(&mut *tx)
            .await?;
            if !other_admin {
                return Err(Errors::single("role", "must leave at least one enabled admin").into());
            }
        }
        let now = UtcDateTime::now();
        sqlx::query!(
            "UPDATE users SET username = ?, display_name = ?, role = ?, disabled_at = ?, updated_at = ? WHERE id = ?",
            username,
            display_name,
            role,
            disabled_at,
            now,
            user.id
        )
        .execute(&mut *tx)
        .await?;
        let display_name = display_name.unwrap_or_default();
        if display_name != user.display_name {
            rename_linked_player(&mut tx, user.id, &display_name).await?;
        }
        if user.disabled_at.is_none() && disabled_at.is_some() {
            sqlx::query!(
                "DELETE FROM users_tokens WHERE user_id = ? AND context = 'session'",
                user.id
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.get_user(user.id).await?.ok_or(ApiError::NotFound)
    }

    /// `disable_user/1`.
    pub async fn disable_user(&self, user: &User) -> Result<User, ApiError> {
        self.update_user(
            user,
            &json!({ "disabled_at": UtcDateTime::now().to_string() }),
        )
        .await
    }

    /// `delete_user/2`: refuses self-deletion, the last administrator, and players with games.
    pub async fn delete_user(&self, user: &User, actor: &User) -> Result<(), ApiError> {
        if user.id == actor.id {
            return Err(ApiError::Forbidden);
        }
        let mut tx = db::begin(&self.pool).await?;
        if user.is_admin() {
            let other_admin = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM users WHERE id != ? AND role = 'admin') AS "exists!: bool""#,
                user.id
            )
            .fetch_one(&mut *tx)
            .await?;
            if !other_admin {
                return Err(ApiError::Forbidden);
            }
        }
        let referenced = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                 SELECT 1 FROM game_players seat
                 WHERE seat.player_id IN (SELECT id FROM players WHERE user_id = ?1)
                    OR seat.eliminated_by_player_id IN (SELECT id FROM players WHERE user_id = ?1)
                    OR seat.deck_id IN (SELECT d.id FROM decks d JOIN players p ON p.id = d.player_id WHERE p.user_id = ?1)
               ) AS "exists!: bool""#,
            user.id
        )
        .fetch_one(&mut *tx)
        .await?;
        if referenced {
            return Err(
                Errors::single("player", "must have zero games before deleting this user").into(),
            );
        }
        sqlx::query!(
            "DELETE FROM decks WHERE player_id IN (SELECT id FROM players WHERE user_id = ?)",
            user.id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!("DELETE FROM players WHERE user_id = ?", user.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!(
            "UPDATE games SET created_by_user_id = NULL WHERE created_by_user_id = ?",
            user.id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!("DELETE FROM users_tokens WHERE user_id = ?", user.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!("DELETE FROM api_keys WHERE user_id = ?", user.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!("DELETE FROM users WHERE id = ?", user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The settings row.
    pub async fn get_settings(&self) -> Result<ServerSettings, sqlx::Error> {
        sqlx::query_as!(
            ServerSettings,
            r#"SELECT registration_enabled AS "registration_enabled: bool",
                      detailed_stats_from AS "detailed_stats_from: IsoDate",
                      registration_invite_hash
               FROM server_settings WHERE id = 1"#
        )
        .fetch_one(&self.pool)
        .await
    }

    /// `update_settings/1`.
    pub async fn update_settings(&self, attrs: &Value) -> Result<ServerSettings, ApiError> {
        let current = self.get_settings().await?;
        let mut cs = Changeset::new(attrs);
        let enabled = cs
            .boolean("registration_enabled")
            .or(Some(current.registration_enabled));
        let from = cs
            .date("detailed_stats_from")
            .or(current.detailed_stats_from);
        cs.required_value("registration_enabled", enabled.as_ref());
        cs.finish()?;
        let now = UtcDateTime::now();
        sqlx::query!(
            "UPDATE server_settings SET registration_enabled = ?, detailed_stats_from = ?, updated_at = ? WHERE id = 1",
            enabled,
            from,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(self.get_settings().await?)
    }

    /// Generates a new reusable invitation, storing only its digest. Returns the secret.
    pub async fn rotate_registration_invite(&self) -> Result<String, sqlx::Error> {
        let token = crypto::url_encode64_unpadded(&crypto::random_bytes::<32>());
        let hash = crypto::sha256(token.as_bytes());
        let now = UtcDateTime::now();
        sqlx::query!(
            "UPDATE server_settings SET registration_invite_hash = ?, updated_at = ? WHERE id = 1",
            hash,
            now
        )
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    /// Whether `hash` is the current invitation's digest.
    pub async fn valid_registration_invite_hash(
        &self,
        hash: Option<&[u8]>,
    ) -> Result<bool, sqlx::Error> {
        let Some(hash) = hash.filter(|hash| hash.len() == 32) else {
            return Ok(false);
        };
        Ok(self
            .get_settings()
            .await?
            .registration_invite_hash
            .is_some_and(|current| crypto::secure_compare(&current, hash)))
    }
}

/// Games, stats, and Discord show a player's name, so a new display name reaches the
/// linked player. A clash with another player's name is a display-name error.
async fn rename_linked_player(
    tx: &mut db::Tx,
    user_id: i64,
    display_name: &str,
) -> Result<(), ApiError> {
    let name = display_name.trim();
    let player = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM players WHERE user_id = ?"#,
        user_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(player_id) = player else {
        return Ok(());
    };
    let now = UtcDateTime::now();
    match sqlx::query!(
        "UPDATE players SET name = ?, updated_at = ? WHERE id = ?",
        name,
        now,
        player_id
    )
    .execute(&mut **tx)
    .await
    {
        Ok(_) => Ok(()),
        Err(error) if db::is_unique_violation(&error, &[]) => {
            Err(Errors::single("display_name", "is already used by another player").into())
        }
        Err(error) => Err(error.into()),
    }
}

/// `sudo_mode?/2`: whether the session authenticated within the last `minutes`.
pub fn sudo_mode(user: &User, minutes: i64) -> bool {
    user.authenticated_at
        .is_some_and(|at| at > UtcDateTime::now().plus(Duration::minutes(-minutes)))
}
