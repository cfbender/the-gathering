//! Bounded historical repair linking decks and MVP picks to catalog cards by name
//! (`TheGathering.Catalog.Backfill` over `Games.LinkCatalogCards.repair_batch/2`).
//!
//! Decks whose `commander_name` holds Mythic Track partner notation
//! (`A || B (Partners)`) are split into commander and partner; decks still named after
//! that notation are renamed `A / B`. Commander, partner, and MVP cards are matched with
//! [`Catalog::find_card_by_name`], and blank deck identities are filled from the cards.

use std::sync::LazyLock;

use serde::Serialize;

use super::{Card, Catalog};
use crate::db::{self, Pool, UtcDateTime};
use crate::games::color_identity;
use crate::regex::{Regex, compile};

const DEFAULT_BATCH_SIZE: i64 = 100;
const MAX_BATCH_SIZE: i64 = 500;

static PARTNER_SUFFIX: LazyLock<Regex> = LazyLock::new(|| compile(r"\s*\([^)]*\)\s*$"));

/// What a repair changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Decks whose piped partner notation was split.
    pub decks_split: u64,
    /// Decks whose commander was linked.
    pub decks_linked: u64,
    /// Decks whose blank color identity was filled.
    pub colors_filled: u64,
    /// MVP picks linked.
    pub mvps_linked: u64,
    /// Names no catalog card matched, sorted and unique.
    pub unmatched: Vec<String>,
}

impl Summary {
    fn merge(&mut self, other: Summary) {
        self.decks_split += other.decks_split;
        self.decks_linked += other.decks_linked;
        self.colors_filled += other.colors_filled;
        self.mvps_linked += other.mvps_linked;
        self.unmatched.extend(other.unmatched);
        self.unmatched.sort();
        self.unmatched.dedup();
    }
}

/// Where the next batch starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    /// Last deck id examined.
    pub deck_id: i64,
    /// Last seat id examined.
    pub seat_id: i64,
}

/// A row the repair could not update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// `deck` or `game_player`.
    pub resource: &'static str,
    /// The row id.
    pub id: i64,
    /// The fields that failed validation.
    pub fields: Vec<&'static str>,
}

/// One bounded batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    /// What it changed.
    pub summary: Summary,
    /// Rows it could not update.
    pub conflicts: Vec<Conflict>,
    /// Where the next batch starts.
    pub cursor: Cursor,
    /// Whether every row has been examined.
    pub done: bool,
}

/// `split_partners/1`: `"A || B (Partners)"` is `("A", Some("B"))`.
pub fn split_partners(name: &str) -> (String, Option<String>) {
    match name.split_once("||") {
        Some((commander, partner)) => (
            commander.trim().to_owned(),
            Some(PARTNER_SUFFIX.replace(partner, "").trim().to_owned()),
        ),
        None => (name.trim().to_owned(), None),
    }
}

/// `Backfill.run/0`: repairs every deck and seat, batch by batch (each batch is its own
/// transaction so other writers are not starved).
pub async fn run(pool: &Pool) -> Result<Summary, sqlx::Error> {
    let mut cursor = Cursor::default();
    let mut summary = Summary::default();
    loop {
        let batch = repair_batch(pool, cursor, DEFAULT_BATCH_SIZE).await?;
        summary.merge(batch.summary);
        if batch.done {
            return Ok(summary);
        }
        cursor = batch.cursor;
    }
}

struct DeckRow {
    id: i64,
    name: String,
    commander_name: String,
    partner_name: Option<String>,
    partner_card_id: Option<String>,
    color_identity: String,
    commander_printing_id: Option<String>,
    partner_printing_id: Option<String>,
}

fn valid_identity(value: &str) -> bool {
    let mut seen = String::new();
    value.chars().all(|color| {
        let fresh = "WUBRG".contains(color) && !seen.contains(color);
        seen.push(color);
        fresh
    })
}

