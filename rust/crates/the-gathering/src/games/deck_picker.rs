//! The deck chooser: a weighted random pick among the signed-in member's playable decks.
//!
//! The built-in weights favor never-played, older, skipped, and less-played decks. A member
//! can replace them with a Lua script ([`super::deck_script`]) that weighs each deck from the
//! tables [`super::chooser_context`] builds; a script that fails falls back to the built-in
//! weights and reports why.

use std::time::Duration;

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::db::UtcDateTime;
use crate::local_time::Zone;

use super::GamesError;
use super::chooser_context;
use super::deck_script::{self, ScriptError, Weighted};
use super::model::{Deck, Player, get_deck, select_decks};
use super::player::get_player_for_user;

/// How long a request waits for a script; the sandbox's own limits normally stop it sooner.
pub(crate) const SCRIPT_TIMEOUT: Duration = Duration::from_secs(2);

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
    Picked(Box<Pick>),
}

/// A picked deck and how it was weighed.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    /// The deck, with the weight it was drawn with.
    pub candidate: Candidate,
    /// Its share of the total weight.
    pub chance: f64,
    /// The script's note for it.
    pub note: Option<String>,
    /// Why the member's script was not used, when it failed.
    pub script_error: Option<ScriptError>,
}

/// Where the weights came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightSource {
    /// The built-in formula (no script, or the script failed).
    BuiltIn,
    /// The member's script.
    Script,
}

impl WeightSource {
    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::Script => "script",
        }
    }
}

/// The weights a pick draws from, one per candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Scored {
    /// One per candidate, in order.
    pub weights: Vec<Weighted>,
    /// Where they came from.
    pub source: WeightSource,
    /// Why the script was not used.
    pub error: Option<ScriptError>,
    /// The script's `print` output.
    pub logs: Vec<String>,
}

impl Scored {
    /// Each weight's share of the total (zero when every weight is zero).
    pub fn chances(&self) -> Vec<f64> {
        let total: f64 = self.weights.iter().map(|weighted| weighted.weight).sum();
        self.weights
            .iter()
            .map(|weighted| {
                if total > 0.0 {
                    weighted.weight / total
                } else {
                    0.0
                }
            })
            .collect()
    }
}

/// Everything a pick or preview weighs: the member's player, playable decks with built-in
/// weights, and the script context.
#[derive(Clone, Debug)]
pub struct Chooser {
    /// The member's player.
    pub player: Player,
    /// Playable decks, with built-in weights.
    pub candidates: Vec<Candidate>,
    /// The `ctx` table; `ctx.decks` holds one `deck` table per candidate.
    pub context: Value,
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
    (now.unix().saturating_sub(played_at.unix()) / 3600)
        .saturating_add(1)
        .max(1)
}

fn float(value: i64) -> f64 {
    i32::try_from(value).map_or(f64::from(i32::MAX), f64::from)
}

/// Recency in hours × (skips + 1) ÷ (plays + 1); never-played decks
/// count as a month older than the oldest played one.
pub fn selection_weights(candidates: Vec<Candidate>, now: UtcDateTime) -> Vec<Candidate> {
    let oldest = candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .last_played_at
                .map(|played_at| recency_hours(now, played_at))
        })
        .max()
        .unwrap_or(0);
    let unplayed = oldest
        .saturating_add(UNPLAYED_BOOST_HOURS)
        .max(UNPLAYED_BOOST_HOURS);
    candidates
        .into_iter()
        .map(|candidate| {
            let recency = candidate
                .last_played_at
                .map_or(unplayed, |played_at| recency_hours(now, played_at));
            let weight = float(recency) * float(candidate.deck.skip_count.saturating_add(1))
                / float(candidate.play_count.saturating_add(1));
            Candidate {
                weight,
                ..candidate
            }
        })
        .collect()
}

/// Draws one candidate in proportion to `weights`; zero-weight candidates are never drawn.
fn weighted_pick(weights: &[Weighted], random: f64) -> Option<usize> {
    let total: f64 = weights.iter().map(|weighted| weighted.weight).sum();
    let threshold = random.clamp(0.0, 1.0) * total;
    let mut cumulative = 0.0;
    let mut last = None;
    for (index, weighted) in weights.iter().enumerate() {
        if weighted.weight <= 0.0 {
            continue;
        }
        cumulative += weighted.weight;
        if cumulative >= threshold {
            return Some(index);
        }
        last = Some(index);
    }
    last
}

async fn playable_decks(
    conn: &mut SqliteConnection,
    player_id: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
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
    let decks = select_decks!("WHERE player_id = ?", player_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let deck = decks.iter().find(|deck| deck.id == row.id)?.clone();
            Some(Candidate {
                deck,
                play_count: row.play_count,
                last_played_at: row.last_played_at,
                weight: 0.0,
            })
        })
        .collect())
}

