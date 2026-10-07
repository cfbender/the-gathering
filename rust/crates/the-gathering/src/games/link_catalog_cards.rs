//! Linking decks and MVP cards recorded by name to catalog cards, after imports and as a batched repair.

use std::sync::LazyLock;

use serde::Serialize;

use sqlx::{Connection, SqliteConnection};

use crate::catalog::{self, Card};
use crate::regex::{Regex, compile};
use crate::validation::ValidationError;

use super::GamesError;
use super::deck::update_deck;
use super::input::DeckInput;
use super::model::{Deck, DeckLinks, select_decks};

const DEFAULT_BATCH_SIZE: i64 = 100;
const MAX_BATCH_SIZE: i64 = 500;

static TRAILING_PARENTHETICAL: LazyLock<Regex> = LazyLock::new(|| compile(r"\s*\([^)]*\)\s*$"));

/// Counts of what was linked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LinkSummary {
    /// `Commander || Partner` names split into two slots.
    pub decks_split: usize,
    /// Decks whose commander resolved.
    pub decks_linked: usize,
    /// Decks whose empty identity was filled.
    pub colors_filled: usize,
    /// MVP seats linked.
    pub mvps_linked: usize,
    /// Names the catalog does not know, sorted and unique.
    pub unmatched: Vec<String>,
}

impl LinkSummary {
    /// `merge_summaries/2`.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        let mut unmatched: Vec<String> = self
            .unmatched
            .iter()
            .chain(&other.unmatched)
            .cloned()
            .collect();
        unmatched.sort();
        unmatched.dedup();
        Self {
            decks_split: self.decks_split.saturating_add(other.decks_split),
            decks_linked: self.decks_linked.saturating_add(other.decks_linked),
            colors_filled: self.colors_filled.saturating_add(other.colors_filled),
            mvps_linked: self.mvps_linked.saturating_add(other.mvps_linked),
            unmatched,
        }
    }
}

/// Which row could not be updated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResource {
    /// A deck.
    Deck,
    /// A seat.
    GamePlayer,
}

/// A row whose update failed validation.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// The kind of row.
    pub resource: ConflictResource,
    /// Its id.
    pub id: i64,
    /// Why.
    pub errors: ValidationError,
}

/// One link pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkResult {
    /// Counts.
    pub summary: LinkSummary,
    /// Rows that could not be updated.
    pub conflicts: Vec<Conflict>,
}

/// Where a repair batch resumes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Cursor {
    /// Last deck id handled.
    pub deck_id: i64,
    /// Last seat id handled.
    pub seat_id: i64,
}

/// One repair batch.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult {
    /// What this batch linked.
    pub result: LinkResult,
    /// Where the next batch starts.
    pub cursor: Cursor,
    /// Whether nothing is left.
    pub done: bool,
}

/// `split_partners/1`: `"Commander || Partner (note)"` → commander and partner names.
pub fn split_partners(name: &str) -> (String, Option<String>) {
    match name.split_once("||") {
        Some((commander, partner)) => (
            commander.trim().to_owned(),
            Some(
                TRAILING_PARENTHETICAL
                    .replace(partner, "")
                    .trim()
                    .to_owned(),
            ),
        ),
        None => (name.trim().to_owned(), None),
    }
}

fn identity(cards: &[Option<&Card>]) -> String {
    let colors: Vec<&String> = cards
        .iter()
        .flatten()
        .flat_map(|card| &card.color_identity)
        .collect();
    ["W", "U", "B", "R", "G"]
        .into_iter()
        .filter(|color| colors.iter().any(|c| c.as_str() == *color))
        .collect()
}

struct ItemResult {
    split: bool,
    linked: bool,
    colored: bool,
    unmatched: Vec<String>,
    conflicts: Vec<Conflict>,
}

