//! Parses game rows exported (or pasted) from the group's Google Sheet
//! (`TheGathering.Imports.GoogleSheet`). The sheet is never fetched: admins paste its
//! cells (TSV) or upload a CSV download.

use std::collections::HashMap;

use serde::Serialize;
use sha2::{Digest, Sha256};
use time::Date;

use super::csv::{hex, parse_iso_date, slash_date};
use super::etf::{self, Term};
use super::{parse_integer, table};

const COLUMNS: [&str; 6] = ["date", "winner", "deck", "win con", "other decks", "notes"];
const DRAW_WARNING: &str =
    "No winner: all listed players will be recorded as a draw. Notes do not change results.";

/// A participant as the sheet lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SheetSeat {
    /// Name as written (an alias until mapped).
    pub player: String,
    /// Deck as written.
    pub deck: String,
    /// The kill column for this player (0 when none).
    pub kills: i64,
    /// `win`, `loss`, or `draw`.
    pub result: String,
}

/// A kill column's count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KillCount {
    /// The column heading.
    pub player: String,
    /// Kills (blank is 0).
    pub kills: i64,
}

/// One parsed game row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetRow {
    /// Stable identity of the raw row (with an occurrence suffix for exact duplicates).
    pub key: String,
    /// Line in the paste.
    pub line: i64,
    /// Parsed date.
    pub date: Option<Date>,
    /// Winner column.
    pub winner: String,
    /// Winner's deck.
    pub deck: String,
    /// Win Con column.
    pub win_con: String,
    /// Notes column.
    pub notes: String,
    /// Winner first, then Other Decks.
    pub seats: Vec<SheetSeat>,
    /// Every kill column.
    pub kill_counts: Vec<KillCount>,
    /// Row problems.
    pub errors: Vec<String>,
    /// Draws and duplicates.
    pub warnings: Vec<String>,
}

/// Header positions.
struct Indexes {
    date: usize,
    winner: usize,
    deck: usize,
    win_con: usize,
    other_decks: usize,
    notes: usize,
    kills: Vec<(usize, String)>,
}

/// Parses the paste: rows, or a message for the whole file.
pub fn parse(payload: &str) -> Result<Vec<SheetRow>, String> {
    // A spreadsheet download can start with a byte-order mark, which Elixir left in the
    // first heading (so "Date" was not found); it is dropped here.
    let payload = payload.strip_prefix('\u{feff}').unwrap_or(payload);
    let tsv = payload.contains('\t');
    let rows = parse_file(payload, tsv)?;
    let header_at = rows
        .iter()
        .position(|row| header_row(&row.fields))
        .ok_or_else(|| "Could not find the Google Sheet header row.".to_owned())?;
    let header = rows
        .get(header_at)
        .map(|row| row.fields.clone())
        .unwrap_or_default();
    let indexes = validate_header(&header)?;
    let parsed: Vec<SheetRow> = rows
        .iter()
        .skip(header_at + 1)
        .filter(|row| !row.fields.iter().all(|field| field.trim().is_empty()))
        .map(|row| build_row(&row.fields, row.line, &indexes))
        .collect();
    Ok(add_occurrences(parsed))
}

fn parse_file(payload: &str, tsv: bool) -> Result<Vec<table::Row>, String> {
    match table::parse(payload, if tsv { b'\t' } else { b',' }) {
        Ok(rows) => Ok(rows),
        // Plain pasted cells can contain literal quotes, unlike a quoted TSV export. Never
        // use this fallback for a file that began a quoted field: that could silently
        // split a malformed multiline cell into separate games.
        Err(_) if tsv && !crate::regex::compile(r#"(?:^|[\t\r\n])""#).is_match(payload) => {
            let newline = crate::regex::compile(r"\r?\n");
            Ok(newline
                .split(payload)
                .zip(1_i64..)
                .map(|(line, number)| table::Row {
                    line: number,
                    fields: line.split('\t').map(str::to_owned).collect(),
                })
                .collect())
        }
        Err(message) => Err(format!("Could not parse file: {message}")),
    }
}

