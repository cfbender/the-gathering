//! Parses the JSON returned by Mythic Track's `POST api/games/get` endpoint
//! (`TheGathering.Imports.MythicTrack`).
//!
//! Mythic Track has no export feature, but its Blazor client fetches the signed-in user's
//! full game list from that endpoint as a `List<GameViewModel>`. Users save that response
//! (the SPA shows a console snippet) and upload it; the server only parses the payload.
//! Only completed games (`gameStatus == 3`) are imported; other statuses are warnings.
//!
//! Produces the same game/seat shape as [`super::csv`], with `line` set to the game's
//! 1-based position in the array. Seats additionally carry Discord IDs, Scryfall card
//! IDs, colour identity, and decklist URLs when Mythic Track supplied them. A game's first
//! key card becomes the winner's MVP card.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use time::macros::time;

use crate::db::UtcDateTime;
use crate::games::{WinCondition, fold_name};

use super::csv::{noon, parse_iso_date, parse_naive_datetime, parse_offset_datetime};
use super::{ImportGame, ImportSeat, LineError, Warning, blank_to_nil, inspect};

const STATUS_COMPLETE: i64 = 3;

/// The parsed games, per-game errors, and skipped-game warnings; or file-level errors.
pub type Parsed = (Vec<ImportGame>, Vec<LineError>, Vec<Warning>);

static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);

/// A field of an object (`map["key"]`; non-objects have no fields).
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

/// An object, or an empty one for anything else (`value || %{}`).
fn object(value: &Value) -> &Map<String, Value> {
    value.as_object().unwrap_or(&EMPTY)
}

/// `List.wrap/1` (non-object items are treated as empty objects where fields are read).
fn wrap(value: &Value) -> Vec<&Value> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    }
}

/// `string/1`: trimmed text; numbers and booleans as text; `nil` (and, where Elixir would
/// have raised, lists and objects) as `""`.
fn string(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => String::new(),
    }
}

fn get_str(map: &Map<String, Value>, key: &str) -> String {
    string(map.get(key).unwrap_or(&Value::Null))
}

/// Parses the uploaded JSON.
pub fn parse(json: &str) -> Result<Parsed, Vec<LineError>> {
    let decoded: Value = serde_json::from_str(json)
        .map_err(|error| vec![LineError::new(1, "json", error.to_string())])?;
    let games = match &decoded {
        Value::Array(games) => games,
        Value::Object(map) => match map.get("data") {
            Some(Value::Array(games)) => games,
            _ => return Err(not_an_array()),
        },
        _ => return Err(not_an_array()),
    };
    if games.is_empty() {
        return Err(vec![LineError::new(
            1,
            "json",
            "must include at least one game",
        )]);
    }
    let mut parsed = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for (game, line) in games.iter().zip(1_i64..) {
        match classify(game, line) {
            Classified::Skip(warning) => warnings.push(warning),
            Classified::Game(game) => parsed.push(*game),
            Classified::Error(error) => errors.push(error),
        }
    }
    // Elixir sorted by the `DateTime` structs' term order (day of month first); this sorts
    // chronologically.
    parsed.sort_by(|a, b| (a.played_at, &a.external_id).cmp(&(b.played_at, &b.external_id)));
    Ok((parsed, errors, warnings))
}

fn not_an_array() -> Vec<LineError> {
    vec![LineError::new(
        1,
        "json",
        "must be a JSON array of Mythic Track games",
    )]
}

enum Classified {
    Skip(Warning),
    Game(Box<ImportGame>),
    Error(LineError),
}

fn classify(game: &Value, line: i64) -> Classified {
    if !game.is_object() {
        return Classified::Error(LineError::new(line, "game", "must be an object"));
    }
    let status = field(game, "gameStatus");
    if status.as_i64() == Some(STATUS_COMPLETE) && status.is_i64() {
        return build_game(game, line);
    }
    let name = match status.as_i64() {
        Some(1) if status.is_i64() => "not started".to_owned(),
        Some(2) if status.is_i64() => "in progress".to_owned(),
        _ => format!("status {}", inspect::value(status)),
    };
    Classified::Skip(Warning {
        line,
        message: format!("skipped: game is {name} ({})", describe(game)),
    })
}

