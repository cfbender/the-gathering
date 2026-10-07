//! Folding a member's hosted decks (Moxfield, Archidekt, ManaVault) into their player's
//! local deck list (`TheGathering.Games.SyncRemoteDecks`).
//!
//! Each remote deck is matched against the player's decks in order:
//!
//! 1. the same `decklist_url`: an already-linked deck, refreshed from the host;
//! 2. the same name (case-insensitive): linked and refreshed;
//! 3. the same commander pair, when that local deck has no link yet: linked and filled
//!    in, but its local name is kept.
//!
//! Anything unmatched becomes a new deck. Hosts that failed to list are skipped and
//! reported in `errors`; their decks are left alone rather than guessed at.

use std::collections::HashMap;

use lotus::decklist::Source;
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use crate::accounts::User;
use crate::catalog::{self, CardSummaries};
use crate::db;
use crate::decklists::remote_decks::RemoteDeck;
use crate::state::AppState;

use super::model::{Deck, select_decks};
use super::{GamesError, deck, fold_name, player};

/// A host that could not be listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceError {
    /// The host.
    pub source: Source,
    /// Why.
    pub error: String,
}

/// The sync's counts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncResult {
    /// Decks created.
    pub created: i64,
    /// Decks linked or refreshed.
    pub updated: i64,
    /// Hosts skipped.
    pub errors: Vec<SourceError>,
}

/// `SyncRemoteDecks.configured?/1`: a deck host is set up.
pub fn configured(user: &User) -> bool {
    let present = |value: &Option<String>| value.as_deref().is_some_and(|value| !value.is_empty());
    present(&user.moxfield_username)
        || present(&user.archidekt_username)
        || (present(&user.manavault_url) && present(&user.manavault_api_key))
}

/// `SyncRemoteDecks.run/1`: a bad request for members with no linked player or no deck
/// host configured.
pub async fn run(state: &AppState, user: &User) -> Result<SyncResult, GamesError> {
    let player = player::get_player_for_user(&mut *state.pool.acquire().await?, user.id)
        .await?
        .ok_or(GamesError::BadRequest)?;
    if !configured(user) {
        return Err(GamesError::BadRequest);
    }
    let remote = state.decklists.remote.list(user).await;
    let failed: Vec<SourceError> = remote
        .sources
        .iter()
        .filter_map(|source| {
            source.error.as_ref().map(|error| SourceError {
                source: source.source,
                error: error.clone(),
            })
        })
        .collect();
    let decks: Vec<&RemoteDeck> = remote
        .decks
        .iter()
        .filter(|deck| !failed.iter().any(|failure| failure.source == deck.source))
        .collect();
    let refs: Vec<(Option<String>, Option<String>)> = decks
        .iter()
        .flat_map(|deck| {
            deck.commanders
                .iter()
                .map(|name| (None, Some(name.clone())))
        })
        .collect();
    let summaries = catalog::card_summaries_in(&mut *state.pool.acquire().await?, &refs).await?;
    let mut tx = db::begin(&state.pool).await?;
    let (created, updated) = sync(&mut tx, player.id, &decks, &summaries).await?;
    tx.commit().await?;
    Ok(SyncResult {
        created,
        updated,
        errors: failed,
    })
}

/// Order-insensitive so "Thrasios + Tymna" and "Tymna + Thrasios" are one deck.
fn commander_key(commander: Option<&str>, partner: Option<&str>) -> Vec<String> {
    let mut key: Vec<String> = [commander, partner]
        .into_iter()
        .flatten()
        .filter(|name| !name.is_empty())
        .map(fold_name)
        .collect();
    key.sort();
    key
}

struct State {
    by_url: HashMap<String, Deck>,
    by_name: HashMap<String, Deck>,
    by_commanders: HashMap<Vec<String>, Deck>,
    created: i64,
    updated: i64,
}

impl State {
    fn remember(&mut self, deck: Deck, created: bool) {
        if created {
            self.created += 1;
        } else {
            self.updated += 1;
        }
        // Now linked, so it must not absorb a second remote deck with the same commander.
        self.by_commanders
            .retain(|_, candidate| candidate.id != deck.id);
        if let Some(url) = &deck.decklist_url {
            self.by_url.insert(url.clone(), deck.clone());
        }
        self.by_name.insert(fold_name(&deck.name), deck);
    }
}