fn normalize_header(header: &str) -> String {
    header
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn header_row(row: &[String]) -> bool {
    let normalized: Vec<String> = row.iter().map(|h| normalize_header(h)).collect();
    ["date", "winner", "deck"]
        .iter()
        .all(|name| normalized.iter().any(|h| h == name))
}

fn validate_header(header: &[String]) -> Result<Indexes, String> {
    let normalized: Vec<String> = header.iter().map(|h| normalize_header(h)).collect();
    let position = |name: &str| normalized.iter().position(|h| h == name);
    let missing: Vec<&str> = COLUMNS
        .iter()
        .copied()
        .filter(|name| position(name).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "Google Sheet header is missing: {}.",
            missing.join(", ")
        ));
    }
    let at = |name: &str| position(name).unwrap_or_default();
    let (deck, win_con) = (at("deck"), at("win con"));
    if deck >= win_con {
        return Err("Google Sheet kill columns must be between Deck and Win Con.".to_owned());
    }
    let kills = (deck + 1..win_con)
        .filter_map(|index| {
            let name = header.get(index).map(|h| h.trim()).unwrap_or_default();
            (!name.is_empty()).then(|| (index, name.to_owned()))
        })
        .collect();
    Ok(Indexes {
        date: at("date"),
        winner: at("winner"),
        deck,
        win_con,
        other_decks: at("other decks"),
        notes: at("notes"),
        kills,
    })
}

fn value(row: &[String], index: usize) -> String {
    row.get(index)
        .map(|v| v.trim().to_owned())
        .unwrap_or_default()
}

fn build_row(raw: &[String], line: i64, indexes: &Indexes) -> SheetRow {
    let winner = value(raw, indexes.winner);
    let winner_deck = value(raw, indexes.deck);
    let (opponents, mut errors) = parse_opponents(&value(raw, indexes.other_decks));
    let draw = winner.is_empty() || winner.to_lowercase() == "n/a";
    let mut participants: Vec<(String, String)> = Vec::new();
    if !draw {
        participants.push((winner.clone(), winner_deck.clone()));
    }
    participants.extend(opponents);
    let (kill_counts, kill_errors) = parse_kills(raw, &indexes.kills);
    let total_kills: i64 = kill_counts.iter().map(|count| count.kills).sum();
    if !draw && winner_deck.is_empty() {
        errors.push("Winner is missing a deck.".to_owned());
    }
    if !(2..=6).contains(&participants.len()) {
        errors.push(
            "Game must have between 2 and 6 players; Other Decks cannot be missing.".to_owned(),
        );
    }
    let names: Vec<String> = participants
        .iter()
        .map(|(player, _)| player.to_lowercase().trim().to_owned())
        .collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != names.len() {
        errors.push("A raw player is listed more than once.".to_owned());
    }
    errors.extend(kill_errors);
    let max_kills = i64::try_from(participants.len().saturating_sub(1)).unwrap_or(i64::MAX);
    if total_kills > max_kills {
        errors.push("Total recorded kills exceed participants minus one.".to_owned());
    }
    let seats = participants
        .into_iter()
        .map(|(player, deck)| {
            let kills = kill_counts
                .iter()
                .find(|count| count.player.to_lowercase() == player.to_lowercase())
                .map_or(0, |count| count.kills);
            let result = if draw {
                "draw"
            } else if winner.to_lowercase() == player.to_lowercase() {
                "win"
            } else {
                "loss"
            };
            SheetSeat {
                player,
                deck,
                kills,
                result: result.to_owned(),
            }
        })
        .collect();
    let date_text = value(raw, indexes.date);
    let date = parse_date(&date_text);
    if date.is_none() {
        errors.push("Date is invalid.".to_owned());
    }
    SheetRow {
        key: raw_key(indexes, raw),
        line,
        date,
        winner,
        deck: winner_deck,
        win_con: value(raw, indexes.win_con),
        notes: value(raw, indexes.notes),
        seats,
        kill_counts,
        errors,
        warnings: if draw {
            vec![DRAW_WARNING.to_owned()]
        } else {
            Vec::new()
        },
    }
}

