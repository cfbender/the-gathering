//! Commander statistics aggregated across every player and deck.
//!
//! A seat counts once for each commander card its deck ran, so a partner deck contributes
//! to both partners, and a mirror match contributes every matching seat. Every stored
//! reference (Scryfall id and/or name) resolves to one canonical identity before grouping:
//! the catalog card it names, else the stored id, else the normalized name. The published
//! `id` is that catalog id (or the card name when the catalog lacks the card), and
//! [`get`] accepts the published id, a stored id, or a card name. Each commander shows the
//! art of its most-played deck. Detail trends cover at most the newest 500 games.

use super::query::DateRange;
use std::collections::{BTreeMap, HashSet};

use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::catalog::{self, CardRef, CardSummaries, CardSummary};
use crate::db::UtcDateTime;
use crate::games::{Game, GameResult, Seat};

use super::query::{self, Reference};
use super::records::{Object, Record, group_by, grouped_records, player_entity, sort_records};
use super::summaries;

const TREND_GAME_LIMIT: usize = 500;

/// A commander's canonical identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// A catalog or stored Scryfall id.
    Id(String),
    /// A normalized card name.
    Name(String),
}

/// The resolved card behind a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommanderCard {
    id: Option<String>,
    name: Option<String>,
    art_crop_url: Option<String>,
    image_url: Option<String>,
    game_changer: bool,
    color_identity: Option<String>,
    stored_id: Option<String>,
    printing_id: Option<String>,
}

/// A seat and its game, for one commander slot.
#[derive(Clone, Copy, Debug)]
pub struct SeatInGame<'a> {
    /// The seat (its deck is present).
    pub seat: &'a Seat,
    /// The seat's game.
    pub game: &'a Game,
}

#[derive(Clone, Debug)]
struct Entry<'a> {
    key: Key,
    card: CommanderCard,
    at: SeatInGame<'a>,
}

fn canonical(
    summary: Option<&CardSummary>,
    stored_id: Option<&str>,
    name: Option<&str>,
) -> (Key, CommanderCard) {
    match (summary, stored_id) {
        (Some(summary), _) => (
            Key::Id(summary.id.clone()),
            CommanderCard {
                id: Some(summary.id.clone()),
                name: Some(summary.name.clone()),
                art_crop_url: summary.art_crop_url.clone(),
                image_url: summary.image_url.clone(),
                game_changer: summary.game_changer,
                color_identity: Some(summary.color_identity.clone()),
                stored_id: stored_id.map(str::to_owned),
                printing_id: None,
            },
        ),
        (None, stored_id) => {
            let key = match stored_id {
                Some(id) => Key::Id(id.to_owned()),
                None => Key::Name(lotus::normalize_name(name.unwrap_or_default())),
            };
            (
                key,
                CommanderCard {
                    id: stored_id.map(str::to_owned),
                    name: name.map(str::to_owned),
                    art_crop_url: None,
                    image_url: None,
                    game_changer: false,
                    color_identity: None,
                    stored_id: stored_id.map(str::to_owned),
                    printing_id: None,
                },
            )
        }
    }
}

/// A commander's summary JSON, keyed by published id or (for name-only references) name.
fn commander_json(key: &Key, card: &CommanderCard) -> Object {
    let id = match key {
        Key::Id(id) => json!(id),
        Key::Name(_) => json!(card.name),
    };
    let mut object = Object::new();
    object.insert("id".into(), id);
    object.insert("name".into(), json!(card.name));
    object.insert("image_url".into(), json!(card.image_url));
    object.insert("art_crop_url".into(), json!(card.art_crop_url));
    object.insert("game_changer".into(), json!(card.game_changer));
    object.insert("color_identity".into(), json!(card.color_identity));
    object
}

fn card_refs<'a>(seats: impl IntoIterator<Item = SeatInGame<'a>>) -> Vec<Reference> {
    seats
        .into_iter()
        .filter_map(|at| at.seat.deck.as_ref())
        .flat_map(|deck| {
            [
                (
                    deck.commander_card_id.clone(),
                    Some(deck.commander_name.clone()),
                ),
                (deck.partner_card_id.clone(), deck.partner_name.clone()),
            ]
        })
        .collect()
}

