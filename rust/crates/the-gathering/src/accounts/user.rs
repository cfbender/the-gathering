//! The `users` table.

use std::sync::LazyLock;

use crate::regex::{Regex, compile};
use serde::Serialize;
use serde_json::Value;

use crate::changeset::{Change, Changeset};
use crate::db::UtcDateTime;

/// Salt for encrypted stored credentials (such as ManaVault API keys).
pub const ENCRYPTED_STRING_SALT: &str = "the_gathering.accounts.encrypted_string";

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

/// Fields an account form submits.
#[derive(Clone, Debug, Default)]
pub struct AccountFields {
    /// Normalized username.
    pub username: Option<String>,
    /// Display name, defaulting to the username.
    pub display_name: Option<String>,
    /// Plain password (hashed before storage).
    pub password: Option<String>,
    /// Role.
    pub role: Option<String>,
}

/// `normalize_username/1`: trim and lowercase.
pub fn normalize_username(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_lowercase())
}

/// `default_display_name/1`.
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

/// `validate_account_fields/1`.
pub fn validate_account_fields(
    cs: &mut Changeset<'_>,
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

/// `validate_password/1` without hashing: required, 12 to 72 characters, at most 72 bytes.
pub fn validate_password(cs: &mut Changeset<'_>, password: Option<&str>) {
    cs.required("password", password);
    cs.length("password", password, Some(12), Some(72));
    if cs.is_valid() {
        cs.max_bytes("password", password, 72);
    }
}

/// Validates deck-host usernames (`profile_changeset/2`).
pub fn validate_deck_host_username(cs: &mut Changeset<'_>, field: &str, value: Option<&str>) {
    cs.length(field, value, None, Some(80));
    cs.format(
        field,
        value,
        &DECK_HOST_USERNAME,
        "must be a username, not a URL",
    );
}

/// The profile form, cast from params.
#[derive(Debug, Default)]
pub struct ProfileChanges {
    /// New display name.
    pub display_name: Change<String>,
    /// New Moxfield username.
    pub moxfield_username: Change<String>,
    /// New Archidekt username.
    pub archidekt_username: Change<String>,
    /// New ManaVault origin.
    pub manavault_url: Change<String>,
    /// New ManaVault key; a blank string keeps the stored key, `null` clears it.
    pub manavault_api_key: Change<String>,
}

/// `profile_changeset/2`. Returns the changes to apply, or errors.
pub fn profile_changes(
    user: &User,
    params: &Value,
    allow_insecure: impl Fn(&str) -> bool,
) -> Result<ProfileChanges, crate::error::Errors> {
    let mut cs = Changeset::new(params);
    // A blank key means "leave the stored key alone"; only an explicit nil clears it.
    let manavault_api_key = match cs.raw("manavault_api_key") {
        Some(Value::String(key)) if key.trim().is_empty() => Change::Unchanged,
        _ => cs
            .string("manavault_api_key")
            .map(|key| key.trim().to_owned()),
    };
    let display_name = cs.string("display_name").map(|name| name.trim().to_owned());
    let trimmed = |change: Change<String>| change.map(|value| value.trim().to_owned());
    let moxfield_username = trimmed(cs.string("moxfield_username"));
    let archidekt_username = trimmed(cs.string("archidekt_username"));
    let mut manavault_url = trimmed(cs.string("manavault_url"));

    let display = display_name.clone().or(Some(user.display_name.clone()));
    cs.required("display_name", display.as_ref());
    cs.length(
        "display_name",
        display.as_deref().filter(|_| display_name.is_set()),
        Some(1),
        Some(80),
    );
    if let Change::Set(value) = &moxfield_username {
        validate_deck_host_username(&mut cs, "moxfield_username", value.as_deref());
    }
    if let Change::Set(value) = &archidekt_username {
        validate_deck_host_username(&mut cs, "archidekt_username", value.as_deref());
    }
    if let Change::Set(value) = &manavault_url {
        cs.length("manavault_url", value.as_deref(), None, Some(2048));
    }
    if let Change::Set(value) = &manavault_api_key {
        cs.length("manavault_api_key", value.as_deref(), None, Some(512));
    }
    if let Change::Set(Some(value)) = &manavault_url
        && !value.is_empty()
        && value.as_str() != user.manavault_url.as_deref().unwrap_or_default()
    {
        match crate::decklists::destination::normalize_origin(value, &allow_insecure) {
            Ok(origin) => manavault_url = Change::Set(Some(origin)),
            Err(message) => cs.add_error("manavault_url", message),
        }
    }
    cs.finish()?;
    Ok(ProfileChanges {
        display_name,
        moxfield_username,
        archidekt_username,
        manavault_url,
        manavault_api_key,
    })
}
