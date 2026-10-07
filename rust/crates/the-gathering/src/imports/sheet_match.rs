//! Finding the recorded game a sheet row describes (`TheGathering.Imports.SheetMatch`).

use time::Date;

use crate::games::fold_name;

use super::sheet_preview::{Candidate, DeckRef};
use super::sheet_resolution::{Choice, ResolvedSeat, SeatPlayer};

/// The single candidate with the same players (same day first, else the nearby days),
/// broken by deck evidence when several match; with the reason shown to the admin.
pub fn find<'a>(
    date: Option<Date>,
    seats: &[ResolvedSeat],
    candidates: &'a [Candidate],
    decks: &[DeckRef],
) -> (Option<&'a Candidate>, String) {
    let ids: Option<Vec<i64>> = seats
        .iter()
        .map(|seat| match &seat.player_id {
            Some(SeatPlayer::Id(id)) => Some(Some(*id)),
            Some(SeatPlayer::New(_)) => Some(None),
            None => None,
        })
        .collect::<Option<Vec<Option<i64>>>>()
        .and_then(|ids| ids.into_iter().collect());
    let mut sorted = ids.clone().unwrap_or_default();
    sorted.sort_unstable();
    let mut unique = sorted.clone();
    unique.dedup();
    let matches: Vec<&Candidate> = match &ids {
        Some(_) if unique.len() == sorted.len() => candidates
            .iter()
            .filter(|game| {
                let mut players: Vec<i64> = game.seats.iter().map(|seat| seat.player_id).collect();
                players.sort_unstable();
                players == sorted
            })
            .collect(),
        _ => Vec::new(),
    };
    let same_day: Vec<&Candidate> = matches
        .iter()
        .copied()
        .filter(|game| Some(game.played_at.date()) == date)
        .collect();
    let (pool, date_reason) = if same_day.is_empty() {
        (matches, "nearby date")
    } else {
        (same_day, "date")
    };
    match pool.as_slice() {
        [game] => (Some(*game), format!("Matched by {date_reason} and players")),
        [] => (
            None,
            "No match with the same players on nearby dates".to_owned(),
        ),
        games => distinguish(games, seats, decks, date_reason),
    }
}

fn distinguish<'a>(
    games: &[&'a Candidate],
    seats: &[ResolvedSeat],
    decks: &[DeckRef],
    date_reason: &str,
) -> (Option<&'a Candidate>, String) {
    let mut ranked: Vec<(&Candidate, usize)> = games
        .iter()
        .map(|game| (*game, deck_score(game, seats, decks)))
        .collect();
    ranked.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
    match ranked.as_slice() {
        [(game, score), (_, next), ..] if score > next && *score > 0 => (
            Some(*game),
            format!("Matched by {date_reason}, players and decks"),
        ),
        _ => (
            None,
            "Multiple games match these players and dates; choose a game".to_owned(),
        ),
    }
}

fn deck_score(game: &Candidate, seats: &[ResolvedSeat], decks: &[DeckRef]) -> usize {
    seats
        .iter()
        .filter(|seat| {
            let Some(SeatPlayer::Id(player_id)) = &seat.player_id else {
                return false;
            };
            let Some(existing) = game.seats.iter().find(|s| s.player_id == *player_id) else {
                return false;
            };
            let Some(deck) = decks.iter().find(|deck| Some(deck.id) == existing.deck_id) else {
                return false;
            };
            seat.deck_id == Some(Choice::Id(deck.id))
                || same_name(&seat.deck, &deck.name)
                || same_name(&seat.deck, &deck.commander_name)
        })
        .count()
}

/// Short commander names can identify a deck, but arbitrary fuzzy nicknames cannot.
fn same_name(left: &str, right: &str) -> bool {
    let left = normalize(left);
    let right = normalize(right);
    left == right || (left.chars().count() >= 4 && right.starts_with(&format!("{left} ")))
}

fn normalize(name: &str) -> String {
    crate::regex::compile(r"[^\p{L}\p{N}]+")
        .replace_all(&fold_name(name), " ")
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_by_prefix_only_when_long_enough() {
        assert!(same_name("Voja", "Voja, Jaws of the Conclave"));
        assert!(same_name("edgar markov", "Edgar Markov"));
        assert!(!same_name("Vo", "Vo Jaws"));
        assert!(!same_name("Nickname", "Edgar Markov"));
    }
}
