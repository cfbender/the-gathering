//! Importing game history from external data sources.
//!
//! CSV and Mythic Track parse into normalized [`ImportGame`]/[`ImportSeat`] values.
//! [`preview::run`] matches players and decks against existing records, and the commits
//! write the whole batch in one transaction. Games are keyed by `{source, external_id}`,
//! so re-importing the same data skips games that already exist.
//!
//! [`sheet_preview::run`] and [`sheet_commit::run`] reconcile the group's Google Sheet using
//! explicit mappings and update/create/skip choices, preserving existing game identities.
//! [`portable`] exports and restores a whole installation's history.

pub mod commit;
pub mod csv;
pub mod csv_changes;
pub mod csv_transfer;
pub mod etf;
pub mod google_sheet;
pub mod mythic_track;
pub mod portable;
pub mod preview;
pub mod sheet_commit;
pub mod sheet_match;
pub mod sheet_preview;
pub mod sheet_resolution;
pub mod table;

use serde::Serialize;

use crate::db::UtcDateTime;
use crate::error::ApiError;
use crate::games::GamesError;
use crate::validation::ValidationError;

pub use self::preview::Preview;

/// A problem with one input line (`%{line, field, message}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LineError {
    /// 1-based line (CSV) or game position (Mythic Track).
    pub line: i64,
    /// The column or field.
    pub field: String,
    /// What is wrong.
    pub message: String,
}

impl LineError {
    /// Builds an error.
    pub fn new(line: i64, field: &str, message: impl Into<String>) -> Self {
        Self {
            line,
            field: field.to_owned(),
            message: message.into(),
        }
    }
}

/// A game that was skipped rather than rejected (`%{line, message}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Warning {
    /// Game position.
    pub line: i64,
    /// Why it was skipped.
    pub message: String,
}

/// A normalized game produced by every import parser (`Imports.Game`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportGame {
    /// Stable identity within the source.
    pub external_id: String,
    /// The id the file used (CSV `game_id`, Mythic Track GUID).
    pub game_id: String,
    /// When it was played.
    pub played_at: UtcDateTime,
    /// Minutes.
    pub duration_minutes: Option<i64>,
    /// Turns.
    pub turns: Option<i64>,
    /// Win condition key.
    pub win_condition: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Source lines.
    pub lines: Vec<i64>,
    /// Seats in seat order.
    pub seats: Vec<ImportSeat>,
    /// CSV `action` (`create`, `update`, `skip`).
    pub action: Option<String>,
    /// CSV `source` of the game to update.
    pub target_source: Option<String>,
    /// CSV `external_id` of the game to update.
    pub target_external_id: Option<String>,
    /// CSV `portable_id` of the game to update.
    pub target_portable_id: Option<String>,
}

/// A normalized seat (`Imports.Seat`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportSeat {
    /// Source line.
    pub line: i64,
    /// Player name.
    pub player: String,
    /// Discord snowflake (Mythic Track).
    pub discord_id: Option<String>,
    /// Deck name.
    pub deck: String,
    /// Commander name.
    pub commander: String,
    /// Commander's Scryfall id.
    pub commander_card_id: Option<String>,
    /// Partner name.
    pub partner_name: Option<String>,
    /// Partner's Scryfall id.
    pub partner_card_id: Option<String>,
    /// WUBRG letters.
    pub color_identity: Option<String>,
    /// Deck list link.
    pub decklist_url: Option<String>,
    /// Seat number.
    pub seat: i64,
    /// `win`, `loss`, or `draw`.
    pub result: String,
    /// Kills.
    pub kills: Option<i64>,
    /// MVP card name.
    pub mvp_card: Option<String>,
    /// MVP card id.
    pub mvp_card_id: Option<String>,
}

/// A commit's counts (`%{created, updated, skipped, game_ids}`; `updated` only for CSV).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ImportResult {
    /// Games created.
    pub created: i64,
    /// Games updated (CSV corrections).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    /// Games skipped.
    pub skipped: i64,
    /// Every game the file referred to.
    pub game_ids: Vec<i64>,
}

/// Why an import failed.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The file is invalid; the preview carries its errors (422 with the preview).
    #[error("invalid import")]
    Validation(Box<Preview>),
    /// A rolled-back import with a message (422 `{"errors": {"import": [message]}}`).
    #[error("{0}")]
    Message(String),
    /// A record failed validation (422 with the changeset errors).
    #[error("invalid record")]
    Invalid(ValidationError),
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<GamesError> for ImportError {
    fn from(error: GamesError) -> Self {
        match error {
            GamesError::Invalid(errors) => Self::Invalid(errors),
            GamesError::Database(error) => Self::Database(error),
            GamesError::NotFound => Self::Database(sqlx::Error::RowNotFound),
            GamesError::BadRequest => Self::Invalid(ValidationError::single("base", "is invalid")),
        }
    }
}

impl From<crate::games::ResolveError> for ImportError {
    fn from(error: crate::games::ResolveError) -> Self {
        match error {
            crate::games::ResolveError::Invalid(errors) => Self::Invalid(errors),
            crate::games::ResolveError::Database(error) => Self::Database(error),
            crate::games::ResolveError::DiscordIdentityConflict => Self::Invalid(
                ValidationError::single("discord_id", "belongs to another account"),
            ),
        }
    }
}

impl ImportError {
    /// The controller rendering: an `import` field error for messages, changeset errors
    /// otherwise. `Validation` is rendered by the controllers themselves.
    pub fn into_api(self) -> ApiError {
        match self {
            Self::Message(message) => {
                ApiError::Validation(ValidationError::single("import", message))
            }
            Self::Invalid(errors) => ApiError::Validation(errors),
            Self::Database(error) => error.into(),
            Self::Validation(_) => ApiError::BadRequest,
        }
    }
}

/// `String.trim/1` then `nil` for blank.
pub(crate) fn blank_to_nil(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

/// Integer.parse/1 requiring the whole string: an optional sign and decimal digits.
pub(crate) fn parse_integer(value: &str) -> Option<i64> {
    let digits = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if let Some(positive) = value.strip_prefix('+') {
        positive.parse().ok()
    } else {
        value.parse().ok()
    }
}

/// `Ecto.UUID.cast/1` succeeds: a hyphenated UUID in either case (or 16 raw bytes).
pub(crate) fn uuid_castable(value: &str) -> bool {
    if value.len() == 16 {
        return true;
    }
    value.len() == 36
        && value.char_indices().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_and_uuid_casts() {
        assert_eq!(parse_integer("+3"), Some(3));
        assert_eq!(parse_integer("-2"), Some(-2));
        assert_eq!(parse_integer("3x"), None);
        assert_eq!(parse_integer(""), None);
        assert!(uuid_castable("8F3A0A44-0000-4000-8000-000000000001"));
        assert!(!uuid_castable("8f3a0a44-0000-4000-8000-00000000000"));
    }
}
