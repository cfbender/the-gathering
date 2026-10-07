//! A participant at a webcam table: the seat state a player publishes about themselves
//! (presence metas, the room's saved seats, and the snapshot's `seats`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::turns::TurnSeat;

/// Life every seat starts a game with.
pub const STARTING_LIFE: i64 = 40;

fn starting_life() -> i64 {
    STARTING_LIFE
}

/// A free-form counter a seat shares ("Lands: 7").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomCounter {
    /// Stable id (renames keep it).
    pub id: String,
    /// Label.
    pub label: String,
    /// 0..=100.
    pub value: i64,
}

/// An anthem or combat buff a seat shares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatEffect {
    /// Stable id.
    pub id: String,
    /// Card or effect name.
    pub name: String,
    /// Power modifier.
    pub power: i64,
    /// Toughness modifier.
    pub toughness: i64,
    /// Which creatures it applies to.
    pub conditions: Vec<String>,
    /// Granted keywords.
    pub keywords: Vec<String>,
}

/// A seat (the channel's participant).
#[allow(clippy::struct_excessive_bools)] // The wire format the frontend reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    /// The connection's peer id (a UUID; replaced on every reconnect).
    pub peer_id: String,
    /// The seated player.
    pub player_id: i64,
    /// The player's name.
    pub player_name: String,
    /// Join time (ms); the default seat order is join order.
    #[serde(default)]
    pub joined_at: i64,
    /// Life total.
    #[serde(default = "starting_life")]
    pub life: i64,
    /// Camera turned off.
    #[serde(default)]
    pub camera_off: bool,
    /// The camera's native rows, so viewers crop their own frames.
    #[serde(default)]
    pub camera_height: Option<i64>,
    /// Whether crops of this board may be uploaded as recognizer training data.
    #[serde(default)]
    pub shares_corrections: bool,
    /// Poison counters.
    #[serde(default)]
    pub poison: i64,
    /// Rad counters.
    #[serde(default)]
    pub rad: i64,
    /// Casts per commander name.
    #[serde(default)]
    pub commander_casts: BTreeMap<String, i64>,
    /// Damage taken per source player id, per commander name.
    #[serde(default)]
    pub commander_damage: BTreeMap<String, BTreeMap<String, i64>>,
    /// Shared custom counters.
    #[serde(default)]
    pub custom_counters: Vec<CustomCounter>,
    /// Shared combat buffs.
    #[serde(default)]
    pub combat_effects: Vec<CombatEffect>,
    /// The one seat allowed to see this board, if it is hidden from others.
    #[serde(default)]
    pub reveal_to: Option<String>,
    /// Knocked out.
    #[serde(default)]
    pub eliminated: bool,
    /// Watching a started game rather than playing.
    #[serde(default)]
    pub spectator: bool,
    /// Chosen deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck_id: Option<i64>,
    /// Chosen deck's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck_name: Option<String>,
}

impl Seat {
    /// A new seat with starting values.
    pub fn new(peer_id: String, player_id: i64, player_name: String, joined_at: i64) -> Self {
        Self {
            peer_id,
            player_id,
            player_name,
            joined_at,
            life: STARTING_LIFE,
            camera_off: false,
            camera_height: None,
            shares_corrections: false,
            poison: 0,
            rad: 0,
            commander_casts: BTreeMap::new(),
            commander_damage: BTreeMap::new(),
            custom_counters: Vec::new(),
            combat_effects: Vec::new(),
            reveal_to: None,
            eliminated: false,
            spectator: false,
            deck_id: None,
            deck_name: None,
        }
    }

    /// Who this seat is, for attribution.
    pub fn holder(&self) -> Holder {
        Holder { peer_id: self.peer_id.clone(), player_name: self.player_name.clone() }
    }

    /// What a seat starts a game with (a rematch resets these).
    #[must_use]
    pub fn reset(self) -> Self {
        Self {
            life: STARTING_LIFE,
            poison: 0,
            rad: 0,
            commander_casts: BTreeMap::new(),
            commander_damage: BTreeMap::new(),
            custom_counters: Vec::new(),
            combat_effects: Vec::new(),
            eliminated: false,
            ..self
        }
    }
}

impl TurnSeat for Seat {
    fn player_id(&self) -> i64 {
        self.player_id
    }

    fn eliminated(&self) -> bool {
        self.eliminated
    }
}

/// A seat named for attribution (the monarch, who changed cards).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    /// Peer id.
    pub peer_id: String,
    /// Player name.
    pub player_name: String,
}