/// One entry per commander card the seat's deck ran.
fn commander_entries<'a>(at: SeatInGame<'a>, summaries: &CardSummaries) -> Vec<Entry<'a>> {
    let Some(deck) = at.seat.deck.as_ref() else {
        return Vec::new();
    };
    [
        (
            deck.commander_card_id.as_deref(),
            Some(deck.commander_name.as_str()),
            deck.commander_printing_id.as_deref(),
        ),
        (
            deck.partner_card_id.as_deref(),
            deck.partner_name.as_deref(),
            deck.partner_printing_id.as_deref(),
        ),
    ]
    .into_iter()
    .filter(|(id, name, _)| id.is_some() || name.is_some_and(|name| !name.is_empty()))
    .map(|(id, name, printing)| {
        let (key, mut card) = canonical(summaries.get(id, name), id, name);
        card.printing_id = printing.map(str::to_owned);
        Entry { key, card, at }
    })
    .collect()
}

/// Gives every entry of a commander the art of its most-played deck (ties go to the most
/// recently played): that deck's printing, else the catalog default.
async fn put_deck_art<'a>(
    conn: &mut SqliteConnection,
    entries: Vec<Entry<'a>>,
) -> Result<Vec<Entry<'a>>, sqlx::Error> {
    let printings: BTreeMap<Key, Option<String>> =
        group_by(entries.iter(), |entry| entry.key.clone())
            .into_iter()
            .map(|(key, rows)| {
                let decks = group_by(rows, |entry| entry.at.seat.deck_id);
                let mut best: Option<((usize, i64), &Entry<'_>)> = None;
                for deck_rows in decks.values() {
                    let last_played = deck_rows
                        .iter()
                        .map(|entry| entry.at.game.played_at.unix())
                        .max()
                        .unwrap_or_default();
                    let score = (deck_rows.len(), last_played);
                    if let Some(first) = deck_rows.first()
                        && best
                            .as_ref()
                            .is_none_or(|(best_score, _)| score > *best_score)
                    {
                        best = Some((score, first));
                    }
                }
                (
                    key,
                    best.and_then(|(_, entry)| entry.card.printing_id.clone()),
                )
            })
            .collect();
    let refs: Vec<CardRef> = printings
        .values()
        .flatten()
        .map(|id| CardRef::Printing(Some(id.clone())))
        .collect();
    let urls = catalog::art_crop_urls_in(conn, &refs).await?;
    Ok(entries
        .into_iter()
        .map(|mut entry| {
            if let Some(Some(printing)) = printings.get(&entry.key) {
                entry.card.art_crop_url = urls
                    .art_crop_url(None, None, Some(printing))
                    .or_else(|| entry.card.art_crop_url.clone());
                entry.card.image_url = urls
                    .card_image_url(None, None, Some(printing))
                    .or_else(|| entry.card.image_url.clone());
            }
            entry
        })
        .collect())
}

fn unique_count<T: Eq + std::hash::Hash>(values: impl IntoIterator<Item = T>) -> usize {
    values.into_iter().collect::<HashSet<T>>().len()
}

