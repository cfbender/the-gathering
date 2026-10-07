//! The shared list of identified cards.
//!
//! Entries stay client-shaped JSON (`id`, `ownerPeerId`, `byPlayerName`, `at`, `card`), so
//! fields the browser adds pass through untouched; the server only validates and dedupes.

use serde_json::Value;

use super::seat::Seat;

const MAX_CARDS: usize = 500;
const MAX_ENTRY_BYTES: usize = 2048;

/// A change to the card list.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// A card was identified on `entry.ownerPeerId`'s board.
    Identified(Value),
    /// An entry was removed.
    Removed(String),
    /// Every card on one board was cleared.
    Cleared(String),
}

impl Change {
    /// Parses the channel payload (`type` plus its fields).
    pub fn parse(payload: &Value) -> Option<Self> {
        match payload.get("type")?.as_str()? {
            "card_identified" => payload
                .get("entry")
                .map(|entry| Self::Identified(entry.clone())),
            "card_removed" => payload
                .get("id")?
                .as_str()
                .map(|id| Self::Removed(id.to_owned())),
            "cards_cleared" => payload
                .get("ownerPeerId")?
                .as_str()
                .map(|id| Self::Cleared(id.to_owned())),
            _ => None,
        }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Applies `change`; `None` when it is invalid.
pub fn update(cards: &[Value], change: &Change, seats: &[Seat]) -> Option<Vec<Value>> {
    match change {
        Change::Identified(entry) => {
            let owner = field(entry, "ownerPeerId")?;
            if !valid(entry)
                || !seats
                    .iter()
                    .any(|seat| seat.peer_id == owner && seat.reveal_to.is_none())
            {
                return None;
            }
            let mut seen = std::collections::HashSet::new();
            let cards: Vec<Value> = cards
                .iter()
                .chain(std::iter::once(entry))
                .filter(|card| seen.insert(dedupe_key(card)))
                .take(MAX_CARDS)
                .cloned()
                .collect();
            Some(cards)
        }
        Change::Removed(id) => Some(
            cards
                .iter()
                .filter(|card| field(card, "id") != Some(id))
                .cloned()
                .collect(),
        ),
        Change::Cleared(owner) => Some(
            cards
                .iter()
                .filter(|card| field(card, "ownerPeerId") != Some(owner))
                .cloned()
                .collect(),
        ),
    }
}

fn dedupe_key(card: &Value) -> (Option<String>, Option<String>) {
    let owner = field(card, "ownerPeerId").map(str::to_owned);
    let name = card
        .get("card")
        .and_then(|card| field(card, "name"))
        .map(|name| name.trim().to_lowercase());
    (owner, name)
}

fn valid(entry: &Value) -> bool {
    let Some(card) = entry.get("card") else {
        return false;
    };
    let strings = [
        field(entry, "id"),
        field(entry, "ownerPeerId"),
        field(entry, "byPlayerName"),
        field(card, "id"),
        field(card, "name"),
        field(card, "set"),
    ];
    entry.get("at").is_some_and(|at| at.is_i64() || at.is_u64())
        && strings
            .iter()
            .all(|value| value.is_some_and(|value| (1..=300).contains(&value.len())))
        && serde_json::to_vec(entry).is_ok_and(|encoded| encoded.len() <= MAX_ENTRY_BYTES)
}
