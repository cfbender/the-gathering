//! The `users` table.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::{IsoDate, UtcDateTime};
use crate::patch::Patch;
use crate::regex::{Regex, compile};
use crate::validation::{ValidationError, Validator};

/// The purpose stored credentials (such as ManaVault API keys) are sealed for.
const STORED_SECRET_PURPOSE: &str = "the-gathering.stored-secret";
/// Marks the current stored-credential format.
const STORED_SECRET_PREFIX: &str = "enc.v1.";

/// Encrypts a credential for storage.
pub fn encrypt_secret(secret_key: &str, plain: &str) -> String {
    format!(
        "{STORED_SECRET_PREFIX}{}",
        crate::crypto::seal(secret_key, STORED_SECRET_PURPOSE, plain.as_bytes())
    )
}

/// Decrypts a stored credential, in the current format or the one earlier releases wrote.
pub fn decrypt_secret(secret_key: &str, stored: &str) -> Option<String> {
    let plain = match stored.strip_prefix(STORED_SECRET_PREFIX) {
        Some(sealed) => crate::crypto::open(secret_key, STORED_SECRET_PURPOSE, sealed),
        None => crate::legacy::decrypt_secret(stored, secret_key),
    }?;
    String::from_utf8(plain).ok()
}

/// Palette ids; keep in sync with `PALETTES` in `assets/react/src/lib/theme.tsx` and
/// `assets/react/src/palettes.css`.
pub const PALETTES: [&str; 12] = [
    "claret",
    "nord",
    "catppuccin",
    "tokyonight",
    "gruvbox",
    "everforest",
    "kanagawa",
    "nightowl",
    "dracula",
    "rosepine",
    "solarized",
    "monochrome",
];
/// Surface styles.
pub const THEME_STYLES: [&str; 2] = ["glass", "classic"];
/// Roles.
pub const ROLES: [&str; 2] = ["admin", "member"];

static USERNAME: LazyLock<Regex> = LazyLock::new(|| compile(r"^[a-z0-9][a-z0-9_.-]*$"));
static DECK_HOST_USERNAME: LazyLock<Regex> = LazyLock::new(|| compile(r"^[^\s/]+$"));

/// A member or administrator account. `Debug` redacts the credentials, like the Ecto
/// schema's `redact: true` fields.
#[derive(Clone, PartialEq, Eq)]
pub struct User {
    /// Primary key.
    pub id: i64,
    /// Lowercase login name.
    pub username: String,
    /// Name shown in the UI.
    pub display_name: String,
    /// `admin` or `member`.
    pub role: String,
    /// When an administrator disabled the account.
    pub disabled_at: Option<UtcDateTime>,
    /// bcrypt hash; only administrators sign in with a password.
    pub hashed_password: Option<String>,
    /// Discord snowflake of a Discord-linked account.
    pub discord_id: Option<String>,
    /// Discord avatar.
    pub avatar_url: Option<String>,
    /// Deck host usernames and ManaVault origin.
    pub moxfield_username: Option<String>,
    /// Archidekt username.
    pub archidekt_username: Option<String>,
    /// Personal ManaVault origin.
    pub manavault_url: Option<String>,
    /// Decrypted ManaVault API key (`nil` when absent or unreadable).
    pub manavault_api_key: Option<String>,
    /// Color palette.
    pub palette: String,
    /// Surface style.
    pub theme_style: String,
    /// Creation time.
    pub inserted_at: UtcDateTime,
    /// Last update.
    pub updated_at: UtcDateTime,
    /// When the session's password (or Discord) authentication happened (virtual).
    pub authenticated_at: Option<UtcDateTime>,
}

/// Shown in place of a redacted field (Ecto's `inspect` output).
pub const REDACTED: &str = "**redacted**";

