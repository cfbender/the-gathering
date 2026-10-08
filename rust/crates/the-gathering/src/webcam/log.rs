//! The table log, newest first, kept with the room's durable
//! state so every seat sees the same history and a reload restores it.
//!
//! An entry is `{id, at, text}` plus optional merge metadata (`actor`, `kind`, `life`,
//! `counter`, `roll`) and a `count` once rapid changes coalesce. Ids only grow, so the head
//! always has the largest id; a merge updates its entry in place and keeps that entry's id.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::seat::{Holder, Seat};

const MAX_ENTRIES: usize = 200;
const MERGE_WINDOW_MS: i64 = 5_000;

/// A life change: `name: from → to life`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifeChange {
    /// Player name.
    pub name: String,
    /// Original total.
    pub from: i64,
    /// New total.
    pub to: i64,
}

/// A counter change: `prefix from → to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterChange {
    /// `"Alice poison: "`.
    pub prefix: String,
    /// Original value.
    pub from: i64,
    /// New value.
    pub to: i64,
}

/// A die face or coin side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RollResult {
    /// A die result.
    Number(i64),
    /// `Heads` or `Tails`.
    Face(String),
}

impl fmt::Display for RollResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(number) => write!(f, "{number}"),
            Self::Face(face) => f.write_str(face),
        }
    }
}

/// Every roll in a coalesced roll line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollLine {
    /// `"Alice rolled a d20: "`.
    pub prefix: String,
    /// Results in order.
    pub results: Vec<RollResult>,
}

/// One log line. Constructors return contents with `id` and `at` zero; [`append`] stamps them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Increasing id.
    #[serde(default)]
    pub id: i64,
    /// Last change (ms).
    #[serde(default)]
    pub at: i64,
    /// The line.
    pub text: String,
    /// Peer id of the seat it is about; table-wide entries have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// What kind of change, for merging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// How many changes merged into this line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    /// Life merge metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub life: Option<LifeChange>,
    /// Counter merge metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<CounterChange>,
    /// Roll merge metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll: Option<RollLine>,
}

fn text(text: impl Into<String>) -> LogEntry {
    LogEntry {
        text: text.into(),
        ..LogEntry::default()
    }
}

fn by(actor: &str, kind: impl Into<String>, line: impl Into<String>) -> LogEntry {
    LogEntry {
        actor: Some(actor.to_owned()),
        kind: Some(kind.into()),
        ..text(line)
    }
}

/// Adds `content` at `at` (ms). It merges into the latest entry with the same actor and kind
/// if that entry changed within five seconds; other players' entries do not separate a
/// merge, table-wide entries (no actor) do. Life and counters keep the original value, rolls
/// keep every result.
pub fn append(log: &[LogEntry], content: LogEntry, at: i64) -> Vec<LogEntry> {
    let next = LogEntry {
        id: log.first().map_or(1, |head| head.id + 1),
        at,
        ..content
    };
    match merge_index(log, &next) {
        None => std::iter::once(next)
            .chain(log.iter().cloned())
            .take(MAX_ENTRIES)
            .collect(),
        Some(index) => log
            .iter()
            .enumerate()
            .map(|(position, entry)| {
                if position == index {
                    merge(entry, next.clone())
                } else {
                    entry.clone()
                }
            })
            .collect(),
    }
}

fn merge_index(log: &[LogEntry], next: &LogEntry) -> Option<usize> {
    let (Some(kind), Some(actor)) = (&next.kind, &next.actor) else {
        return None;
    };
    let index = log
        .iter()
        .take_while(|entry| entry.actor.is_some())
        .position(|entry| {
            entry.actor.as_ref() == Some(actor) && entry.kind.as_ref() == Some(kind)
        })?;
    let previous = log.get(index)?;
    (0..=MERGE_WINDOW_MS)
        .contains(&(next.at - previous.at))
        .then_some(index)
}

fn merge(previous: &LogEntry, next: LogEntry) -> LogEntry {
    let mut merged = LogEntry {
        id: previous.id,
        count: Some(previous.count.unwrap_or(1) + 1),
        ..next
    };
    if let (Some(before), Some(after)) = (&previous.life, &merged.life) {
        let life = LifeChange {
            from: before.from,
            ..after.clone()
        };
        merged.text = life_text(&life);
        merged.life = Some(life);
    } else if let (Some(before), Some(after)) = (&previous.counter, &merged.counter) {
        let counter = CounterChange {
            from: before.from,
            ..after.clone()
        };
        merged.text = counter_text(&counter);
        merged.counter = Some(counter);
    } else if let (Some(before), Some(after)) = (&previous.roll, &merged.roll) {
        let results: Vec<RollResult> = before
            .results
            .iter()
            .chain(&after.results)
            .cloned()
            .collect();
        let joined = results
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        merged.text = format!("{}{joined}", after.prefix);
        merged.roll = Some(RollLine {
            prefix: after.prefix.clone(),
            results,
        });
    }
    merged
}

/// A rematch reset the table.
pub fn rematch() -> LogEntry {
    text("Rematch: back to setup with the same seats")
}

/// A player joined.
pub fn joined(name: &str) -> LogEntry {
    text(format!("{name} joined the table"))
}

/// A player left for good.
pub fn left(name: &str) -> LogEntry {
    text(format!("{name} left the table"))
}

/// The monarch changed hands; `actor` gave it (or took it themselves).
pub fn monarch(holder: &Holder, actor: &Holder) -> LogEntry {
    if holder.peer_id == actor.peer_id {
        text(format!("{} took the monarch", holder.player_name))
    } else {
        text(format!(
            "{} gave {} the monarch",
            actor.player_name, holder.player_name
        ))
    }
}