/// Enough context to find the game in Mythic Track and fix it there.
fn describe(game: &Value) -> String {
    let players: Vec<String> = wrap(field(game, "players"))
        .into_iter()
        .map(|player| player_name(object(field(player, "player"))))
        .filter(|name| !name.is_empty())
        .collect();
    let created: String = string(field(game, "createdOn")).chars().take(10).collect();
    [
        blank_to_nil(string(field(game, "name"))),
        blank_to_nil(created),
        (!players.is_empty()).then(|| format!("players: {}", players.join(", "))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

struct KeyCard {
    name: String,
    card_id: Option<String>,
}

fn build_game(game: &Value, line: i64) -> Classified {
    let external_id = string(field(game, "id"));
    let played_at = parse_datetime(field(game, "createdOn"));
    let players = wrap(field(game, "players"));
    let key_cards = key_cards(game);

    let mut ordered: Vec<(usize, &Value)> = players.iter().copied().enumerate().collect();
    ordered.sort_by(|(a_index, a), (b_index, b)| {
        let order = |player: &Value| field(player, "turnOrder").as_f64().unwrap_or(1000.0);
        order(a).total_cmp(&order(b)).then(a_index.cmp(b_index))
    });
    let mut seats: Vec<ImportSeat> = ordered
        .into_iter()
        .zip(1_i64..)
        .map(|((_, player), seat)| build_seat(player, seat, line, &players))
        .collect();
    if let Some(mvp) = key_cards.first() {
        for seat in seats.iter_mut().filter(|seat| seat.result == "win") {
            seat.mvp_card = Some(mvp.name.clone());
            seat.mvp_card_id.clone_from(&mvp.card_id);
        }
    }

    // A game the admin cannot repair in this file is skipped with a reason rather than
    // blocking the other games; malformed identity fields are real errors.
    if external_id.is_empty() {
        return Classified::Error(LineError::new(line, "id", "is required"));
    }
    let Some(played_at) = played_at else {
        return Classified::Error(LineError::new(
            line,
            "createdOn",
            "is required and must be a valid date",
        ));
    };
    if let Some(reason) = skip_reason(&seats) {
        return Classified::Skip(Warning {
            line,
            message: format!("skipped: {reason} ({})", describe(game)),
        });
    }
    let win_condition = field(game, "winCondition")
        .as_i64()
        .filter(|_| field(game, "winCondition").is_i64())
        .map_or(WinCondition::Unknown, WinCondition::from_mythic);
    Classified::Game(Box::new(ImportGame {
        game_id: external_id.clone(),
        external_id,
        played_at,
        duration_minutes: positive_or_nil(field(game, "gameTimeInMinutes")),
        turns: positive_or_nil(field(game, "totalTurns")),
        win_condition: Some(win_condition.as_str().to_owned()),
        notes: notes(game, &key_cards, &seats),
        lines: vec![line],
        seats,
        action: None,
        target_source: None,
        target_external_id: None,
        target_portable_id: None,
    }))
}

fn skip_reason(seats: &[ImportSeat]) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for seat in seats {
        *counts.entry(fold_name(&seat.player)).or_default() += 1;
    }
    let duplicates: Vec<String> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .filter_map(|(name, _)| {
            seats
                .iter()
                .find(|seat| fold_name(&seat.player) == name)
                .map(|seat| seat.player.clone())
        })
        .collect();
    if !(2..=6).contains(&seats.len()) {
        Some(format!(
            "needs between 2 and 6 players, has {}",
            seats.len()
        ))
    } else if seats.iter().any(|seat| seat.player.is_empty()) {
        Some("a player has no name".to_owned())
    } else if !duplicates.is_empty() {
        Some(format!("{} is listed twice", duplicates.join(", ")))
    } else if seats.iter().filter(|seat| seat.result == "win").count() > 1 {
        Some("more than one player is marked as the winner".to_owned())
    } else {
        None
    }
}

fn build_seat(player: &Value, seat: i64, line: i64, players: &[&Value]) -> ImportSeat {
    let commander = object(field(player, "commander"));
    let partner = object(field(player, "commanderPartner"));
    // Mythic Track sometimes writes partners as "A || B (Partners)" in the commander name
    // instead of filling commanderPartner.
    let (commander_name, piped_partner) = split_partners(&get_str(commander, "name"));
    let partner_name = blank_to_nil(get_str(partner, "name")).or(piped_partner);
    let identity = object(field(player, "player"));
    ImportSeat {
        line,
        player: player_name(identity),
        discord_id: blank_to_nil(get_str(identity, "discordUserId")),
        deck: deck_name(commander, &commander_name, partner_name.as_deref()),
        commander: commander_name,
        commander_card_id: blank_to_nil(get_str(commander, "scryfallId")),
        partner_name,
        partner_card_id: blank_to_nil(get_str(partner, "scryfallId")),
        color_identity: Some(color_identity(commander, partner)),
        decklist_url: blank_to_nil(get_str(commander, "decklistUrl")),
        seat,
        result: result(player, players).to_owned(),
        kills: None,
        mvp_card: None,
        mvp_card_id: None,
    }
}

/// Mythic Track records a game's key cards without tying them to a seat; in practice they
/// are the cards that won the game, so the first one becomes the winner's MVP. Remaining
/// key cards are kept in the game notes.
fn key_cards(game: &Value) -> Vec<KeyCard> {
    wrap(field(game, "keyCards"))
        .into_iter()
        .filter_map(Value::as_object)
        .map(|card| KeyCard {
            name: get_str(card, "name"),
            card_id: blank_to_nil(get_str(card, "scryfallId")),
        })
        .filter(|card| !card.name.is_empty())
        .collect()
}

fn player_name(identity: &Map<String, Value>) -> String {
    ["name", "friendlyName", "username"]
        .iter()
        .map(|key| get_str(identity, key))
        .find(|name| !name.is_empty())
        .unwrap_or_default()
}

fn deck_name(
    commander: &Map<String, Value>,
    commander_name: &str,
    partner_name: Option<&str>,
) -> String {
    let name = get_str(commander, "deckName");
    match (name.is_empty(), partner_name) {
        (true, Some(partner)) => format!("{commander_name} / {partner}"),
        (true, None) => commander_name.to_owned(),
        (false, _) => name,
    }
}

fn split_partners(name: &str) -> (String, Option<String>) {
    match name.split_once("||") {
        Some((commander, partner)) => {
            let suffix = crate::regex::compile(r"\s*\([^)]*\)\s*$");
            (
                commander.trim().to_owned(),
                Some(suffix.replace(partner, "").trim().to_owned()),
            )
        }
        None => (name.trim().to_owned(), None),
    }
}

fn color_identity(commander: &Map<String, Value>, partner: &Map<String, Value>) -> String {
    let colors: Vec<String> = wrap(commander.get("colors").unwrap_or(&Value::Null))
        .into_iter()
        .chain(wrap(partner.get("colors").unwrap_or(&Value::Null)))
        .map(|color| string(color).to_uppercase())
        .collect();
    ["W", "U", "B", "R", "G"]
        .iter()
        .filter(|color| colors.iter().any(|c| c == *color))
        .copied()
        .collect()
}

fn is_winner(player: &Value) -> bool {
    field(player, "isWinner") == &Value::Bool(true)
}

fn result(player: &Value, players: &[&Value]) -> &'static str {
    if !players.iter().any(|player| is_winner(player)) {
        "draw"
    } else if is_winner(player) {
        "win"
    } else {
        "loss"
    }
}

fn notes(game: &Value, key_cards: &[KeyCard], seats: &[ImportSeat]) -> Option<String> {
    // Key cards that did not become the winner's MVP (or all of them when the game had no
    // winner) are listed in the notes so the data is not lost.
    let skip = usize::from(seats.iter().any(|seat| seat.result == "win"));
    let extra: Vec<&str> = key_cards
        .iter()
        .skip(skip)
        .map(|card| card.name.as_str())
        .collect();
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!("Key cards: {}", extra.join(", "))
    };
    let mut parts: Vec<String> = Vec::new();
    for part in [
        string(field(game, "name")),
        string(field(game, "notes")),
        extra,
    ] {
        if !part.is_empty() && !parts.contains(&part) {
            parts.push(part);
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// `createdOn`: an offset timestamp, a naive one (UTC; a midnight calendar date becomes
/// noon), or a bare date (noon UTC).
fn parse_datetime(value: &Value) -> Option<UtcDateTime> {
    let value = value.as_str()?;
    if let Some(parsed) = parse_offset_datetime(value) {
        return Some(parsed);
    }
    if let Some(naive) = parse_naive_datetime(value) {
        // Sub-seconds are truncated below either way.
        let naive = if naive.hour() == 0 && naive.minute() == 0 && naive.second() == 0 {
            naive.replace_time(time!(12:00))
        } else {
            naive
        };
        return Some(UtcDateTime::from_offset(naive.assume_utc()));
    }
    parse_iso_date(value).map(noon)
}

fn positive_or_nil(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .filter(|number| value.is_i64() && *number > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dates_partners_and_colors() {
        let at = |value: &str| parse_datetime(&json!(value)).map(|at| at.to_string());
        assert_eq!(
            at("2026-03-14T19:30:15.123456").as_deref(),
            Some("2026-03-14T19:30:15Z")
        );
        assert_eq!(
            at("2025-03-17T00:00:00").as_deref(),
            Some("2025-03-17T12:00:00Z")
        );
        assert_eq!(at("2025-03-17").as_deref(), Some("2025-03-17T12:00:00Z"));
        assert_eq!(
            at("2025-03-17T00:00:00-04:00").as_deref(),
            Some("2025-03-17T04:00:00Z")
        );
        assert_eq!(at("nope"), None);
        assert_eq!(
            split_partners("Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)"),
            (
                "Frodo, Adventurous Hobbit".to_owned(),
                Some("Sam, Loyal Attendant".to_owned())
            )
        );
    }
}