/// `Some("**redacted**")` for a present secret.
pub fn redacted<T>(value: Option<&T>) -> Option<&'static str> {
    value.map(|_| REDACTED)
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("display_name", &self.display_name)
            .field("role", &self.role)
            .field("disabled_at", &self.disabled_at)
            .field("hashed_password", &redacted(self.hashed_password.as_ref()))
            .field("discord_id", &self.discord_id)
            .field("avatar_url", &self.avatar_url)
            .field("moxfield_username", &self.moxfield_username)
            .field("archidekt_username", &self.archidekt_username)
            .field("manavault_url", &self.manavault_url)
            .field(
                "manavault_api_key",
                &redacted(self.manavault_api_key.as_ref()),
            )
            .field("palette", &self.palette)
            .field("theme_style", &self.theme_style)
            .field("inserted_at", &self.inserted_at)
            .field("updated_at", &self.updated_at)
            .field("authenticated_at", &self.authenticated_at)
            .finish()
    }
}

impl User {
    /// Whether the user is an administrator.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// The user as the API renders it.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(UserJson {
            id: self.id,
            username: &self.username,
            display_name: &self.display_name,
            discord_id: self.discord_id.as_deref(),
            avatar_url: self.avatar_url.as_deref(),
            moxfield_username: self.moxfield_username.as_deref(),
            archidekt_username: self.archidekt_username.as_deref(),
            manavault_url: self.manavault_url.as_deref(),
            has_manavault_api_key: self.manavault_api_key.is_some(),
            has_password: self.hashed_password.is_some(),
            palette: &self.palette,
            theme_style: &self.theme_style,
            role: &self.role,
            disabled: self.disabled_at.is_some(),
            inserted_at: self.inserted_at,
        })
        .unwrap_or(Value::Null)
    }
}

#[derive(Serialize)]
struct UserJson<'a> {
    id: i64,
    username: &'a str,
    display_name: &'a str,
    discord_id: Option<&'a str>,
    avatar_url: Option<&'a str>,
    moxfield_username: Option<&'a str>,
    archidekt_username: Option<&'a str>,
    manavault_url: Option<&'a str>,
    has_manavault_api_key: bool,
    has_password: bool,
    palette: &'a str,
    theme_style: &'a str,
    role: &'a str,
    disabled: bool,
    inserted_at: UtcDateTime,
}

/// A new account: the bootstrap administrator, or one an administrator creates.
#[derive(Clone, Default, Deserialize)]
pub struct NewAccount {
    /// Username (trimmed and lowercased before storage).
    #[serde(default)]
    pub username: Option<String>,
    /// Display name, defaulting to the username.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Plain password (hashed before storage).
    #[serde(default)]
    pub password: Option<String>,
    /// Role; `member` unless given.
    #[serde(default)]
    pub role: Option<String>,
}

/// `PATCH /api/session/user`: the signed-in member's profile.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProfileUpdate {
    /// Display name.
    #[serde(default)]
    pub display_name: Patch<String>,
    /// Moxfield username; blank clears it.
    #[serde(default)]
    pub moxfield_username: Patch<String>,
    /// Archidekt username; blank clears it.
    #[serde(default)]
    pub archidekt_username: Patch<String>,
    /// ManaVault origin; blank clears it.
    #[serde(default)]
    pub manavault_url: Patch<String>,
    /// ManaVault API key: blank keeps the stored key, `null` clears it.
    #[serde(default)]
    pub manavault_api_key: Patch<String>,
}

/// `PATCH /api/session/appearance`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AppearanceUpdate {
    /// Color palette id.
    #[serde(default)]
    pub palette: Patch<String>,
    /// Surface style id.
    #[serde(default)]
    pub theme_style: Patch<String>,
}

/// `PATCH /api/session/password`.
#[derive(Clone, Deserialize)]
pub struct PasswordUpdate {
    /// The new password.
    pub password: String,
    /// Must match `password` when given.
    #[serde(default)]
    pub password_confirmation: Patch<String>,
}

/// `PATCH /api/admin/users/:id`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AccountUpdate {
    /// Username.
    #[serde(default)]
    pub username: Patch<String>,
    /// Display name.
    #[serde(default)]
    pub display_name: Patch<String>,
    /// Role.
    #[serde(default)]
    pub role: Patch<String>,
    /// Disables (signing the account out everywhere) or re-enables the account.
    #[serde(default)]
    pub disabled: Option<bool>,
}

/// `PATCH /api/admin/settings`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SettingsUpdate {
    /// Whether anyone may register.
    #[serde(default)]
    pub registration_enabled: Patch<bool>,
    /// Games before this date leave out detailed statistics; `null` counts every game.
    #[serde(default)]
    pub detailed_stats_from: Patch<IsoDate>,
}

