//! Result drafts (`discord_result_drafts`): the `/won` form's private state and the
//! `/log` web handoff. A draft is bound to a reporter, guild, channel, and a snapshot of
//! the staged roster, and expires after an hour.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::db::UtcDateTime;

/// Draft lifetime.
pub const LIFETIME_SECONDS: i64 = 3600;

/// A stored draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultDraft {
    /// UUID.
    pub id: String,
    /// The staged game.
    pub pending_game_id: i64,
    /// Who opened it.
    pub discord_id: String,
    /// Where it was opened.
    pub guild_id: String,
    /// Where it was opened.
    pub channel_id: String,
    /// [`super::pending::PendingGame::snapshot`] when opened.
    pub snapshot: Vec<u8>,
    /// Form state as JSON.
    pub data: String,
    /// Expiry.
    pub expires_at: UtcDateTime,
}

/// `Ecto.UUID.cast/1`: the canonical lowercase hyphenated form.
pub fn cast_uuid(id: &str) -> Option<String> {
    uuid::Uuid::parse_str(id)
        .ok()
        .map(|uuid| uuid.hyphenated().to_string())
}

/// Drops expired drafts.
pub async fn delete_expired(
    conn: &mut SqliteConnection,
    now: UtcDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM discord_result_drafts WHERE expires_at < ?",
        now
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Inserts a draft.
pub async fn insert(conn: &mut SqliteConnection, draft: &ResultDraft) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO discord_result_drafts (id, pending_game_id, discord_id, guild_id, channel_id, snapshot, data, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        draft.id,
        draft.pending_game_id,
        draft.discord_id,
        draft.guild_id,
        draft.channel_id,
        draft.snapshot,
        draft.data,
        draft.expires_at
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A draft by id.
pub async fn get(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<Option<ResultDraft>, sqlx::Error> {
    sqlx::query_as!(
        ResultDraft,
        r#"SELECT id AS "id!", pending_game_id, discord_id, guild_id, channel_id, snapshot, data,
                  expires_at AS "expires_at: UtcDateTime"
           FROM discord_result_drafts WHERE id = ?"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Replaces a draft's data.
pub async fn update_data(
    conn: &mut SqliteConnection,
    id: &str,
    data: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE discord_result_drafts SET data = ? WHERE id = ?",
        data,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Deletes a draft.
pub async fn delete(conn: &mut SqliteConnection, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM discord_result_drafts WHERE id = ?", id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Accepts a JSON string or number as text.
fn lenient_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::String(text)) => Some(text),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    })
}

/// Accepts a JSON number or numeric string as an integer.
fn lenient_integer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        _ => None,
    })
}

/// The `/log` handoff's data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebDraftData {
    /// Winner chosen in Discord, if any.
    pub winner: Option<String>,
    /// Minutes since the game started, when the link was made.
    #[serde(deserialize_with = "lenient_integer")]
    pub duration: Option<i64>,
}

/// A card choice in the `/won` form (`CardChoice`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardChoice {
    /// What the reporter typed, or the resolved card's name.
    pub name: String,
    /// The resolved card.
    pub id: Option<String>,
    /// Matches to choose from.
    pub candidates: Vec<Candidate>,
    /// Why the name did not resolve.
    pub error: Option<String>,
}

/// A candidate card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// Card id.
    pub id: String,
    /// Card name.
    pub name: String,
}

/// A player's commander entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommanderChoices {
    /// Commander.
    pub commander: CardChoice,
    /// Partner or Background.
    pub partner: CardChoice,
}

/// Which commander slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The commander.
    Commander,
    /// The partner or Background.
    Partner,
}

impl Role {
    /// `commander` or `partner`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Commander => "commander",
            Self::Partner => "partner",
        }
    }
}

impl CommanderChoices {
    /// The choice in `role`.
    pub fn get(&self, role: Role) -> &CardChoice {
        match role {
            Role::Commander => &self.commander,
            Role::Partner => &self.partner,
        }
    }

    /// The choice in `role`, mutably.
    pub fn get_mut(&mut self, role: Role) -> &mut CardChoice {
        match role {
            Role::Commander => &mut self.commander,
            Role::Partner => &mut self.partner,
        }
    }
}

/// The `/won` form's state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WonDraftData {
    /// Winner's Discord id.
    pub winner: Option<String>,
    /// Win condition key.
    pub win_condition: Option<String>,
    /// Minutes, as typed.
    #[serde(deserialize_with = "lenient_text")]
    pub duration: Option<String>,
    /// Turns, as typed.
    pub turns: Option<String>,
    /// MVP card name.
    pub mvp: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// The details modal was submitted.
    pub details_done: bool,
    /// Kills per Discord id, as typed (a present blank means unknown).
    pub kills: BTreeMap<String, String>,
    /// Resolved MVP card.
    pub mvp_id: Option<String>,
    /// MVP matches to choose from.
    pub mvp_candidates: Vec<Candidate>,
    /// Why the MVP did not resolve.
    pub mvp_error: Option<String>,
    /// Commander entries per Discord id.
    pub commanders: BTreeMap<String, CommanderChoices>,
}