fn parse_opponents(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    if text.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let pair = crate::regex::compile(r"(?P<player>[^(),;]+?)\s*\((?P<deck>[^()]*)\)");
    let pairs: Vec<(String, String)> = pair
        .captures_iter(text)
        .map(|captures| {
            let get = |name: &str| {
                captures
                    .name(name)
                    .map(|m| m.as_str().trim().to_owned())
                    .unwrap_or_default()
            };
            (get("player"), get("deck"))
        })
        .collect();
    let residue = crate::regex::compile(r"[\s,;]+")
        .replace_all(&pair.replace_all(text, ""), "")
        .into_owned();
    let mut errors = Vec::new();
    if !residue.is_empty() {
        errors.push(format!("Other Decks contains malformed text: {text}"));
    }
    if pairs
        .iter()
        .any(|(player, deck)| player.is_empty() || deck.is_empty())
    {
        errors.push("Every opponent needs a player and deck name.".to_owned());
    }
    (pairs, errors)
}

fn parse_kills(row: &[String], columns: &[(usize, String)]) -> (Vec<KillCount>, Vec<String>) {
    let mut counts = Vec::new();
    let mut errors = Vec::new();
    for (index, player) in columns {
        let raw = value(row, *index);
        let kills = if raw.is_empty() {
            Some(0)
        } else {
            parse_integer(&raw).filter(|kills| *kills >= 0)
        };
        match kills {
            Some(kills) => counts.push(KillCount {
                player: player.clone(),
                kills,
            }),
            None => errors.push(format!("Kills for {player} must be a nonnegative integer.")),
        }
    }
    (counts, errors)
}

fn parse_date(value: &str) -> Option<Date> {
    parse_iso_date(value).or_else(|| slash_date(value, true))
}

fn add_occurrences(rows: Vec<SheetRow>) -> Vec<SheetRow> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    rows.into_iter()
        .map(|mut row| {
            let occurrence = counts.entry(row.key.clone()).or_default();
            *occurrence += 1;
            if *occurrence > 1 {
                row.key = format!("{}-{occurrence}", row.key);
                row.warnings
                    .push(format!("Duplicate row occurrence {occurrence}."));
            }
            row
        })
        .collect()
}

fn index(value: usize) -> Term<'static> {
    Term::Int(i64::try_from(value).unwrap_or(i64::MAX))
}

/// SHA-256 of `:erlang.term_to_binary({:blank_kills_zero, indexes, raw})`, the identity
/// the Elixir importer stored in `sheet_import_receipts` and `sheet:<key>` external ids.
///
/// The BEAM encodes the atom-keyed `indexes` map in its internal key order, which
/// depends on atom creation order at runtime; this uses the order of the map literal in
/// `GoogleSheet.validate_header/1` (what a fresh VM produces), so keys match the Elixir
/// server's whenever its VM did the same.
fn raw_key(indexes: &Indexes, raw: &[String]) -> String {
    let kills = Term::List(
        indexes
            .kills
            .iter()
            .map(|(column, name)| Term::Tuple(vec![index(*column), Term::Binary(name)]))
            .collect(),
    );
    let map = Term::Map(vec![
        (Term::Atom("date"), index(indexes.date)),
        (Term::Atom("winner"), index(indexes.winner)),
        (Term::Atom("deck"), index(indexes.deck)),
        (Term::Atom("win_con"), index(indexes.win_con)),
        (Term::Atom("other_decks"), index(indexes.other_decks)),
        (Term::Atom("notes"), index(indexes.notes)),
        (Term::Atom("kills"), kills),
    ]);
    let term = Term::Tuple(vec![
        Term::Atom("blank_kills_zero"),
        map,
        Term::List(raw.iter().map(|field| Term::Binary(field)).collect()),
    ]);
    hex(&Sha256::digest(etf::encode(&term)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_keys_match_a_fresh_elixir_vm() {
        // :crypto.hash(:sha256, :erlang.term_to_binary({:blank_kills_zero, %{date: 0, winner: 1,
        // deck: 2, win_con: 6, other_decks: 7, notes: 8, kills: [{3, "Dan"}]}, ["a"]}))
        let indexes = Indexes {
            date: 0,
            winner: 1,
            deck: 2,
            win_con: 6,
            other_decks: 7,
            notes: 8,
            kills: vec![(3, "Dan".to_owned())],
        };
        assert_eq!(
            raw_key(&indexes, &["a".to_owned()]),
            "664134b4fb9aab75e665acb9cb78b527ead4e23be48f4af2f412edd61bd1cc17"
        );
    }
}
