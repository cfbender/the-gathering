//! Normalized game data from Discord sources (`TheGathering.Discord.GameReport`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::db::UtcDateTime;

/// A reported game.
#[derive(Clone, Debug, PartialEq)]
pub struct GameReport {
    /// `spellbot:SB12345`.
    pub external_id: String,
    /// Always `discord`.
    pub source: String,
    /// When the game started.
    pub played_at: UtcDateTime,
    /// Guild snowflake.
    pub guild_id: String,
    /// Channel snowflake.
    pub channel_id: String,
    /// Players in seat order.
    pub players: Vec<ReportPlayer>,
    /// At most one winner; empty until someone reports it.
    pub winner_discord_ids: Vec<String>,
    /// Provenance kept with the staged game (message id, embed, reporter).
    pub raw: Map<String, Value>,
    /// Result details from `/won`; `None` for SpellBot observations and bare winners.
    pub details: Option<ReportDetails>,
}

/// A reported player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPlayer {
    /// Discord snowflake.
    pub discord_id: String,
    /// Name shown in SpellBot's embed.
    pub display_name: String,
    /// Commander, when known.
    pub commander_name: Option<String>,
}

/// Result details collected by the `/won` form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportDetails {
    /// Win condition key.
    pub win_condition: Option<String>,
    /// Turns.
    pub turns: Option<i64>,
    /// Minutes.
    pub duration_minutes: Option<i64>,
    /// Notes.
    pub notes: Option<String>,
    /// Kills per Discord id (`None` is unknown).
    pub kills: BTreeMap<String, Option<i64>>,
    /// Decks per Discord id; `None` records "no deck". Players without an entry keep the
    /// reported commander.
    pub commanders: BTreeMap<String, Option<DeckAttrs>>,
    /// The winner's MVP card id.
    pub mvp_card_id: Option<String>,
    /// The winner's MVP card name.
    pub mvp_card_name: Option<String>,
}

/// A catalog-linked deck pairing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeckAttrs {
    /// Commander card id.
    pub commander_card_id: String,
    /// Commander name.
    pub commander_name: String,
    /// Partner card id.
    pub partner_card_id: Option<String>,
    /// Partner name.
    pub partner_name: Option<String>,
    /// Canonical color identity.
    pub color_identity: String,
}
