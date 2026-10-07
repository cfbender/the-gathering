//! The seat-per-row CSV template and Mythic Track's spreadsheet export.

use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};
use time::macros::{format_description, time};
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime};

use crate::db::UtcDateTime;
use crate::games::{WinCondition, fold_name};

use super::etf::{self, Term};
use super::table;
use super::{ImportGame, ImportSeat, LineError, blank_to_nil, parse_integer, uuid_castable};

const NATIVE_REQUIRED: [&str; 7] = [
    "gameid",
    "date",
    "player",
    "deck",
    "commander",
    "seat",
    "result",
];
const MYTHIC_REQUIRED: [&str; 6] = [
    "date",
    "player1",
    "player2",
    "player1commander",
    "player2commander",
    "winner",
];
const SOURCES: [&str; 4] = ["manual", "csv", "mythic_track", "discord"];

/// `DateTime.from_iso8601/1`: a timestamp with a `Z` or numeric offset (`T` or space
/// separated), converted to UTC.
pub(crate) fn parse_offset_datetime(value: &str) -> Option<UtcDateTime> {
    let normalized = if value.get(10..11) == Some(" ") {
        format!(
            "{}T{}",
            value.get(..10).unwrap_or_default(),
            value.get(11..).unwrap_or_default()
        )
    } else {
        value.to_owned()
    };
    OffsetDateTime::parse(&normalized, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(UtcDateTime::from_offset)
}

/// `NaiveDateTime.from_iso8601/1`: a timestamp without a zone (any zone suffix ignored).
pub(crate) fn parse_naive_datetime(value: &str) -> Option<PrimitiveDateTime> {
    let normalized = value.replacen(' ', "T", 1);
    let naive = normalized
        .strip_suffix('Z')
        .unwrap_or(&normalized)
        .to_owned();
    let formats = [
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]"),
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]"),
    ];
    formats
        .iter()
        .find_map(|format| PrimitiveDateTime::parse(&naive, format).ok())
}

/// `Date.from_iso8601/1`.
pub(crate) fn parse_iso_date(value: &str) -> Option<Date> {
    Date::parse(value, format_description!("[year]-[month]-[day]")).ok()
}

/// `Date.new/3`.
pub(crate) fn new_date(year: i64, month: i64, day: i64) -> Option<Date> {
    let month = Month::try_from(u8::try_from(month).ok()?).ok()?;
    Date::from_calendar_date(i32::try_from(year).ok()?, month, u8::try_from(day).ok()?).ok()
}

/// Noon UTC on `date`.
pub(crate) fn noon(date: Date) -> UtcDateTime {
    UtcDateTime::from_offset(PrimitiveDateTime::new(date, time!(12:00)).assume_utc())
}

/// `M/D/Y` with exact integers.
pub(crate) fn slash_date(value: &str, two_digit_years: bool) -> Option<Date> {
    let mut parts = value.split('/');
    let (Some(month), Some(day), Some(year), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let year = parse_integer(year)?;
    let year = if two_digit_years && year < 100 {
        2000 + year
    } else {
        year
    };
    new_date(year, parse_integer(month)?, parse_integer(day)?)
}

fn parse_date(value: &str) -> Option<UtcDateTime> {
    if value.is_empty() {
        return None;
    }
    parse_offset_datetime(value)
        .or_else(|| parse_iso_date(value).map(noon))
        .or_else(|| slash_date(value, false).map(noon))
}

/// An integer column: blank, unparseable, or a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Int {
    Blank,
    Invalid,
    Value(i64),
}

impl Int {
    fn required(value: &str) -> Self {
        parse_integer(value).map_or(Self::Invalid, Self::Value)
    }

    fn optional(value: &str) -> Self {
        if value.is_empty() {
            Self::Blank
        } else {
            Self::required(value)
        }
    }

    fn value(self) -> Option<i64> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }
}

/// One row before validation.
struct RawRow {
    game_id: String,
    date: Option<UtcDateTime>,
    player: String,
    deck: String,
    commander: String,
    seat: Int,
    result: String,
    kills: Int,
    partner_name: Option<String>,
    mvp_card: Option<String>,
    duration_minutes: Int,
    turns: Int,
    notes: Option<String>,
    win_condition: Option<String>,
    action: String,
    target_source: Option<String>,
    target_external_id: Option<String>,
    target_portable_id: Option<String>,
    line: i64,
}

