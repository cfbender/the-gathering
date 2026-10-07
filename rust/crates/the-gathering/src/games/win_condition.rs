//! Canonical game win conditions and Mythic Track's persisted numeric mapping
//! (`TheGathering.Games.WinCondition`).

use std::fmt;

use serde::{Deserialize, Serialize};

/// How a game was won.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum WinCondition {
    /// Damage.
    Damage,
    /// Infinite Combo.
    InfiniteCombo,
    /// Mill.
    Mill,
    /// Poison.
    Poison,
    /// On-card Alternate Win Con.
    AlternateWinCon,
    /// Hard Lock.
    HardLock,
    /// Commander Damage.
    CommanderDamage,
    /// Draw.
    Draw,
    /// Non-Combat Damage.
    NonCombatDamage,
    /// Combat Damage.
    CombatDamage,
    /// Concede.
    Concede,
    /// Unknown.
    Unknown,
}

const VALUES: [(WinCondition, &str, &str); 12] = [
    (WinCondition::Damage, "damage", "Damage"),
    (
        WinCondition::InfiniteCombo,
        "infinite_combo",
        "Infinite Combo",
    ),
    (WinCondition::Mill, "mill", "Mill"),
    (WinCondition::Poison, "poison", "Poison"),
    (
        WinCondition::AlternateWinCon,
        "alternate_win_con",
        "On-card Alternate Win Con",
    ),
    (WinCondition::HardLock, "hard_lock", "Hard Lock"),
    (
        WinCondition::CommanderDamage,
        "commander_damage",
        "Commander Damage",
    ),
    (WinCondition::Draw, "draw", "Draw"),
    (
        WinCondition::NonCombatDamage,
        "non_combat_damage",
        "Non-Combat Damage",
    ),
    (WinCondition::CombatDamage, "combat_damage", "Combat Damage"),
    (WinCondition::Concede, "concede", "Concede"),
    (WinCondition::Unknown, "unknown", "Unknown"),
];

impl WinCondition {
    /// `values/0`: every condition in display order.
    pub fn all() -> impl Iterator<Item = Self> {
        VALUES.iter().map(|(value, _, _)| *value)
    }

    /// The stored key.
    pub fn as_str(self) -> &'static str {
        VALUES
            .iter()
            .find(|(value, _, _)| *value == self)
            .map_or("unknown", |(_, key, _)| key)
    }

    /// Parses a stored key.
    pub fn parse(key: &str) -> Option<Self> {
        VALUES
            .iter()
            .find(|(_, stored, _)| *stored == key)
            .map(|(value, _, _)| *value)
    }

    /// `label/1`.
    pub fn label(self) -> &'static str {
        VALUES
            .iter()
            .find(|(value, _, _)| *value == self)
            .map_or("Unknown", |(_, _, label)| label)
    }

    /// `label/1` for an optional key (`nil` and unknown keys are "Unknown").
    pub fn label_of(value: Option<Self>) -> &'static str {
        value.map_or("Unknown", Self::label)
    }

    /// `from_mythic/1`.
    pub fn from_mythic(value: i64) -> Self {
        match value {
            1 => Self::Damage,
            2 => Self::InfiniteCombo,
            3 => Self::Mill,
            4 => Self::Poison,
            5 => Self::AlternateWinCon,
            6 => Self::HardLock,
            7 => Self::CommanderDamage,
            8 => Self::Draw,
            9 => Self::NonCombatDamage,
            10 => Self::CombatDamage,
            11 => Self::Concede,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for WinCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_labels_and_mythic_mapping() {
        assert_eq!(
            WinCondition::parse("alternate_win_con").map(WinCondition::label),
            Some("On-card Alternate Win Con")
        );
        assert_eq!(WinCondition::label_of(None), "Unknown");
        assert_eq!(WinCondition::from_mythic(10), WinCondition::CombatDamage);
        assert_eq!(WinCondition::from_mythic(42), WinCondition::Unknown);
        assert_eq!(WinCondition::all().count(), 12);
        assert_eq!(
            serde_json::to_value(WinCondition::NonCombatDamage).unwrap(),
            "non_combat_damage"
        );
    }
}
