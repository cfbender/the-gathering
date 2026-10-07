//! Typed inputs for recording and editing players, decks, and games.
//!
//! Every field is a [`Patch`]: left out keeps the stored value (or the default for new
//! records), `null` clears it. Blank strings clear optional text, as forms submit `""`
//! for an emptied field. Provenance fields (`discord_id`, `source`, `external_id`) are set
//! by trusted callers; the web handlers clear them before passing a request body on.

use serde::Deserialize;

use crate::db::UtcDateTime;
use crate::patch::Patch;

/// A new player or changes to one.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PlayerInput {
    /// Name, unique among players.
    #[serde(default)]
    pub name: Patch<String>,
    /// When the player was archived; `null` restores them.
    #[serde(default)]
    pub archived_at: Patch<UtcDateTime>,
    /// The linked Discord account (trusted callers only; ignored on update).
    #[serde(default)]
    pub discord_id: Option<String>,
}

impl PlayerInput {
    /// A player with this name.
    pub fn named(name: &str) -> Self {
        Self {
            name: Patch::Set(Some(name.to_owned())),
            ..Self::default()
        }
    }
}

/// A new deck or changes to one.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct DeckInput {
    /// The owning player (new decks only).
    #[serde(default)]
    pub player_id: Patch<i64>,
    /// Name, unique per player.
    #[serde(default)]
    pub name: Patch<String>,
    /// Catalog id of the commander.
    #[serde(default)]
    pub commander_card_id: Patch<String>,
    /// Commander name.
    #[serde(default)]
    pub commander_name: Patch<String>,
    /// Chosen commander printing.
    #[serde(default)]
    pub commander_printing_id: Patch<String>,
    /// Catalog id of the partner.
    #[serde(default)]
    pub partner_card_id: Patch<String>,
    /// Partner name.
    #[serde(default)]
    pub partner_name: Patch<String>,
    /// Chosen partner printing.
    #[serde(default)]
    pub partner_printing_id: Patch<String>,
    /// Color identity letters (`WUBRG`, or empty for colorless).
    #[serde(default)]
    pub color_identity: Patch<String>,
    /// Public deck-list link.
    #[serde(default)]
    pub decklist_url: Patch<String>,
    /// When the deck was retired; `null` brings it back.
    #[serde(default)]
    pub archived_at: Patch<UtcDateTime>,
    /// Whether the deck chooser may pick it (`null` keeps the current value).
    #[serde(default)]
    pub included_for_play: Patch<bool>,
}

impl DeckInput {
    /// A deck named `name` for `player_id` with this commander.
    pub fn new(player_id: i64, name: &str) -> Self {
        Self {
            player_id: Patch::Set(Some(player_id)),
            name: Patch::Set(Some(name.to_owned())),
            ..Self::default()
        }
    }
}

/// A seat of a game.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SeatInput {
    /// The existing seat this updates (unknown ids insert a new seat).
    #[serde(default)]
    pub id: Option<i64>,
    /// The seated player.
    #[serde(default)]
    pub player_id: Patch<i64>,
    /// The deck they played.
    #[serde(default)]
    pub deck_id: Patch<i64>,
    /// Turn order, from 1.
    #[serde(default)]
    pub seat: Patch<i64>,
    /// `win`, `loss`, or `draw`.
    #[serde(default)]
    pub result: Patch<String>,
    /// Kills, 0 to 9.
    #[serde(default)]
    pub kills: Patch<i64>,
    /// The turn the player was eliminated on.
    #[serde(default)]
    pub eliminated_turn: Patch<i64>,
    /// Who eliminated them.
    #[serde(default)]
    pub eliminated_by_player_id: Patch<i64>,
    /// Catalog id of their most valuable card.
    #[serde(default)]
    pub mvp_card_id: Patch<String>,
    /// That card's name.
    #[serde(default)]
    pub mvp_card_name: Patch<String>,
    /// Notes about the seat.
    #[serde(default)]
    pub notes: Patch<String>,
}

/// A new game or changes to one.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GameInput {
    /// When it was played.
    #[serde(default)]
    pub played_at: Patch<UtcDateTime>,
    /// Length in minutes.
    #[serde(default)]
    pub duration_minutes: Patch<i64>,
    /// Number of turns.
    #[serde(default)]
    pub turns: Patch<i64>,
    /// How it was won (a `WinCondition` key).
    #[serde(default)]
    pub win_condition: Patch<String>,
    /// Notes.
    #[serde(default)]
    pub notes: Patch<String>,
    /// `commander`, `two_headed_giant`, or `five_star`.
    #[serde(default)]
    pub format: Patch<String>,
    /// The seats; given seats replace the game's seats.
    #[serde(default)]
    pub seats: Patch<Vec<SeatInput>>,
    /// Where the game came from (trusted callers only; `manual` by default).
    #[serde(default)]
    pub source: Option<String>,
    /// The game's id at its source (trusted callers only).
    #[serde(default)]
    pub external_id: Option<String>,
}

impl GameInput {
    /// Drops provenance a request body must not set.
    #[must_use]
    pub fn without_provenance(self) -> Self {
        Self {
            source: None,
            external_id: None,
            ..self
        }
    }
}