/// A valid row.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    game_id: String,
    date: UtcDateTime,
    player: String,
    deck: String,
    commander: String,
    seat: i64,
    result: String,
    kills: Option<i64>,
    partner_name: Option<String>,
    mvp_card: Option<String>,
    duration_minutes: Option<i64>,
    turns: Option<i64>,
    notes: Option<String>,
    win_condition: Option<String>,
    action: String,
    target_source: Option<String>,
    target_external_id: Option<String>,
    target_portable_id: Option<String>,
    line: i64,
}

struct Values(HashMap<String, String>);

impl Values {
    fn new(headers: &[String], row: &[String]) -> Self {
        Self(
            headers
                .iter()
                .zip(row)
                .map(|(header, value)| (header.clone(), value.clone()))
                .collect(),
        )
    }

    fn get(&self, key: &str) -> String {
        self.0
            .get(key)
            .map(|value| value.trim().to_owned())
            .unwrap_or_default()
    }
}

/// Parses a CSV payload: `Ok((games, errors))` once a header is recognized, otherwise the
/// file-level errors.
pub fn parse(csv: &str) -> Result<(Vec<ImportGame>, Vec<LineError>), Vec<LineError>> {
    let rows =
        table::parse(csv, b',').map_err(|message| vec![LineError::new(1, "csv", message)])?;
    let mut rows = rows.into_iter();
    let Some(header) = rows.next() else {
        return Err(vec![LineError::new(1, "csv", "must include a header row")]);
    };
    let headers: Vec<String> = header.fields.iter().map(|h| normalize_header(h)).collect();
    let data: Vec<table::Row> = rows
        .filter(|row| !row.fields.iter().all(|field| field.trim().is_empty()))
        .collect();
    let has = |required: &[&str]| required.iter().all(|key| headers.iter().any(|h| h == key));
    let raw: Vec<RawRow> = if has(&NATIVE_REQUIRED) {
        data.iter()
            .map(|row| native_row(&headers, &row.fields, row.line))
            .collect()
    } else if has(&MYTHIC_REQUIRED) {
        data.iter()
            .flat_map(|row| mythic_rows(&headers, &row.fields, row.line))
            .collect()
    } else {
        return Err(vec![LineError::new(1, "headers", header_error(&headers))]);
    };
    let mut errors = Vec::new();
    let mut parsed = Vec::new();
    for row in raw {
        let row_errors = validate_row(&row);
        if row_errors.is_empty() {
            parsed.push(valid(row));
        } else {
            errors.extend(row_errors);
        }
    }
    let groups = group_games(parsed);
    let games = build_games(&groups);
    if games.is_empty() && errors.is_empty() {
        errors.push(LineError::new(
            1,
            "csv",
            "must include at least one data row",
        ));
    }
    for group in &groups {
        errors.extend(validate_game(group));
    }
    Ok((games, errors))
}

fn normalize_header(header: &str) -> String {
    header
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        .collect()
}

fn native_row(headers: &[String], row: &[String], line: i64) -> RawRow {
    let values = Values::new(headers, row);
    let action = values.get("action");
    RawRow {
        game_id: values.get("gameid"),
        date: parse_date(&values.get("date")),
        player: values.get("player"),
        deck: values.get("deck"),
        commander: values.get("commander"),
        seat: Int::required(&values.get("seat")),
        result: values.get("result").to_lowercase(),
        kills: Int::optional(&values.get("kills")),
        partner_name: blank_to_nil(values.get("partner")),
        mvp_card: blank_to_nil(values.get("mvpcard")),
        duration_minutes: Int::optional(&values.get("durationminutes")),
        turns: Int::optional(&values.get("turns")),
        notes: blank_to_nil(values.get("notes")),
        win_condition: blank_to_nil(values.get("wincondition")),
        action: if action.is_empty() {
            "create".to_owned()
        } else {
            action
        },
        target_source: blank_to_nil(values.get("source")),
        target_external_id: blank_to_nil(values.get("externalid")),
        target_portable_id: blank_to_nil(values.get("portableid")),
        line,
    }
}

