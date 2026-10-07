//! Parsing an import and matching its players and decks against existing records
//! (`TheGathering.Imports.Preview`).

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqliteConnection;

use crate::games::{Resolution, deck, fold_name, resolve_player};

use super::csv_transfer::Review;
use super::{ImportGame, ImportSeat, LineError, Warning, csv, mythic_track};

/// Which parser reads the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The CSV template (or Mythic Track's spreadsheet export).
    Csv,
    /// Mythic Track's game-list JSON.
    MythicTrack,
}

impl Source {
    /// The `games.source` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::MythicTrack => "mythic_track",
        }
    }
}

/// An existing player an import seat resolves to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MatchedPlayer {
    /// Player id.
    pub id: i64,
    /// Player name.
    pub name: String,
}

/// Players the import creates and reuses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PlayerMatches {
    /// Names of new players.
    pub create: Vec<String>,
    /// Existing players.
    pub matched: Vec<MatchedPlayer>,
}

/// A deck the import creates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NewDeck {
    /// Owner's name in the file.
    pub player: String,
    /// Deck name.
    pub name: String,
    /// Commander.
    pub commander: String,
}

/// An existing deck the import reuses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MatchedDeck {
    /// Deck id.
    pub id: i64,
    /// Owner id.
    pub player_id: i64,
    /// Owner name.
    pub player: String,
    /// Deck name.
    pub name: String,
    /// Commander.
    pub commander: String,
}

/// Decks the import creates and reuses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DeckMatches {
    /// New decks.
    pub create: Vec<NewDeck>,
    /// Existing decks.
    pub matched: Vec<MatchedDeck>,
}

/// An import preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// No errors.
    pub valid: bool,
    /// Parsed games.
    pub games: Vec<ImportGame>,
    /// Player matching.
    pub players: PlayerMatches,
    /// Deck matching.
    pub decks: DeckMatches,
    /// Errors.
    pub errors: Vec<LineError>,
    /// Skipped games.
    pub warnings: Vec<Warning>,
    /// CSV corrections: the fingerprint a commit must present.
    pub revision: Option<String>,
    /// CSV corrections: what the commit would do to each game.
    pub review: Option<Vec<Review>>,
}

