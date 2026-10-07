//! Resolving one sheet row against players, decks, nearby games, and the admin's choices.

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::db::IsoDate;
use crate::games::{deck, fold_name};

use super::google_sheet::{KillCount, SheetRow, SheetSeat};
use super::sheet_match;
use super::sheet_preview::{Candidate, DeckRef, PlayerRef};

/// An admin choice: a record id or a keyword (`new`, `create`, `skip`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Choice {
    /// A record id.
    Id(i64),
    /// A keyword.
    Text(String),
}

impl Choice {
    /// Reads a validated params value (integers and strings only).
    pub fn of(value: &Value) -> Option<Self> {
        match value {
            Value::Number(number) => number.as_i64().map(Self::Id),
            Value::String(text) => Some(Self::Text(text.clone())),
            _ => None,
        }
    }

    fn is(&self, keyword: &str) -> bool {
        matches!(self, Self::Text(text) if text == keyword)
    }
}

/// A seat's player: an existing id or `"new:<folded name>"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum SeatPlayer {
    /// Existing player.
    Id(i64),
    /// A player to create, as `new:<folded name>`.
    New(String),
}

/// A resolved seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResolvedSeat {
    /// Name as written.
    pub player: String,
    /// Deck as written.
    pub deck: String,
    /// Kills credited to the resolved player.
    pub kills: i64,
    /// Result.
    pub result: String,
    /// Resolved player.
    pub player_id: Option<SeatPlayer>,
    /// `Jason.encode!([player, deck])`, the key of the deck mapping.
    pub deck_key: String,
    /// Resolved deck: an id, `"new"`, or nothing.
    pub deck_id: Option<Choice>,
    /// Whether the deck mapping can be committed.
    pub deck_valid: bool,
}

/// A difference between the row and its target game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SheetChange {
    /// `result`, `kills`, `deck`, or `notes`.
    pub field: &'static str,
    /// Whose seat (none for notes).
    pub player: Option<String>,
    /// Current value.
    pub before: Value,
    /// Sheet value.
    pub after: Value,
}

/// A row with its resolution, as the preview renders it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResolvedRow {
    /// Row identity.
    pub key: String,
    /// Line.
    pub line: i64,
    /// Date.
    pub date: Option<IsoDate>,
    /// Winner column.
    pub winner: String,
    /// Winner's deck column.
    pub deck: String,
    /// Win Con column.
    pub win_con: String,
    /// `Win con: ...` and the Notes column.
    pub notes: String,
    /// Resolved seats.
    pub seats: Vec<ResolvedSeat>,
    /// Kill columns.
    pub kill_counts: Vec<KillCount>,
    /// Problems.
    pub errors: Vec<String>,
    /// Warnings.
    pub warnings: Vec<String>,
    /// The game id to update, `create`, or `skip`.
    pub action: Choice,
    /// `changed`, `unchanged`, `review`, or `reconciled`.
    pub status: &'static str,
    /// How the target was chosen.
    pub match_reason: String,
    /// Differences from the target.
    pub changes: Vec<SheetChange>,
    /// The game a previous import of this row reconciled.
    pub imported_id: Option<i64>,
    /// Games on nearby dates.
    pub candidates: Vec<Candidate>,
    /// The game to update.
    pub target: Option<Candidate>,
}

fn choice(params: &Value, group: &str, key: &str) -> Option<Choice> {
    params
        .get(group)
        .and_then(|map| map.get(key))
        .and_then(Choice::of)
}

/// The inputs shared by every row.
pub struct Context<'a> {
    /// The request (`text`, `players`, `decks`, `actions`).
    pub params: &'a Value,
    /// Every player.
    pub players: &'a [PlayerRef],
    /// Every deck.
    pub decks: &'a [DeckRef],
}

fn player_id(name: &str, context: &Context<'_>) -> Option<SeatPlayer> {
    match choice(context.params, "players", name) {
        Some(Choice::Text(text)) if text == "new" => {
            Some(SeatPlayer::New(format!("new:{}", fold_name(name))))
        }
        Some(Choice::Id(id)) => context
            .players
            .iter()
            .any(|player| player.id == id)
            .then_some(SeatPlayer::Id(id)),
        _ => context
            .players
            .iter()
            .find(|player| fold_name(&player.name) == fold_name(name))
            .map(|player| SeatPlayer::Id(player.id)),
    }
}