fn mythic_rows(headers: &[String], row: &[String], line: i64) -> Vec<RawRow> {
    let values = Values::new(headers, row);
    let players: Vec<i64> = (1..=4)
        .filter(|index| !values.get(&format!("player{index}")).is_empty())
        .collect();
    let winner = values.get("winner");
    let game_id = format!(
        "mythic-{line}-{}-{}",
        values.get("date"),
        players
            .iter()
            .map(|index| values.get(&format!("player{index}")))
            .collect::<Vec<_>>()
            .join("-")
    );
    players
        .iter()
        .map(|index| {
            let player = values.get(&format!("player{index}"));
            let commander = values.get(&format!("player{index}commander"));
            let result = if winner.is_empty() {
                "draw"
            } else if winner.to_lowercase() == player.to_lowercase() {
                "win"
            } else {
                "loss"
            };
            RawRow {
                game_id: game_id.clone(),
                date: parse_date(&values.get("date")),
                player,
                deck: commander.clone(),
                commander,
                seat: Int::Value(*index),
                result: result.to_owned(),
                kills: Int::Blank,
                partner_name: None,
                mvp_card: None,
                duration_minutes: Int::optional(&values.get("gametimeminutes")),
                turns: Int::optional(&values.get("totalturns")),
                notes: blank_to_nil(values.get("notes")),
                win_condition: None,
                action: "create".to_owned(),
                target_source: None,
                target_external_id: None,
                target_portable_id: None,
                line,
            }
        })
        .collect()
}

fn validate_row(row: &RawRow) -> Vec<LineError> {
    let line = row.line;
    let mut errors = Vec::new();
    let required = |field: &str, present: bool, errors: &mut Vec<LineError>| {
        if !present {
            errors.push(LineError::new(line, field, "is required and must be valid"));
        }
    };
    required("game_id", !row.game_id.is_empty(), &mut errors);
    required("date", row.date.is_some(), &mut errors);
    required("player", !row.player.is_empty(), &mut errors);
    required("deck", !row.deck.is_empty(), &mut errors);
    required("commander", !row.commander.is_empty(), &mut errors);
    for (field, value) in [("player", &row.player), ("deck", &row.deck)] {
        if value.len() > 100 {
            errors.push(LineError::new(
                line,
                field,
                "must be at most 100 characters",
            ));
        }
    }
    let positive = |field: &str, value: Int, errors: &mut Vec<LineError>| {
        if !matches!(value, Int::Value(number) if number > 0) {
            errors.push(LineError::new(line, field, "must be a positive integer"));
        }
    };
    positive("seat", row.seat, &mut errors);
    if !["win", "loss", "draw"].contains(&row.result.as_str()) {
        errors.push(LineError::new(line, "result", "must be win, loss, or draw"));
    }
    for (field, value) in [
        ("duration_minutes", row.duration_minutes),
        ("turns", row.turns),
    ] {
        if value != Int::Blank {
            positive(field, value, &mut errors);
        }
    }
    let checks = [
        (
            !["create", "update", "skip"].contains(&row.action.as_str()),
            "action",
            "must be create, update, or skip",
        ),
        (
            row.kills != Int::Blank && !matches!(row.kills, Int::Value(0..=5)),
            "kills",
            "must be 0–5 or blank",
        ),
        (
            row.win_condition
                .as_deref()
                .is_some_and(|key| WinCondition::parse(key).is_none()),
            "win_condition",
            "must be a supported win condition key",
        ),
        (
            row.target_source
                .as_deref()
                .is_some_and(|source| !SOURCES.contains(&source)),
            "source",
            "is not supported",
        ),
        (
            row.target_source.is_none() != row.target_external_id.is_none(),
            "external_id",
            "requires both source and external_id",
        ),
        (
            row.target_portable_id
                .as_deref()
                .is_some_and(|id| !uuid_castable(id)),
            "portable_id",
            "must be a UUID",
        ),
        (
            row.action == "update"
                && row.target_portable_id.is_none()
                && row.target_external_id.is_none(),
            "action",
            "updates require portable_id or source and external_id",
        ),
    ];
    for (invalid, field, message) in checks {
        if invalid {
            errors.push(LineError::new(line, field, message));
        }
    }
    errors
}

fn valid(row: RawRow) -> Row {
    Row {
        game_id: row.game_id,
        date: row.date.unwrap_or_default(),
        player: row.player,
        deck: row.deck,
        commander: row.commander,
        seat: row.seat.value().unwrap_or_default(),
        result: row.result,
        kills: row.kills.value(),
        partner_name: row.partner_name,
        mvp_card: row.mvp_card,
        duration_minutes: row.duration_minutes.value(),
        turns: row.turns.value(),
        notes: row.notes,
        win_condition: row.win_condition,
        action: row.action,
        target_source: row.target_source,
        target_external_id: row.target_external_id,
        target_portable_id: row.target_portable_id,
        line: row.line,
    }
}