/// `repair_batch/2`: links up to `limit` decks without a commander card and `limit` seats
/// with an unlinked MVP name after `cursor`.
pub async fn repair_batch(pool: &Pool, cursor: Cursor, limit: i64) -> Result<Batch, sqlx::Error> {
    let limit = limit.clamp(1, MAX_BATCH_SIZE);
    let catalog = Catalog { pool: pool.clone() };
    let decks = sqlx::query_as!(
        DeckRow,
        r#"SELECT id AS "id!", name, commander_name, partner_name, partner_card_id, color_identity,
                  commander_printing_id, partner_printing_id
           FROM decks WHERE id > ? AND commander_card_id IS NULL ORDER BY id LIMIT ?"#,
        cursor.deck_id,
        limit
    )
    .fetch_all(pool)
    .await?;
    let seats = sqlx::query!(
        r#"SELECT id AS "id!", mvp_card_name AS "mvp_card_name!" FROM game_players
           WHERE id > ? AND mvp_card_id IS NULL AND mvp_card_name IS NOT NULL ORDER BY id LIMIT ?"#,
        cursor.seat_id,
        limit
    )
    .fetch_all(pool)
    .await?;

    // Card lookups go through the pool, so resolve everything before the write transaction.
    let mut plans = Vec::with_capacity(decks.len());
    for deck in &decks {
        let (commander_name, split_partner) = split_partners(&deck.commander_name);
        let split = split_partner.is_some();
        let partner_name = deck.partner_name.clone().or(split_partner);
        let commander = catalog.find_card_by_name(&commander_name).await?;
        let partner = match &partner_name {
            Some(name) => catalog.find_card_by_name(name).await?,
            None => None,
        };
        let partner_card_id = deck
            .partner_card_id
            .clone()
            .or_else(|| partner.as_ref().map(|card| card.id.clone()));
        let blank = deck.color_identity.is_empty();
        let mut identity = if blank {
            color_identity::canonical(
                &[&commander, &partner]
                    .iter()
                    .filter_map(|card| card.as_ref())
                    .flat_map(|card: &Card| card.color_identity.clone())
                    .collect::<String>(),
            )
        } else {
            deck.color_identity.clone()
        };
        // `Deck.update_changeset` widens the identity to cover every commander card.
        let summaries = catalog
            .card_summaries(&[
                (
                    commander.as_ref().map(|card| card.id.clone()),
                    Some(commander_name.clone()),
                ),
                (partner_card_id.clone(), partner_name.clone()),
            ])
            .await?;
        let valid = valid_identity(&identity);
        if valid {
            let extra: String = [
                summaries.get(
                    commander.as_ref().map(|card| card.id.as_str()),
                    Some(&commander_name),
                ),
                summaries.get(partner_card_id.as_deref(), partner_name.as_deref()),
            ]
            .into_iter()
            .flatten()
            .map(|summary| summary.color_identity.clone())
            .collect();
            identity = color_identity::canonical(&format!("{identity}{extra}"));
        }
        plans.push((
            deck,
            commander_name,
            partner_name,
            commander,
            partner,
            partner_card_id,
            identity,
            split,
            blank,
            valid,
        ));
    }
    let mut mvp_cards = Vec::with_capacity(seats.len());
    for seat in &seats {
        mvp_cards.push(catalog.find_card_by_name(&seat.mvp_card_name).await?);
    }

    let mut summary = Summary::default();
    let mut conflicts = Vec::new();
    let mut unmatched = Vec::new();
    let now = UtcDateTime::now();
    let mut tx = db::begin(pool).await?;
    for (
        deck,
        commander_name,
        partner_name,
        commander,
        partner,
        partner_card_id,
        identity,
        split,
        blank,
        valid,
    ) in plans
    {
        if commander.is_none() {
            unmatched.push(commander_name.clone());
        }
        if let (Some(name), None) = (&partner_name, &partner) {
            unmatched.push(name.clone());
        }
        let name = if split && deck.name == deck.commander_name {
            format!(
                "{commander_name} / {}",
                partner_name.clone().unwrap_or_default()
            )
        } else {
            deck.name.clone()
        };
        let commander_card_id = commander.as_ref().map(|card| card.id.clone());
        // `DeckPrintings.validate/1`: a changed card or name clears its saved printing.
        let commander_printing_id =
            if commander_card_id.is_some() || commander_name != deck.commander_name {
                None
            } else {
                deck.commander_printing_id.clone()
            };
        let partner_printing_id =
            if partner_card_id != deck.partner_card_id || partner_name != deck.partner_name {
                None
            } else {
                deck.partner_printing_id.clone()
            };
        let mut fields = Vec::new();
        if !valid {
            fields.push("color_identity");
        }
        let name_length = name.trim().chars().count();
        if name_length == 0 || name_length > 100 {
            fields.push("name");
        }
        if commander_name.is_empty() {
            fields.push("commander_name");
        }
        if fields.is_empty() {
            let name = name.trim().to_owned();
            let updated = sqlx::query!(
                "UPDATE decks SET name = ?, commander_name = ?, partner_name = ?, commander_card_id = ?,
                 partner_card_id = ?, color_identity = ?, commander_printing_id = ?, partner_printing_id = ?,
                 updated_at = ? WHERE id = ?",
                name,
                commander_name,
                partner_name,
                commander_card_id,
                partner_card_id,
                identity,
                commander_printing_id,
                partner_printing_id,
                now,
                deck.id
            )
            .execute(&mut *tx)
            .await;
            match updated {
                Ok(_) => {}
                Err(error) if db::is_unique_violation(&error, &[]) => fields.push("name"),
                Err(error) => return Err(error),
            }
        }
        if fields.is_empty() {
            summary.decks_split += u64::from(split);
            summary.decks_linked += u64::from(commander.is_some());
            summary.colors_filled += u64::from(blank && !identity.is_empty());
        } else {
            conflicts.push(Conflict {
                resource: "deck",
                id: deck.id,
                fields,
            });
        }
    }
    for (seat, card) in seats.iter().zip(mvp_cards) {
        match card {
            None => unmatched.push(seat.mvp_card_name.clone()),
            Some(card) => {
                sqlx::query!(
                    "UPDATE game_players SET mvp_card_id = ?, updated_at = ? WHERE id = ?",
                    card.id,
                    now,
                    seat.id
                )
                .execute(&mut *tx)
                .await?;
                summary.mvps_linked += 1;
            }
        }
    }
    tx.commit().await?;
    unmatched.sort();
    unmatched.dedup();
    summary.unmatched = unmatched;
    let deck_count = i64::try_from(decks.len()).unwrap_or(i64::MAX);
    let seat_count = i64::try_from(seats.len()).unwrap_or(i64::MAX);
    Ok(Batch {
        summary,
        conflicts,
        cursor: Cursor {
            deck_id: decks.last().map_or(cursor.deck_id, |deck| deck.id),
            seat_id: seats.last().map_or(cursor.seat_id, |seat| seat.id),
        },
        done: deck_count < limit && seat_count < limit,
    })
}