fn commander_names(remote: &RemoteDeck) -> (Option<&str>, Option<&str>) {
    let mut names = remote.commanders.iter().map(String::as_str);
    (names.next(), names.next())
}

fn deck_attrs(
    remote: &RemoteDeck,
    player_id: i64,
    summaries: &CardSummaries,
) -> Map<String, Value> {
    let (commander_name, partner_name) = commander_names(remote);
    let commander = commander_name.and_then(|name| summaries.get(None, Some(name)));
    let partner = partner_name.and_then(|name| summaries.get(None, Some(name)));
    let Value::Object(attrs) = json!({
        "player_id": player_id,
        "name": remote.name,
        "commander_card_id": commander.map(|card| &card.id),
        "commander_name": commander.map(|card| card.name.as_str()).or(commander_name),
        "partner_card_id": partner.map(|card| &card.id),
        "partner_name": partner.map(|card| card.name.as_str()).or(partner_name),
        "color_identity": remote.color_identity.concat(),
        "decklist_url": remote.url,
    }) else {
        return Map::new();
    };
    attrs
}

/// Links an independently created deck: points it at the host and fills in what the owner
/// never recorded, without renaming what they call it.
fn link_attrs(deck: &Deck, attrs: &Map<String, Value>) -> Value {
    let mut linked = Map::new();
    if let Some(url) = attrs.get("decklist_url") {
        linked.insert("decklist_url".into(), url.clone());
    }
    let mut fill = |key: &str, current_missing: bool| {
        if current_missing && let Some(value) = attrs.get(key).filter(|value| !value.is_null()) {
            linked.insert(key.to_owned(), value.clone());
        }
    };
    fill("commander_card_id", deck.commander_card_id.is_none());
    fill("partner_card_id", deck.partner_card_id.is_none());
    fill("color_identity", deck.color_identity.is_empty());
    Value::Object(linked)
}

async fn sync(
    conn: &mut SqliteConnection,
    player_id: i64,
    remote_decks: &[&RemoteDeck],
    summaries: &CardSummaries,
) -> Result<(i64, i64), GamesError> {
    let existing = select_decks!("WHERE player_id = ? ORDER BY id", player_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut state = State {
        by_url: existing
            .iter()
            .filter_map(|deck| deck.decklist_url.clone().map(|url| (url, deck.clone())))
            .collect(),
        by_name: existing
            .iter()
            .map(|deck| (fold_name(&deck.name), deck.clone()))
            .collect(),
        // Unlinked decks only: a deck that already points somewhere is never re-pointed by a
        // commander coincidence. Earliest deck wins when several share a commander.
        by_commanders: existing
            .iter()
            .rev()
            .filter(|deck| deck.decklist_url.is_none())
            .map(|deck| {
                (
                    commander_key(Some(&deck.commander_name), deck.partner_name.as_deref()),
                    deck.clone(),
                )
            })
            .collect(),
        created: 0,
        updated: 0,
    };
    for remote in remote_decks {
        let attrs = deck_attrs(remote, player_id, summaries);
        let (commander, partner) = commander_names(remote);
        let by_name = remote.name.as_deref().map(fold_name);
        let saved = if let Some(found) = state.by_url.get(&remote.url).cloned() {
            (
                deck::update_deck(conn, &found, &Value::Object(attrs)).await?,
                false,
            )
        } else if let Some(found) = by_name.and_then(|name| state.by_name.get(&name).cloned()) {
            (
                deck::update_deck(conn, &found, &Value::Object(attrs)).await?,
                false,
            )
        } else if let Some(found) = state
            .by_commanders
            .get(&commander_key(commander, partner))
            .cloned()
        {
            let linked = link_attrs(&found, &attrs);
            (deck::update_deck(conn, &found, &linked).await?, false)
        } else {
            (deck::create_deck(conn, &Value::Object(attrs)).await?, true)
        };
        state.remember(saved.0, saved.1);
    }
    Ok((state.created, state.updated))
}