/// Groups entries per commander into `commander + record (+ list extras)`.
fn commander_records(entries: &[Entry<'_>], extras: bool) -> Vec<Object> {
    let mut rows: Vec<Object> = group_by(entries.iter(), |entry| entry.key.clone())
        .into_iter()
        .filter_map(|(key, rows)| {
            let first = rows.first()?;
            let mut object = commander_json(&key, &first.card);
            Record::of(rows.iter().map(|entry| entry.at.seat.result)).merge_into(&mut object);
            if extras {
                object.insert(
                    "pilots".into(),
                    json!(unique_count(
                        rows.iter().map(|entry| entry.at.seat.player_id)
                    )),
                );
                object.insert(
                    "decks".into(),
                    json!(unique_count(rows.iter().map(|entry| entry.at.seat.deck_id))),
                );
                let last: Option<UtcDateTime> =
                    rows.iter().map(|entry| entry.at.game.played_at).max();
                object.insert("last_played_at".into(), json!(last));
            }
            Some(object)
        })
        .collect();
    sort_records(&mut rows);
    rows
}

/// Every commander among `seats`, most played first.
pub async fn summarize(
    conn: &mut SqliteConnection,
    seats: &[SeatInGame<'_>],
) -> Result<Vec<Object>, sqlx::Error> {
    let summaries = catalog::card_summaries_in(conn, &card_refs(seats.iter().copied())).await?;
    let entries: Vec<Entry<'_>> = seats
        .iter()
        .flat_map(|at| commander_entries(*at, &summaries))
        .collect();
    let entries = put_deck_art(conn, entries).await?;
    Ok(commander_records(&entries, true))
}

/// Seats with decks, newest game first, then by seat number.
pub fn deck_seats(games: &[Game]) -> Vec<SeatInGame<'_>> {
    games
        .iter()
        .flat_map(|game| {
            let mut seats: Vec<&Seat> = game
                .seats
                .iter()
                .filter(|seat| seat.deck.is_some())
                .collect();
            seats.sort_by_key(|seat| seat.seat);
            seats.into_iter().map(move |seat| SeatInGame { seat, game })
        })
        .collect()
}

/// Every commander played in the date range.
pub async fn list(
    conn: &mut SqliteConnection,
    params: &DateRange,
) -> Result<Vec<Value>, sqlx::Error> {
    let games = query::games(conn, params, None, None).await?;
    let seats = deck_seats(&games);
    Ok(summarize(conn, &seats)
        .await?
        .into_iter()
        .map(Value::Object)
        .collect())
}

fn normalized(value: Option<&str>) -> Option<String> {
    value.map(lotus::normalize_name)
}

async fn resolve(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<Option<(Key, CommanderCard)>, sqlx::Error> {
    let direct =
        catalog::card_summaries_in(conn, &[(Some(id.to_owned()), Some(id.to_owned()))]).await?;
    if let Some(summary) = direct.get(Some(id), Some(id)) {
        return Ok(Some(canonical(
            Some(summary),
            Some(id),
            Some(&summary.name),
        )));
    }
    let references = query::commander_references(conn, id).await?;
    let summaries = catalog::card_summaries_in(conn, &references).await?;
    let wanted = normalized(Some(id));
    Ok(references.iter().find_map(|(stored_id, name)| {
        (stored_id.as_deref() == Some(id) || normalized(name.as_deref()) == wanted).then(|| {
            canonical(
                summaries.get(stored_id.as_deref(), name.as_deref()),
                stored_id.as_deref(),
                name.as_deref(),
            )
        })
    }))
}

/// Detail for one commander by published id, stored id, or card name; `None` when
/// never played in the range.
pub async fn get(
    conn: &mut SqliteConnection,
    id: &str,
    params: &DateRange,
) -> Result<Option<Value>, sqlx::Error> {
    let Some((key, card)) = resolve(conn, id).await? else {
        return Ok(None);
    };
    let mut ids: Vec<String> = Vec::new();
    for candidate in [Some(id.to_owned()), card.id.clone(), card.stored_id.clone()]
        .into_iter()
        .flatten()
    {
        if !ids.contains(&candidate) {
            ids.push(candidate);
        }
    }
    let mut names: Vec<String> = card.name.clone().into_iter().collect();
    let (stored_ids, stored_names) = query::commander_aliases(conn, &ids, &names).await?;
    for stored in stored_ids {
        if !ids.contains(&stored) {
            ids.push(stored);
        }
    }
    for stored in stored_names {
        if !names.contains(&stored) {
            names.push(stored);
        }
    }
    let lowered: HashSet<String> = names.iter().map(|name| name.to_lowercase()).collect();
    let games = query::games(conn, params, None, None).await?;
    let candidates: Vec<SeatInGame<'_>> = deck_seats(&games)
        .into_iter()
        .filter(|at| {
            at.seat.deck.as_ref().is_some_and(|deck| {
                deck.commander_card_id
                    .as_ref()
                    .is_some_and(|id| ids.contains(id))
                    || deck
                        .partner_card_id
                        .as_ref()
                        .is_some_and(|id| ids.contains(id))
                    || lowered.contains(&deck.commander_name.to_lowercase())
                    || deck
                        .partner_name
                        .as_ref()
                        .is_some_and(|name| lowered.contains(&name.to_lowercase()))
            })
        })
        .collect();
    let summaries =
        catalog::card_summaries_in(conn, &card_refs(candidates.iter().copied())).await?;
    let entries: Vec<Entry<'_>> = candidates
        .iter()
        .flat_map(|at| commander_entries(*at, &summaries))
        .filter(|entry| entry.key == key)
        .collect();
    let entries = put_deck_art(conn, entries).await?;
    detail(conn, &entries, &summaries).await
}

async fn detail(
    conn: &mut SqliteConnection,
    entries: &[Entry<'_>],
    summaries: &CardSummaries,
) -> Result<Option<Value>, sqlx::Error> {
    let Some(first) = entries.first() else {
        return Ok(None);
    };
    let seats: Vec<SeatInGame<'_>> = entries.iter().map(|entry| entry.at).collect();
    let seat_ids: Vec<i64> = seats.iter().map(|at| at.seat.id).collect();
    let mut game_ids: Vec<i64> = Vec::new();
    for at in &seats {
        if !game_ids.contains(&at.seat.game_id) {
            game_ids.push(at.seat.game_id);
        }
    }
    let recent = query::recent_games(conn, &game_ids, 10).await?;
    let tracked: HashSet<i64> = seat_ids.iter().copied().collect();

    // partners/3
    let partner_entries: Vec<Entry<'_>> = seats
        .iter()
        .flat_map(|at| commander_entries(*at, summaries))
        .filter(|entry| entry.key != first.key)
        .collect();
    let partner_entries = put_deck_art(conn, partner_entries).await?;
    let partners: Vec<Value> = commander_records(&partner_entries, false)
        .into_iter()
        .map(Value::Object)
        .collect();

    let opponents = summaries::records(query::opponent_counts(conn, &game_ids, &seat_ids).await?);

    // trend_games/1: one point per game, every tracked seat counting.
    let mut trend: Vec<(UtcDateTime, i64, Vec<GameResult>)> =
        group_by(entries.iter(), |entry| entry.at.seat.game_id)
            .into_values()
            .filter_map(|rows| {
                let game = rows.first()?.at.game;
                let mut seen = HashSet::new();
                let results: Vec<GameResult> = rows
                    .iter()
                    .filter(|entry| seen.insert(entry.at.seat.id))
                    .map(|entry| entry.at.seat.result)
                    .collect();
                Some((game.played_at, game.id, results))
            })
            .collect();
    trend.sort_by_key(|(played_at, id, _)| std::cmp::Reverse((played_at.unix(), *id)));
    trend.truncate(TREND_GAME_LIMIT);

    let seat_refs: Vec<&Seat> = seats.iter().map(|at| at.seat).collect();
    let detail = json!({
        "commander": commander_json(&first.key, &first.card),
        "record": Record::of_seats(seat_refs.iter().copied()).to_json(),
        "pilots": grouped_records(seat_refs.iter().copied(), player_entity, |seat| seat.player_id),
        "decks": grouped_records(
            seat_refs.iter().copied(),
            |seat| seat.deck.as_ref().map(|deck| summaries::deck(deck, summaries)).unwrap_or_default(),
            |seat| seat.deck_id,
        ),
        "partners": partners,
        "opponents": opponents,
        "win_rate_over_time": super::records::cumulative_win_rate(
            trend.into_iter().map(|(played_at, _, results)| (played_at, results)),
        ),
        "recent_games": recent
            .iter()
            .map(|game| {
                let results: Vec<GameResult> =
                    game.seats.iter().filter(|seat| tracked.contains(&seat.id)).map(|seat| seat.result).collect();
                Value::Object(summaries::recent_game(game, &results))
            })
            .collect::<Vec<_>>(),
    });
    Ok(Some(detail))
}