/// Rows grouped by `game_id`, in `game_id` order (Elixir iterated a map).
fn group_games(rows: Vec<Row>) -> Vec<Vec<Row>> {
    let mut groups: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for row in rows {
        groups.entry(row.game_id.clone()).or_default().push(row);
    }
    groups.into_values().collect()
}

/// Games sorted by `{played_at, external_id}`.
///
/// Elixir sorted by the `DateTime` structs' term order, which compares the day of the
/// month before the month and year; this sorts chronologically.
fn build_games(groups: &[Vec<Row>]) -> Vec<ImportGame> {
    let mut games: Vec<ImportGame> = groups
        .iter()
        .filter_map(|seats| build_game(seats))
        .collect();
    games.sort_by(|a, b| (a.played_at, &a.external_id).cmp(&(b.played_at, &b.external_id)));
    games
}

fn sorted_by_seat(seats: &[Row]) -> Vec<&Row> {
    let mut sorted: Vec<&Row> = seats.iter().collect();
    sorted.sort_by_key(|seat| seat.seat);
    sorted
}

fn lines(seats: &[Row]) -> Vec<i64> {
    let mut lines: Vec<i64> = seats.iter().map(|seat| seat.line).collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The game's identity: SHA-256 of its normalized rows, plus the Erlang term of the
/// kills/partner/win-condition extensions when any is present (as the Elixir importer
/// computed it, so re-imports of files imported there are still recognized).
fn external_id(first: &Row, seats: &[&Row]) -> String {
    let opt = |value: Option<i64>| value.map(|value| value.to_string()).unwrap_or_default();
    let normalized = seats
        .iter()
        .map(|seat| {
            [
                first.game_id.clone(),
                first.date.to_db_string(),
                seat.player.clone(),
                seat.deck.clone(),
                seat.commander.clone(),
                seat.seat.to_string(),
                seat.result.clone(),
                seat.mvp_card.clone().unwrap_or_default(),
                opt(first.duration_minutes),
                opt(first.turns),
                first.notes.clone().unwrap_or_default(),
            ]
            .join("|")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut bytes = normalized.into_bytes();
    let extras_present = seats.iter().any(|seat| {
        seat.kills.is_some() || seat.partner_name.is_some() || seat.win_condition.is_some()
    });
    if extras_present {
        let extras = Term::List(
            seats
                .iter()
                .map(|seat| {
                    Term::Tuple(vec![
                        Term::int_or_nil(seat.kills),
                        Term::binary_or_nil(seat.partner_name.as_deref()),
                        Term::binary_or_nil(seat.win_condition.as_deref()),
                    ])
                })
                .collect(),
        );
        bytes.extend(etf::encode(&extras));
    }
    hex(&Sha256::digest(&bytes))
}

/// Lowercase hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn build_game(seats: &[Row]) -> Option<ImportGame> {
    let first = seats.first()?;
    let sorted = sorted_by_seat(seats);
    Some(ImportGame {
        external_id: external_id(first, &sorted),
        game_id: first.game_id.clone(),
        played_at: first.date,
        duration_minutes: first.duration_minutes,
        turns: first.turns,
        win_condition: first.win_condition.clone(),
        notes: first.notes.clone(),
        lines: lines(seats),
        seats: sorted
            .into_iter()
            .map(|seat| ImportSeat {
                line: seat.line,
                player: seat.player.clone(),
                deck: seat.deck.clone(),
                commander: seat.commander.clone(),
                partner_name: seat.partner_name.clone(),
                seat: seat.seat,
                result: seat.result.clone(),
                kills: seat.kills,
                mvp_card: seat.mvp_card.clone(),
                ..ImportSeat::default()
            })
            .collect(),
        action: Some(first.action.clone()),
        target_source: first.target_source.clone(),
        target_external_id: first.target_external_id.clone(),
        target_portable_id: first.target_portable_id.clone(),
    })
}

fn consistent<T: PartialEq>(seats: &[Row], field: impl Fn(&Row) -> T) -> bool {
    let mut values = seats.iter().map(field);
    match values.next() {
        Some(first) => values.all(|value| value == first),
        None => false,
    }
}

fn validate_game(seats: &[Row]) -> Vec<LineError> {
    let lines = lines(seats);
    let mut errors = Vec::new();
    let mut add = |invalid: bool, field: &str, message: &str| {
        if invalid {
            errors.extend(
                lines
                    .iter()
                    .map(|line| LineError::new(*line, field, message)),
            );
        }
    };
    let must_match = "must match for every row in the game";
    add(
        !consistent(seats, |s| s.action.clone()),
        "action",
        must_match,
    );
    add(
        !consistent(seats, |s| s.target_source.clone()),
        "target_source",
        must_match,
    );
    add(
        !consistent(seats, |s| s.target_external_id.clone()),
        "target_external_id",
        must_match,
    );
    add(
        !consistent(seats, |s| s.target_portable_id.clone()),
        "target_portable_id",
        must_match,
    );
    add(
        !consistent(seats, |s| s.win_condition.clone()),
        "win_condition",
        must_match,
    );
    add(
        !(2..=6).contains(&seats.len()),
        "game_id",
        "must contain between 2 and 6 players",
    );
    let names: Vec<String> = seats.iter().map(|seat| fold_name(&seat.player)).collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    add(
        unique.len() != names.len(),
        "player",
        "cannot contain the same player twice",
    );
    let mut numbers: Vec<i64> = seats.iter().map(|seat| seat.seat).collect();
    numbers.sort_unstable();
    add(
        !numbers
            .iter()
            .copied()
            .eq(1..=i64::try_from(seats.len()).unwrap_or(0)),
        "seat",
        "must use consecutive seat numbers starting at 1",
    );
    let winners = seats.iter().filter(|seat| seat.result == "win").count();
    let valid_results = (winners == 1
        && seats
            .iter()
            .all(|seat| seat.result == "win" || seat.result == "loss"))
        || seats.iter().all(|seat| seat.result == "draw");
    add(
        !valid_results,
        "result",
        "must have exactly one winner and all other players lose, or all players draw",
    );
    add(!consistent(seats, |s| s.date), "date", must_match);
    add(
        !consistent(seats, |s| s.duration_minutes),
        "duration_minutes",
        must_match,
    );
    add(!consistent(seats, |s| s.turns), "turns", must_match);
    add(!consistent(seats, |s| s.notes.clone()), "notes", must_match);
    errors
}

fn header_error(headers: &[String]) -> String {
    let missing: Vec<&str> = NATIVE_REQUIRED
        .iter()
        .filter(|key| !headers.iter().any(|header| header == *key))
        .map(|key| if *key == "gameid" { "game_id" } else { key })
        .collect();
    format!(
        "unrecognized CSV format; native format is missing: {}",
        missing.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_identities() {
        assert_eq!(
            parse_date("2026-09-18").unwrap().to_string(),
            "2026-09-18T12:00:00Z"
        );
        assert_eq!(
            parse_date("1/2/2024").unwrap().to_string(),
            "2024-01-02T12:00:00Z"
        );
        assert_eq!(
            parse_date("2026-09-18 10:00:00+02:00").unwrap().to_string(),
            "2026-09-18T08:00:00Z"
        );
        assert!(parse_date("2026-09-18T10:00:00").is_none());
        assert!(parse_date("2/30/2024").is_none());
        assert_eq!(normalize_header(" Game ID "), "gameid");
        assert_eq!(normalize_header("\u{feff}game_id"), "gameid");
    }

    /// The identity the Elixir importer computed for the same rows.
    #[test]
    fn external_ids_match_elixir() {
        let csv = "game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes\n\
                   friday-1,2026-09-18,Alice,Birds,\"Kangee, Sky Warden\",1,win,Swan Song,75,10,Close game\n\
                   friday-1,2026-09-18,Bob,Goblins,Krenko,2,loss,,75,10,Close game\n";
        let (games, errors) = parse(csv).unwrap();
        assert!(errors.is_empty());
        let expected = hex(&Sha256::digest(
            "friday-1|2026-09-18T12:00:00Z|Alice|Birds|Kangee, Sky Warden|1|win|Swan Song|75|10|Close game\n\
             friday-1|2026-09-18T12:00:00Z|Bob|Goblins|Krenko|2|loss||75|10|Close game",
        ));
        assert_eq!(games[0].external_id, expected);
    }
}