/// The line for a new seat order; `started` is whether the clock was already running.
pub fn seat_order(shuffled: bool, started: bool) -> LogEntry {
    match (shuffled, started) {
        (true, _) => text("Seat order randomized"),
        (false, false) => text("Game started in seat order"),
        (false, true) => text("Seat order changed"),
    }
}

/// A seat was eliminated or restored.
pub fn elimination(seat: &Seat, eliminated: bool) -> LogEntry {
    if eliminated {
        by(
            &seat.peer_id,
            "eliminated",
            format!("{} was eliminated", seat.player_name),
        )
    } else {
        by(
            &seat.peer_id,
            "restored",
            format!("{} was restored to the game", seat.player_name),
        )
    }
}

/// What was rolled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollKind {
    /// A die with this many sides.
    Dice(i64),
    /// A coin.
    Coin,
}

/// A die or coin roll by `player_name` (peer `actor`).
pub fn roll(kind: RollKind, result: &RollResult, actor: &str, player_name: &str) -> LogEntry {
    let (kind, prefix) = match kind {
        RollKind::Dice(sides) => (
            format!("dice:{sides}"),
            format!("{player_name} rolled a d{sides}: "),
        ),
        RollKind::Coin => ("coin".to_owned(), format!("{player_name} flipped a coin: ")),
    };
    LogEntry {
        roll: Some(RollLine {
            prefix: prefix.clone(),
            results: vec![result.clone()],
        }),
        ..by(actor, kind, format!("{prefix}{result}"))
    }
}

/// Lines for what a seat changed about itself: deck, life, camera and counters. Elimination
/// is logged where the room decides it. `seats` names commander damage sources.
pub fn seat_changes(previous: &Seat, next: &Seat, seats: &[Seat]) -> Vec<LogEntry> {
    let name = &next.player_name;
    let mut entries = Vec::new();
    if next.deck_id != previous.deck_id
        && let Some(deck_name) = &next.deck_name
    {
        entries.push(by(
            &next.peer_id,
            "deck",
            format!("{name} chose {deck_name}"),
        ));
    }
    if next.life != previous.life {
        let life = LifeChange {
            name: name.clone(),
            from: previous.life,
            to: next.life,
        };
        entries.push(LogEntry {
            life: Some(life.clone()),
            ..by(&next.peer_id, "life", life_text(&life))
        });
    }
    if next.camera_off != previous.camera_off {
        let state = if next.camera_off { "off" } else { "on" };
        entries.push(by(
            &next.peer_id,
            "camera",
            format!("{name} turned their camera {state}"),
        ));
    }
    entries.extend(counter_changes(previous, next, seats));
    entries
}

fn life_text(life: &LifeChange) -> String {
    format!("{}: {} → {} life", life.name, life.from, life.to)
}

fn counter_text(counter: &CounterChange) -> String {
    format!("{}{} → {}", counter.prefix, counter.from, counter.to)
}

/// The keys of `a`, then the keys only `b` has.
fn keys<'a, V>(a: &'a BTreeMap<String, V>, b: &'a BTreeMap<String, V>) -> Vec<&'a String> {
    let mut keys: Vec<&String> = a.keys().collect();
    keys.extend(b.keys().filter(|key| !a.contains_key(*key)));
    keys
}

fn counter_changes(previous: &Seat, next: &Seat, seats: &[Seat]) -> Vec<LogEntry> {
    let mut changes = vec![
        ("poison".to_owned(), previous.poison, next.poison),
        ("rad".to_owned(), previous.rad, next.rad),
    ];
    for commander in keys(&previous.commander_casts, &next.commander_casts) {
        changes.push((
            format!("{commander} commander tax"),
            previous
                .commander_casts
                .get(commander)
                .copied()
                .unwrap_or(0)
                * 2,
            next.commander_casts.get(commander).copied().unwrap_or(0) * 2,
        ));
    }
    let empty = BTreeMap::new();
    for id in keys(&previous.commander_damage, &next.commander_damage) {
        let before = previous.commander_damage.get(id).unwrap_or(&empty);
        let after = next.commander_damage.get(id).unwrap_or(&empty);
        for commander in keys(before, after) {
            changes.push((
                format!("damage from {}'s {commander}", source_name(seats, id)),
                before.get(commander).copied().unwrap_or(0),
                after.get(commander).copied().unwrap_or(0),
            ));
        }
    }
    // Shared custom counters are matched by id, so renaming one does not log a change and a
    // counter that stops being shared (or is removed) leaves nothing behind.
    let before: BTreeMap<&str, i64> = previous
        .custom_counters
        .iter()
        .map(|counter| (counter.id.as_str(), counter.value))
        .collect();
    for counter in &next.custom_counters {
        changes.push((
            counter.label.clone(),
            before.get(counter.id.as_str()).copied().unwrap_or(0),
            counter.value,
        ));
    }
    changes
        .into_iter()
        .filter(|(_, from, to)| from != to)
        .map(|(label, from, to)| {
            let counter = CounterChange {
                prefix: format!("{} {label}: ", next.player_name),
                from,
                to,
            };
            LogEntry {
                counter: Some(counter.clone()),
                ..by(
                    &next.peer_id,
                    format!("counter:{label}"),
                    counter_text(&counter),
                )
            }
        })
        .collect()
}

fn source_name(seats: &[Seat], id: &str) -> String {
    seats
        .iter()
        .find(|seat| seat.player_id.to_string() == id)
        .map_or_else(|| format!("player {id}"), |seat| seat.player_name.clone())
}