/// `Preview.run/2`.
pub async fn run(
    conn: &mut SqliteConnection,
    source: Source,
    payload: &str,
) -> Result<Preview, sqlx::Error> {
    let parsed = match source {
        Source::Csv => csv::parse(payload).map(|(games, errors)| (games, errors, Vec::new())),
        Source::MythicTrack => mythic_track::parse(payload),
    };
    match parsed {
        Ok((games, errors, warnings)) => {
            let players = match_players(conn, &games).await?;
            let decks = match_decks(conn, &games).await?;
            Ok(Preview {
                valid: errors.is_empty(),
                games,
                players,
                decks,
                errors,
                warnings,
                revision: None,
                review: None,
            })
        }
        Err(errors) => Ok(Preview {
            valid: false,
            games: Vec::new(),
            players: PlayerMatches::default(),
            decks: DeckMatches::default(),
            errors,
            warnings: Vec::new(),
            revision: None,
            review: None,
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PlayerKey {
    Discord(String),
    Name(String),
}

fn player_key(seat: &ImportSeat) -> PlayerKey {
    match &seat.discord_id {
        Some(id) => PlayerKey::Discord(id.clone()),
        None => PlayerKey::Name(fold_name(&seat.player)),
    }
}

fn all_seats(games: &[ImportGame]) -> impl Iterator<Item = &ImportSeat> {
    games.iter().flat_map(|game| &game.seats)
}

/// Reverses `items` and drops repeats (`Enum.uniq/1` of a list built by prepending).
fn unique_reversed<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut unique: Vec<T> = Vec::with_capacity(items.len());
    for item in items.into_iter().rev() {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique
}

fn sort_key(name: &str) -> String {
    name.to_lowercase()
}

async fn match_players(
    conn: &mut SqliteConnection,
    games: &[ImportGame],
) -> Result<PlayerMatches, sqlx::Error> {
    let mut seen = Vec::new();
    let mut identities = Vec::new();
    for seat in all_seats(games) {
        let key = player_key(seat);
        if !seen.contains(&key) {
            seen.push(key);
            identities.push((seat.player.clone(), seat.discord_id.clone()));
        }
    }
    let mut result = PlayerMatches::default();
    for resolution in resolve_player::preview(conn, &identities).await? {
        match resolution {
            Resolution::Create(name) => result.create.push(name),
            Resolution::Matched(player) => {
                result.matched.push(MatchedPlayer {
                    id: player.id,
                    name: player.name,
                });
            }
        }
    }
    // Elixir prepended while reducing, so ties keep the reversed order.
    result.create.reverse();
    result.matched = unique_reversed(result.matched);
    result.create.sort_by_key(|name| sort_key(name));
    result.matched.sort_by_key(|player| sort_key(&player.name));
    Ok(result)
}

struct Existing {
    by_name: HashMap<String, (i64, String)>,
    by_discord: HashMap<String, (i64, String)>,
}

async fn existing_players(
    conn: &mut SqliteConnection,
    seats: &[&ImportSeat],
) -> Result<Existing, sqlx::Error> {
    let mut existing = Existing {
        by_name: HashMap::new(),
        by_discord: HashMap::new(),
    };
    if seats.is_empty() {
        return Ok(existing);
    }
    let names: Vec<String> = seats.iter().map(|seat| fold_name(&seat.player)).collect();
    let discord_ids: Vec<&str> = seats
        .iter()
        .filter_map(|seat| seat.discord_id.as_deref())
        .collect();
    let names = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
    let discord_ids = serde_json::to_string(&discord_ids).unwrap_or_else(|_| "[]".into());
    let players = sqlx::query!(
        r#"SELECT id AS "id!", name, discord_id FROM players
           WHERE lower(name) IN (SELECT value FROM json_each(?))
              OR discord_id IN (SELECT value FROM json_each(?))"#,
        names,
        discord_ids
    )
    .fetch_all(&mut *conn)
    .await?;
    for player in players {
        existing
            .by_name
            .insert(fold_name(&player.name), (player.id, player.name.clone()));
        if let Some(discord_id) = player.discord_id {
            existing
                .by_discord
                .insert(discord_id, (player.id, player.name));
        }
    }
    Ok(existing)
}

async fn match_decks(
    conn: &mut SqliteConnection,
    games: &[ImportGame],
) -> Result<DeckMatches, sqlx::Error> {
    let seats: Vec<&ImportSeat> = all_seats(games).collect();
    let existing = existing_players(conn, &seats).await?;
    let mut seen = Vec::new();
    let mut result = DeckMatches::default();
    for seat in seats {
        let key = (player_key(seat), fold_name(&seat.deck));
        if seen.contains(&key) {
            continue;
        }
        let player = match &key.0 {
            PlayerKey::Discord(id) => existing.by_discord.get(id),
            PlayerKey::Name(name) => existing.by_name.get(name),
        };
        seen.push(key);
        // Same rule as the commit: a deck is reused by name, or by commander pairing.
        let found = match player {
            Some((player_id, _)) => {
                deck::find_deck(
                    conn,
                    *player_id,
                    &seat.deck,
                    Some(&seat.commander),
                    seat.partner_name.as_deref(),
                )
                .await?
            }
            None => None,
        };
        match (found, player) {
            (Some(found), Some((player_id, player_name))) => {
                result.matched.push(MatchedDeck {
                    id: found.id,
                    player_id: *player_id,
                    player: player_name.clone(),
                    name: found.name,
                    commander: found.commander_name,
                });
            }
            _ => result.create.push(NewDeck {
                player: seat.player.clone(),
                name: seat.deck.clone(),
                commander: seat.commander.clone(),
            }),
        }
    }
    result.create.reverse();
    result.matched = unique_reversed(result.matched);
    result.create.sort_by_key(|deck| sort_key(&deck.name));
    result.matched.sort_by_key(|deck| sort_key(&deck.name));
    Ok(result)
}