/// The member's playable decks and script context, or `None` without a linked player. The
/// deck `exclude_id` is left out when other candidates remain.
pub async fn load_chooser(
    conn: &mut SqliteConnection,
    user_id: i64,
    exclude_id: Option<i64>,
    now: UtcDateTime,
    zone: &Zone,
) -> Result<Option<Chooser>, sqlx::Error> {
    let Some(player) = get_player_for_user(conn, user_id).await? else {
        return Ok(None);
    };
    let mut candidates = playable_decks(conn, player.id).await?;
    let mut excluded = None;
    if candidates.len() > 1
        && let Some(id) = exclude_id
    {
        candidates.retain(|candidate| candidate.deck.id != id);
        excluded = Some(id);
    }
    let candidates = selection_weights(candidates, now);
    let history = chooser_context::load_history(conn, player.id).await?;
    let context = chooser_context::build(&player, &candidates, &history, now, zone, excluded);
    Ok(Some(Chooser {
        player,
        candidates,
        context,
    }))
}

/// Weighs the candidates with `script`, or with the built-in weights when there is no
/// script or it fails (including when it leaves every deck out).
pub async fn score(chooser: &Chooser, script: Option<String>) -> Scored {
    let built_in = |error: Option<ScriptError>, logs: Vec<String>| Scored {
        weights: chooser
            .candidates
            .iter()
            .map(|candidate| Weighted {
                weight: candidate.weight,
                note: None,
            })
            .collect(),
        source: WeightSource::BuiltIn,
        error,
        logs,
    };
    let Some(script) = script else {
        return built_in(None, Vec::new());
    };
    let context = chooser.context.clone();
    let task = tokio::task::spawn_blocking(move || deck_script::run(&script, &context));
    let run = match tokio::time::timeout(SCRIPT_TIMEOUT, task).await {
        Ok(Ok(run)) => run,
        Ok(Err(_)) => return built_in(Some(ScriptError::new("the script crashed")), Vec::new()),
        Err(_) => {
            return built_in(
                Some(ScriptError::new("the script did not finish in time")),
                Vec::new(),
            );
        }
    };
    match run.result {
        Ok(weights)
            if !weights.is_empty() && weights.iter().all(|weighted| weighted.weight <= 0.0) =>
        {
            built_in(
                Some(ScriptError::new("weight() returned 0 for every deck")),
                run.logs,
            )
        }
        Ok(weights) => Scored {
            weights,
            source: WeightSource::Script,
            error: None,
            logs: run.logs,
        },
        Err(error) => built_in(Some(error), run.logs),
    }
}

/// Picks one of the chooser's decks with `scored` weights; `random` is a uniform draw in
/// `[0, 1]`.
pub fn pick(chooser: Chooser, scored: Scored, random: f64) -> DeckPick {
    let chances = scored.chances();
    let Some(index) = weighted_pick(&scored.weights, random) else {
        return DeckPick::NoEligibleDecks;
    };
    let (Some(candidate), Some(weighted)) = (
        chooser.candidates.into_iter().nth(index),
        scored.weights.into_iter().nth(index),
    ) else {
        return DeckPick::NoEligibleDecks;
    };
    DeckPick::Picked(Box::new(Pick {
        candidate: Candidate {
            weight: weighted.weight,
            ..candidate
        },
        chance: chances.get(index).copied().unwrap_or_default(),
        note: weighted.note,
        script_error: scored.error,
    }))
}

/// The member's saved script.
pub async fn get_script(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Option<String>, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        "SELECT deck_chooser_script FROM users WHERE id = ?",
        user_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .flatten())
}

/// Saves (or with `None`, clears) the member's script; validate it with
/// [`deck_script::check`] first.
pub async fn set_script(
    conn: &mut SqliteConnection,
    user_id: i64,
    script: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET deck_chooser_script = ? WHERE id = ?",
        script,
        user_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Only the member's own, unretired decks.
pub async fn record_outcome(
    conn: &mut SqliteConnection,
    user_id: i64,
    deck_id: i64,
    outcome: Outcome,
) -> Result<Deck, GamesError> {
    let player = get_player_for_user(conn, user_id)
        .await?
        .ok_or(GamesError::NotFound)?;
    let deck = get_deck(conn, deck_id)
        .await?
        .filter(|deck| deck.player_id == player.id)
        .ok_or(GamesError::NotFound)?;
    if deck.archived_at.is_some() {
        return Err(GamesError::BadRequest);
    }
    match outcome {
        Outcome::Played => {
            sqlx::query!("UPDATE decks SET skip_count = 0 WHERE id = ?", deck.id)
                .execute(&mut *conn)
                .await?
        }
        Outcome::Skipped => {
            sqlx::query!(
                "UPDATE decks SET skip_count = skip_count + 1 WHERE id = ?",
                deck.id
            )
            .execute(&mut *conn)
            .await?
        }
    };
    get_deck(conn, deck.id).await?.ok_or(GamesError::NotFound)
}