/// `POST /api/session/api-keys`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct NewApiKey {
    /// A label for the key.
    #[serde(default)]
    pub name: Option<String>,
}

/// Trims and lowercases a username.
pub fn normalize_username(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_lowercase())
}

/// The display name, or the username when it is missing or empty.
pub fn default_display_name(
    display_name: Option<String>,
    username: Option<&String>,
) -> Option<String> {
    match display_name {
        None => username.cloned(),
        Some(value) if value.is_empty() => username.cloned(),
        Some(value) => Some(value.trim().to_owned()),
    }
}

/// Username, display name, and role rules.
pub fn validate_account_fields(
    cs: &mut Validator,
    username: Option<&str>,
    display_name: Option<&str>,
    role: Option<&str>,
) {
    cs.required("username", username);
    cs.required("display_name", display_name);
    cs.required("role", role);
    cs.length("username", username, Some(3), Some(40));
    cs.format(
        "username",
        username,
        &USERNAME,
        "may only contain letters, numbers, dots, dashes, and underscores",
    );
    cs.length("display_name", display_name, Some(1), Some(80));
    cs.inclusion("role", role, &ROLES);
}

/// Password rules: required, 12 to 72 characters, at most 72 bytes (bcrypt's limit).
pub fn validate_password(cs: &mut Validator, password: Option<&str>) {
    cs.required("password", password);
    cs.length("password", password, Some(12), Some(72));
    if cs.is_valid() {
        cs.max_bytes("password", password, 72);
    }
}

/// Deck-host username rules.
pub fn validate_deck_host_username(cs: &mut Validator, field: &str, value: Option<&str>) {
    cs.length(field, value, None, Some(80));
    cs.format(
        field,
        value,
        &DECK_HOST_USERNAME,
        "must be a username, not a URL",
    );
}

/// Validates a profile update and normalizes it: names trimmed, blank deck-host fields
/// cleared, a blank API key left unchanged, and the ManaVault URL reduced to its origin.
pub fn profile_changes(
    user: &User,
    update: &ProfileUpdate,
    allow_insecure: impl Fn(&str) -> bool,
) -> Result<ProfileUpdate, ValidationError> {
    let mut cs = Validator::new();
    let manavault_api_key = match &update.manavault_api_key {
        Patch::Set(Some(key)) if key.trim().is_empty() => Patch::Unchanged,
        other => other.clone().trimmed(),
    };
    let display_name = update.display_name.clone().trimmed();
    let moxfield_username = update.moxfield_username.clone().trimmed();
    let archidekt_username = update.archidekt_username.clone().trimmed();
    let mut manavault_url = update.manavault_url.clone().trimmed();

    let display = display_name.clone().or(Some(user.display_name.clone()));
    cs.required("display_name", display.as_ref());
    cs.length(
        "display_name",
        display.as_deref().filter(|_| display_name.is_set()),
        Some(1),
        Some(80),
    );
    if let Patch::Set(value) = &moxfield_username {
        validate_deck_host_username(&mut cs, "moxfield_username", value.as_deref());
    }
    if let Patch::Set(value) = &archidekt_username {
        validate_deck_host_username(&mut cs, "archidekt_username", value.as_deref());
    }
    if let Patch::Set(value) = &manavault_url {
        cs.length("manavault_url", value.as_deref(), None, Some(2048));
    }
    if let Patch::Set(value) = &manavault_api_key {
        cs.length("manavault_api_key", value.as_deref(), None, Some(512));
    }
    if let Patch::Set(Some(value)) = &manavault_url
        && value.as_str() != user.manavault_url.as_deref().unwrap_or_default()
    {
        match crate::decklists::destination::normalize_origin(value, &allow_insecure) {
            Ok(origin) => manavault_url = Patch::Set(Some(origin)),
            Err(message) => cs.add_error("manavault_url", message),
        }
    }
    cs.finish()?;
    Ok(ProfileUpdate {
        display_name,
        moxfield_username,
        archidekt_username,
        manavault_url,
        manavault_api_key,
    })
}
