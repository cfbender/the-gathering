//! The deck chooser (`TheGathering.Games.DeckPicker`): a weighted random pick among the
//! signed-in member's playable decks that favors never-played, older, skipped, and
//! less-played decks.

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::changeset::cast_integer;
use crate::db::UtcDateTime;

use super::GamesError;
use super::model::{Deck, get_deck, select_decks};
use super::player::get_player_for_user;

const UNPLAYED_BOOST_HOURS: i64 = 30 * 24;

/// A playable deck with how often and how recently it was played.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The deck.
    pub deck: Deck,
    /// Seats that used it.
    pub play_count: i64,
    /// Its latest game.
    pub last_played_at: Option<UtcDateTime>,
    /// Selection weight (set by [`selection_weights`]).
    pub weight: f64,
}

/// The chooser's answer.
#[derive(Clone, Debug, PartialEq)]
pub enum DeckPick {
    /// The member has no linked player (`reason: "player_not_linked"`).
    PlayerNotLinked,
    /// No deck is playable (`reason: "no_eligible_decks"`).
    NoEligibleDecks,
    /// A deck.
    Picked(Box<Candidate>),
}

/// An outcome the member reports for a suggested deck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Chosen: clears the skip count.
    Played,
    /// Skipped: counts one more skip.
    Skipped,
}

impl Outcome {
    /// Parses `played`/`skipped`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "played" => Some(Self::Played),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Played => "played",
            Self::Skipped => "skipped",
        }
    }
}

fn recency_hours(now: UtcDateTime, played_at: UtcDateTime) -> i64 {
    (now.unix().saturating_sub(played_at.unix()) / 3600).saturating_add(1).max(1)
}

fn float(value: i64) -> f64 {
    i32::try_from(value).map_or(f64::from(i32::MAX), f64::from)
}

/// `selection_weights/2`: recency in hours × (skips + 1) ÷ (plays + 1); never-played decks
/// count as a month older than the oldest played one.
pub fn selection_weights(candidates: Vec<Candidate>, now: UtcDateTime) -> Vec<Candidate> {
    let oldest = candidates
        .iter()
        .filter_map(|candidate| candidate.last_played_at.map(|played_at| recency_hours(now, played_at)))
        .max()
        .unwrap_or(0);
    let unplayed = oldest.saturating_add(UNPLAYED_BOOST_HOURS).max(UNPLAYED_BOOST_HOURS);
    candidates
        .into_iter()
        .map(|candidate| {
            let recency = candidate.last_played_at.map_or(unplayed, |played_at| recency_hours(now, played_at));
            let weight = float(recency) * float(candidate.deck.skip_count.saturating_add(1))
                / float(candidate.play_count.saturating_add(1));
            Candidate { weight, ..candidate }
        })
        .collect()
}

fn weighted_pick(weighted: Vec<Candidate>, random: f64) -> Option<Candidate> {
    let total: f64 = weighted.iter().map(|candidate| candidate.weight).sum();
    let threshold = random.clamp(0.0, 1.0) * total;
    let mut cumulative = 0.0;
    let mut last = None;
    for candidate in weighted {
        cumulative += candidate.weight;
        if cumulative >= threshold {
            return Some(candidate);
        }
        last = Some(candidate);
    }
    last
}

async fn playable_decks(conn: &mut SqliteConnection, player_id: i64) -> Result<Vec<Candidate>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT d.id AS "id!", count(s.id) AS "play_count!: i64", max(g.played_at) AS "last_played_at?: UtcDateTime"
           FROM decks d
           LEFT JOIN game_players s ON s.deck_id = d.id
           LEFT JOIN games g ON g.id = s.game_id
           WHERE d.player_id = ? AND d.archived_at IS NULL AND d.included_for_play
           GROUP BY d.id
           ORDER BY lower(d.name), d.id"#,
        player_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let decks = select_decks!("WHERE player_id = ?", player_id).fetch_all(&mut *conn).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let deck = decks.iter().find(|deck| deck.id == row.id)?.clone();
            Some(Candidate { deck, play_count: row.play_count, last_played_at: row.last_played_at, weight: 0.0 })
        })
        .collect())
}

/// `DeckPicker.random_deck/2`. `exclude_id` (cast like an Ecto `:id`) is dropped when
/// other candidates remain; `random` is a uniform draw in `[0, 1]`.
pub async fn random_deck(
    conn: &mut SqliteConnection,
    user_id: i64,
    exclude_id: Option<&Value>,
    now: UtcDateTime,
    random: f64,
) -> Result<DeckPick, sqlx::Error> {
    let Some(player) = get_player_for_user(conn, user_id).await? else { return Ok(DeckPick::PlayerNotLinked) };
    let mut candidates = playable_decks(conn, player.id).await?;
    if candidates.len() > 1
        && let Some(id) = exclude_id.and_then(cast_integer)
    {
        candidates.retain(|candidate| candidate.deck.id != id);
    }
    Ok(match weighted_pick(selection_weights(candidates, now), random) {
        Some(candidate) => DeckPick::Picked(Box::new(candidate)),
        None => DeckPick::NoEligibleDecks,
    })
}

/// `DeckPicker.record_outcome/3`: only the member's own, unretired decks.
pub async fn record_outcome(
    conn: &mut SqliteConnection,
    user_id: i64,
    deck_id: i64,
    outcome: Outcome,
) -> Result<Deck, GamesError> {
    let player = get_player_for_user(conn, user_id).await?.ok_or(GamesError::NotFound)?;
    let deck = get_deck(conn, deck_id)
        .await?
        .filter(|deck| deck.player_id == player.id)
        .ok_or(GamesError::NotFound)?;
    if deck.archived_at.is_some() {
        return Err(GamesError::BadRequest);
    }
    match outcome {
        Outcome::Played => sqlx::query!("UPDATE decks SET skip_count = 0 WHERE id = ?", deck.id).execute(&mut *conn).await?,
        Outcome::Skipped => {
            sqlx::query!("UPDATE decks SET skip_count = skip_count + 1 WHERE id = ?", deck.id).execute(&mut *conn).await?
        }
    };
    get_deck(conn, deck.id).await?.ok_or(GamesError::NotFound)
}