async fn resolve_seat(
    conn: &mut SqliteConnection,
    seat: &SheetSeat,
    context: &Context<'_>,
    target: Option<&Candidate>,
) -> Result<ResolvedSeat, sqlx::Error> {
    let player = player_id(&seat.player, context);
    let key = json!([seat.player, seat.deck]).to_string();
    let deck_choice = match (choice(context.params, "decks", &key), &player) {
        (Some(choice), Some(SeatPlayer::Id(player_id))) if choice.is("new") => {
            match deck::find_deck(conn, *player_id, &seat.deck, Some(&seat.deck), None).await? {
                Some(found) => Some(Choice::Id(found.id)),
                None => Some(choice),
            }
        }
        (choice, _) => choice,
    };
    let player_number = match &player {
        Some(SeatPlayer::Id(id)) => Some(*id),
        _ => None,
    };
    let existing = target.and_then(|target| {
        target
            .seats
            .iter()
            .find(|existing| Some(existing.player_id) == player_number)
    });
    let exact = context.decks.iter().find(|deck| {
        Some(deck.player_id) == player_number && fold_name(&deck.name) == fold_name(&seat.deck)
    });
    let deck_id = deck_choice.or_else(|| match existing {
        Some(existing) => existing.deck_id.map(Choice::Id),
        None => exact.map(|deck| Choice::Id(deck.id)),
    });
    let deck_valid = match &deck_id {
        Some(Choice::Text(text)) if text == "new" => true,
        None => existing.is_some(),
        Some(Choice::Id(id)) => context
            .decks
            .iter()
            .any(|deck| deck.id == *id && Some(deck.player_id) == player_number),
        Some(Choice::Text(_)) => false,
    };
    Ok(ResolvedSeat {
        player: seat.player.clone(),
        deck: seat.deck.clone(),
        kills: seat.kills,
        result: seat.result.clone(),
        player_id: player,
        deck_key: key,
        deck_id,
        deck_valid,
    })
}

/// `SheetResolution.resolve/6`.
pub async fn resolve(
    conn: &mut SqliteConnection,
    row: &SheetRow,
    context: &Context<'_>,
    candidates: Vec<Candidate>,
    imported_id: Option<i64>,
) -> Result<ResolvedRow, sqlx::Error> {
    let chosen = choice(context.params, "actions", &row.key);
    let mut unresolved = Vec::with_capacity(row.seats.len());
    for seat in &row.seats {
        unresolved.push(resolve_seat(conn, seat, context, None).await?);
    }
    let (matched, reason) = sheet_match::find(row.date, &unresolved, &candidates, context.decks);
    let target: Option<Candidate> = match &chosen {
        Some(Choice::Id(id)) => candidates.iter().find(|game| game.id == *id).cloned(),
        Some(choice) if choice.is("create") => None,
        _ => matched.cloned(),
    };
    let action = chosen.clone().unwrap_or_else(|| match &target {
        Some(target) => Choice::Id(target.id),
        None => Choice::Text("skip".to_owned()),
    });
    let mut seats = Vec::with_capacity(row.seats.len());
    for seat in &row.seats {
        seats.push(resolve_seat(conn, seat, context, target.as_ref()).await?);
    }
    let kills: Vec<(Option<SeatPlayer>, i64)> = row
        .kill_counts
        .iter()
        .map(|count| (player_id(&count.player, context), count.kills))
        .collect();
    for seat in &mut seats {
        seat.kills = kills
            .iter()
            .find(|(id, _)| id.is_some() && *id == seat.player_id)
            .map_or(0, |(_, count)| *count);
    }
    let mut errors = row.errors.clone();
    errors.extend(validate(&seats, &kills, &action, target.as_ref()));
    let notes = notes(row);
    let changes = changes(target.as_ref(), &seats, &notes, context.decks);
    let status = if imported_id.is_some() {
        "reconciled"
    } else if !errors.is_empty() || target.is_none() {
        "review"
    } else if changes.is_empty() {
        "unchanged"
    } else {
        "changed"
    };
    let action = match (status, &chosen) {
        ("unchanged" | "reconciled", _) | ("review", None) => Choice::Text("skip".to_owned()),
        _ => action,
    };
    Ok(ResolvedRow {
        key: row.key.clone(),
        line: row.line,
        date: row.date.map(IsoDate),
        winner: row.winner.clone(),
        deck: row.deck.clone(),
        win_con: row.win_con.clone(),
        notes,
        seats,
        kill_counts: row.kill_counts.clone(),
        errors,
        warnings: row.warnings.clone(),
        action,
        status,
        match_reason: if matches!(chosen, Some(Choice::Id(_))) {
            "Manually selected game".to_owned()
        } else {
            reason
        },
        changes,
        imported_id,
        candidates,
        target,
    })
}