fn unmatched(pairs: &[(Option<&str>, Option<&Card>)]) -> Vec<String> {
    pairs
        .iter()
        .filter_map(|(name, card)| {
            if card.is_none() {
                name.map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

async fn link_deck(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    deck: &Deck,
) -> Result<ItemResult, GamesError> {
    let (commander_name, split_partner) = split_partners(&deck.commander_name);
    let split = split_partner.is_some();
    let partner_name = deck.partner_name.clone().or(split_partner);
    let commander = catalog::find_card_by_name_in(conn, &commander_name).await?;
    let partner = match &partner_name {
        Some(name) => catalog::find_card_by_name_in(conn, name).await?,
        None => None,
    };
    let blank = deck.color_identity.is_empty();
    let color_identity = if blank {
        identity(&[commander.as_ref(), partner.as_ref()])
    } else {
        deck.color_identity.clone()
    };
    let mut input = DeckInput {
        commander_name: Some(commander_name.clone()).into(),
        partner_name: partner_name.clone().into(),
        commander_card_id: commander.as_ref().map(|card| card.id.clone()).into(),
        partner_card_id: deck
            .partner_card_id
            .clone()
            .or_else(|| partner.as_ref().map(|card| card.id.clone()))
            .into(),
        color_identity: Some(color_identity.clone()).into(),
        ..DeckInput::default()
    };
    if split && deck.name == deck.commander_name {
        input.name = Some(format!(
            "{commander_name} / {}",
            partner_name.clone().unwrap_or_default()
        ))
        .into();
    }
    let names = unmatched(&[
        (Some(commander_name.as_str()), commander.as_ref()),
        (partner_name.as_deref(), partner.as_ref()),
    ]);
    match update_deck(conn, links, deck, &input).await {
        Ok(_) => Ok(ItemResult {
            split,
            linked: commander.is_some(),
            colored: blank && !color_identity.is_empty(),
            unmatched: names,
            conflicts: Vec::new(),
        }),
        Err(GamesError::Invalid(errors)) => Ok(ItemResult {
            split: false,
            linked: false,
            colored: false,
            unmatched: names,
            conflicts: vec![Conflict {
                resource: ConflictResource::Deck,
                id: deck.id,
                errors,
            }],
        }),
        Err(other) => Err(other),
    }
}

async fn link_mvp(
    conn: &mut SqliteConnection,
    seat_id: i64,
    name: &str,
) -> Result<ItemResult, GamesError> {
    let empty = ItemResult {
        split: false,
        linked: false,
        colored: false,
        unmatched: Vec::new(),
        conflicts: Vec::new(),
    };
    match catalog::find_card_by_name_in(conn, name).await? {
        None => Ok(ItemResult {
            unmatched: vec![name.to_owned()],
            ..empty
        }),
        Some(card) => {
            let now = crate::db::UtcDateTime::now();
            sqlx::query!(
                "UPDATE game_players SET mvp_card_id = ?, updated_at = ? WHERE id = ?",
                card.id,
                now,
                seat_id
            )
            .execute(&mut *conn)
            .await?;
            Ok(ItemResult {
                linked: true,
                ..empty
            })
        }
    }
}

async fn link_rows(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    decks: &[Deck],
    seats: &[(i64, Option<String>, Option<String>)],
) -> Result<LinkResult, GamesError> {
    let mut deck_results = Vec::with_capacity(decks.len());
    for deck in decks {
        deck_results.push(link_deck(conn, links, deck).await?);
    }
    let mut mvp_results = Vec::with_capacity(seats.len());
    for (id, mvp_card_id, mvp_card_name) in seats {
        mvp_results.push(match (mvp_card_id, mvp_card_name) {
            (None, Some(name)) => link_mvp(conn, *id, name).await?,
            _ => ItemResult {
                split: false,
                linked: false,
                colored: false,
                unmatched: Vec::new(),
                conflicts: Vec::new(),
            },
        });
    }
    let mut unmatched: Vec<String> = deck_results
        .iter()
        .chain(&mvp_results)
        .flat_map(|result| result.unmatched.clone())
        .collect();
    unmatched.sort();
    unmatched.dedup();
    Ok(LinkResult {
        summary: LinkSummary {
            decks_split: deck_results.iter().filter(|result| result.split).count(),
            decks_linked: deck_results.iter().filter(|result| result.linked).count(),
            colors_filled: deck_results.iter().filter(|result| result.colored).count(),
            mvps_linked: mvp_results.iter().filter(|result| result.linked).count(),
            unmatched,
        },
        conflicts: deck_results
            .into_iter()
            .chain(mvp_results)
            .flat_map(|result| result.conflicts)
            .collect(),
    })
}

/// `link_game/1`: the game's unlinked decks and its seats' MVP cards.
pub async fn link_game(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    game_id: i64,
) -> Result<LinkResult, GamesError> {
    let mut tx = conn.begin().await?;
    let decks = select_decks!(
        "WHERE commander_card_id IS NULL AND id IN (SELECT deck_id FROM game_players WHERE game_id = ?) ORDER BY id",
        game_id
    )
    .fetch_all(&mut *tx)
    .await?;
    let seats: Vec<(i64, Option<String>, Option<String>)> = sqlx::query!(
        r#"SELECT id AS "id!", mvp_card_id, mvp_card_name FROM game_players WHERE game_id = ? ORDER BY id"#,
        game_id
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| (row.id, row.mvp_card_id, row.mvp_card_name))
    .collect();
    let result = link_rows(&mut tx, links, &decks, &seats).await?;
    tx.commit().await?;
    Ok(result)
}

/// `repair_batch/2`: the next `limit` (default 100, at most 500) unlinked decks and
/// unlinked MVP seats after `cursor`.
pub async fn repair_batch(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    cursor: Cursor,
    limit: Option<i64>,
) -> Result<BatchResult, GamesError> {
    let limit = limit.unwrap_or(DEFAULT_BATCH_SIZE).clamp(1, MAX_BATCH_SIZE);
    let mut tx = conn.begin().await?;
    let decks = select_decks!(
        "WHERE id > ? AND commander_card_id IS NULL ORDER BY id LIMIT ?",
        cursor.deck_id,
        limit
    )
    .fetch_all(&mut *tx)
    .await?;
    let seats: Vec<(i64, Option<String>, Option<String>)> = sqlx::query!(
        r#"SELECT id AS "id!", mvp_card_id, mvp_card_name FROM game_players
           WHERE id > ? AND mvp_card_id IS NULL AND mvp_card_name IS NOT NULL ORDER BY id LIMIT ?"#,
        cursor.seat_id,
        limit
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| (row.id, row.mvp_card_id, row.mvp_card_name))
    .collect();
    let result = link_rows(&mut tx, links, &decks, &seats).await?;
    tx.commit().await?;
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    Ok(BatchResult {
        cursor: Cursor {
            deck_id: decks.last().map_or(cursor.deck_id, |deck| deck.id),
            seat_id: seats.last().map_or(cursor.seat_id, |seat| seat.0),
        },
        done: decks.len() < limit && seats.len() < limit,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_partner_names() {
        assert_eq!(
            split_partners("Thrasios || Tymna (partner)"),
            ("Thrasios".into(), Some("Tymna".into()))
        );
        assert_eq!(split_partners(" Krenko "), ("Krenko".into(), None));
    }
}