fn changes(
    target: Option<&Candidate>,
    seats: &[ResolvedSeat],
    notes: &str,
    decks: &[DeckRef],
) -> Vec<SheetChange> {
    let Some(target) = target else {
        return Vec::new();
    };
    let mut changes = Vec::new();
    let mut diff = |field: &'static str, player: Option<&str>, before: Value, after: Value| {
        if before != after {
            changes.push(SheetChange {
                field,
                player: player.map(str::to_owned),
                before,
                after,
            });
        }
    };
    for seat in seats {
        let Some(existing) = target
            .seats
            .iter()
            .find(|existing| seat.player_id == Some(SeatPlayer::Id(existing.player_id)))
        else {
            continue;
        };
        diff(
            "result",
            Some(&existing.player),
            json!(existing.result),
            json!(seat.result),
        );
        diff(
            "kills",
            Some(&existing.player),
            json!(existing.kills),
            json!(seat.kills),
        );
        if existing.deck_id.map(Choice::Id) != seat.deck_id {
            let deck = decks
                .iter()
                .find(|deck| seat.deck_id == Some(Choice::Id(deck.id)));
            diff(
                "deck",
                Some(&existing.player),
                json!(existing.deck),
                json!(deck.map_or(seat.deck.as_str(), |deck| deck.name.as_str())),
            );
        }
    }
    let after = if notes.is_empty() {
        target.notes.clone()
    } else {
        Some(notes.to_owned())
    };
    diff("notes", None, json!(target.notes), json!(after));
    changes
}

fn validate(
    seats: &[ResolvedSeat],
    kills: &[(Option<SeatPlayer>, i64)],
    action: &Choice,
    target: Option<&Candidate>,
) -> Vec<String> {
    let ids: Vec<&Option<SeatPlayer>> = seats.iter().map(|seat| &seat.player_id).collect();
    let duplicates = |items: &[&SeatPlayer]| {
        items
            .iter()
            .enumerate()
            .any(|(index, item)| items.iter().take(index).any(|earlier| earlier == item))
    };
    let all_ids: Vec<Option<&SeatPlayer>> = ids.iter().map(|id| id.as_ref()).collect();
    let id_duplicates = all_ids
        .iter()
        .enumerate()
        .any(|(index, item)| all_ids.iter().take(index).any(|earlier| earlier == item));
    let recorded: Vec<&SeatPlayer> = kills.iter().filter_map(|(id, _)| id.as_ref()).collect();
    let mut errors = Vec::new();
    let mut error = |invalid: bool, message: &str| {
        if invalid {
            errors.push(message.to_owned());
        }
    };
    error(
        ids.iter().any(|id| id.is_none()),
        "Map every player to an existing player or explicitly create one.",
    );
    error(
        id_duplicates,
        "Player aliases resolve to the same player twice. Repair this row.",
    );
    error(
        seats.iter().any(|seat| !seat.deck_valid),
        "Map missing decks before creating games.",
    );
    error(
        kills.iter().any(|(id, count)| {
            *count > 0
                && id
                    .as_ref()
                    .is_none_or(|id| !ids.iter().any(|seat| seat.as_ref() == Some(id)))
        }),
        "A positive kill count belongs to someone not seated. Fix the column or player mapping.",
    );
    error(
        duplicates(&recorded),
        "Multiple kill columns map to the same player.",
    );
    error(
        !action.is("skip") && !action.is("create") && target.is_none(),
        "Choose a game from the nearby dates.",
    );
    if let Some(target) = target {
        let mut seated: Vec<Option<i64>> = ids
            .iter()
            .map(|id| match id {
                Some(SeatPlayer::Id(id)) => Some(*id),
                _ => None,
            })
            .collect();
        seated.sort_unstable();
        let mut expected: Vec<Option<i64>> = target
            .seats
            .iter()
            .map(|seat| Some(seat.player_id))
            .collect();
        expected.sort_unstable();
        error(
            seated != expected || ids.iter().any(|id| !matches!(id, Some(SeatPlayer::Id(_)))),
            "Player lists differ. Correct this row or edit the existing game before reconciling.",
        );
    }
    errors
}

fn notes(row: &SheetRow) -> String {
    let win_con = (!row.win_con.is_empty() && row.win_con != "N/A")
        .then(|| format!("Win con: {}", row.win_con));
    [win_con, Some(row.notes.clone())]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
