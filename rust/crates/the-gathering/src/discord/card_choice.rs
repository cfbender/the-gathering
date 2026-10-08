//! Resolving typed card names for the `/won` form (`CardChoice`, `ResultCommanders`,
//! `ResultDetails`).

use sqlx::SqliteConnection;

use crate::catalog::{self, Card};
use crate::games::{WinCondition, color_identity};

use super::draft::{Candidate, CardChoice, CommanderChoices, Role, WonDraftData};
use super::report::{DeckAttrs, ReportDetails, ReportPlayer};

/// Which cards a name may match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Any card (MVP).
    All,
    /// Commanders.
    Commander,
    /// Second commanders and Backgrounds, including rule-zero pairings.
    Partner,
}

fn search_name(name: &str) -> String {
    lotus::normalize_name(name).replace(['\'', '’', ','], "")
}

async fn matches(
    conn: &mut SqliteConnection,
    name: &str,
    mode: Mode,
) -> Result<Vec<Card>, sqlx::Error> {
    if name.is_empty() {
        return Ok(Vec::new());
    }
    match mode {
        Mode::All => catalog::search_in(conn, name, Some(26), None, false).await,
        Mode::Commander => catalog::search_in(conn, name, Some(26), Some(true), false).await,
        Mode::Partner => {
            let mut cards = catalog::search_in(conn, name, Some(26), Some(true), false).await?;
            for card in catalog::search_in(conn, name, Some(26), None, true).await? {
                if !cards.iter().any(|existing| existing.id == card.id) {
                    cards.push(card);
                }
            }
            Ok(cards)
        }
    }
}

/// Prefers the single match whose whole leading name is the query ("Bello" →
/// "Bello, Bard of the Brambles" over "Bellowing Decoy").
fn prefer_unique_leading_name(matches: Vec<Card>, name: &str) -> Vec<Card> {
    if matches.len() == 1 {
        return matches;
    }
    let prefix = format!("{} ", search_name(name));
    let leading: Vec<&Card> = matches
        .iter()
        .filter(|card| search_name(&card.name).starts_with(&prefix))
        .collect();
    match leading.as_slice() {
        [card] => vec![(*card).clone()],
        _ => matches,
    }
}

/// Resolves a typed card name to one catalog card, or an error to show for none or too many.
pub async fn resolve(
    conn: &mut SqliteConnection,
    name: Option<&str>,
    label: &str,
    mode: Mode,
) -> Result<CardChoice, sqlx::Error> {
    let name = name.unwrap_or_default().trim().to_owned();
    let mut choice = CardChoice {
        name: name.clone(),
        ..CardChoice::default()
    };
    if name.is_empty() {
        return Ok(choice);
    }
    let mut found = matches(conn, &name, mode).await?;
    if let Some(exact) = catalog::find_card_by_name_in(conn, &name).await?
        && found.iter().any(|card| card.id == exact.id)
    {
        found = vec![exact];
    }
    let found = prefer_unique_leading_name(found, &name);
    match found.as_slice() {
        [card] => {
            choice.name.clone_from(&card.name);
            choice.id = Some(card.id.clone());
        }
        [] => {
            choice.error = Some(format!(
                "{label} card not found. Edit the name or leave it blank."
            ));
        }
        cards if cards.len() > 25 => {
            choice.error = Some(format!(
                "Too many {label} matches. Enter a more specific name."
            ));
        }
        cards => {
            choice.candidates = cards
                .iter()
                .map(|card| Candidate {
                    id: card.id.clone(),
                    name: card.name.clone(),
                })
                .collect();
            choice.error = Some(format!("Choose a matching {label} card below."));
        }
    }
    Ok(choice)
}

/// Picks one of the candidates.
pub fn choose(choice: &CardChoice, id: &str) -> Result<CardChoice, String> {
    let card = choice
        .candidates
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| "Select one of the matching cards.".to_owned())?;
    Ok(CardChoice {
        name: card.name.clone(),
        id: Some(card.id.clone()),
        candidates: Vec::new(),
        error: None,
    })
}

/// The chosen card, `None` for a blank name.
pub async fn card(
    conn: &mut SqliteConnection,
    choice: &CardChoice,
    label: &str,
) -> Result<Result<Option<Card>, String>, sqlx::Error> {
    if choice.name.is_empty() {
        return Ok(Ok(None));
    }
    let found = match &choice.id {
        Some(id) => catalog::get_card_in(conn, id).await?,
        None => None,
    };
    Ok(found.map(Some).ok_or_else(|| {
        choice
            .error
            .clone()
            .unwrap_or_else(|| format!("Select a {label} card or leave it blank."))
    }))
}

/// Resolves a player's typed commander and partner.
pub async fn put_commanders(
    conn: &mut SqliteConnection,
    data: &mut WonDraftData,
    player_id: &str,
    commander: Option<&str>,
    partner: Option<&str>,
) -> Result<(), sqlx::Error> {
    let choices = CommanderChoices {
        commander: resolve(conn, commander, "Commander", Mode::Commander).await?,
        partner: resolve(conn, partner, "Partner", Mode::Partner).await?,
    };
    data.commanders.insert(player_id.to_owned(), choices);
    Ok(())
}

/// Picks one of the offered cards for a player's commander or partner.
pub fn choose_commander(
    data: &mut WonDraftData,
    player_id: &str,
    role: Role,
    id: &str,
) -> Result<(), String> {
    let current = data
        .commanders
        .get(player_id)
        .map(|choices| choices.get(role).clone())
        .unwrap_or_default();
    let chosen = choose(&current, id)?;
    *data
        .commanders
        .entry(player_id.to_owned())
        .or_default()
        .get_mut(role) = chosen;
    Ok(())
}

async fn deck(
    conn: &mut SqliteConnection,
    choices: &CommanderChoices,
) -> Result<Result<Option<DeckAttrs>, String>, sqlx::Error> {
    let commander = match card(conn, &choices.commander, "Commander").await? {
        Ok(card) => card,
        Err(error) => return Ok(Err(error)),
    };
    let partner = match card(conn, &choices.partner, "Partner").await? {
        Ok(card) => card,
        Err(error) => return Ok(Err(error)),
    };
    Ok(match (commander, partner) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err("Choose a commander before adding a partner.".into()),
        (Some(commander), Some(partner)) if commander.id == partner.id => {
            Err("Commander and partner must be different cards.".into())
        }
        (Some(commander), partner) => {
            let colors: String = commander
                .color_identity
                .iter()
                .chain(partner.iter().flat_map(|card| card.color_identity.iter()))
                .map(String::as_str)
                .collect();
            Ok(Some(DeckAttrs {
                commander_card_id: commander.id,
                commander_name: commander.name,
                partner_card_id: partner.as_ref().map(|card| card.id.clone()),
                partner_name: partner.map(|card| card.name),
                color_identity: color_identity::canonical(&colors),
            }))
        }
    })
}

/// Resolves the typed MVP.
pub async fn with_mvp(
    conn: &mut SqliteConnection,
    data: &mut WonDraftData,
) -> Result<(), sqlx::Error> {
    let choice = resolve(conn, data.mvp.as_deref(), "MVP", Mode::All).await?;
    data.mvp = Some(choice.name);
    data.mvp_id = choice.id;
    data.mvp_candidates = choice.candidates;
    data.mvp_error = choice.error;
    Ok(())
}

fn number(value: Option<&str>, label: &str, min: i64, max: i64) -> Result<Option<i64>, String> {
    match value {
        None | Some("") => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|number| (min..=max).contains(number))
            .map(Some)
            .ok_or_else(|| {
                format!("{label} must be a whole number from {min} to {max}, or blank if unknown.")
            }),
    }
}

/// The win conditions a reporter may choose (every key but `draw`).
pub fn reportable_conditions() -> impl Iterator<Item = WinCondition> {
    WinCondition::all().filter(|condition| *condition != WinCondition::Draw)
}

/// The details to save, or the first problem.
pub async fn validate(
    conn: &mut SqliteConnection,
    data: &WonDraftData,
    players: &[ReportPlayer],
) -> Result<Result<ReportDetails, String>, sqlx::Error> {
    let complete = data.details_done
        && players
            .iter()
            .all(|player| data.kills.contains_key(&player.discord_id))
        && data
            .winner
            .as_ref()
            .is_some_and(|winner| players.iter().any(|player| &player.discord_id == winner))
        && data
            .win_condition
            .as_deref()
            .is_some_and(|key| reportable_conditions().any(|condition| condition.as_str() == key));
    if !complete {
        return Ok(Err(
            "Complete the game details and every kills page before saving.".into(),
        ));
    }
    let turns = match number(data.turns.as_deref(), "Turns", 1, 10_000) {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    let duration = match number(data.duration.as_deref(), "Duration", 1, 100_000) {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    let mut details = ReportDetails {
        win_condition: data.win_condition.clone(),
        turns,
        duration_minutes: duration,
        notes: data.notes.clone(),
        ..ReportDetails::default()
    };
    for player in players {
        let label = format!("{}'s kills", player.display_name);
        match number(
            data.kills.get(&player.discord_id).map(String::as_str),
            &label,
            0,
            5,
        ) {
            Ok(kills) => {
                details.kills.insert(player.discord_id.clone(), kills);
            }
            Err(error) => return Ok(Err(error)),
        }
    }
    for player in players {
        let Some(choices) = data.commanders.get(&player.discord_id) else {
            continue;
        };
        match deck(conn, choices).await? {
            Ok(attrs) => {
                details.commanders.insert(player.discord_id.clone(), attrs);
            }
            Err(error) => {
                return Ok(Err(format!(
                    "{}: {error} Open Commanders to fix it.",
                    player.display_name
                )));
            }
        }
    }
    if data.mvp.as_deref() != Some("") {
        let found = match &data.mvp_id {
            Some(id) => catalog::get_card_in(conn, id).await?,
            None => None,
        };
        match found {
            Some(card) => {
                details.mvp_card_id = Some(card.id);
                details.mvp_card_name = Some(card.name);
            }
            None => {
                return Ok(Err(data.mvp_error.clone().unwrap_or_else(|| {
                    "Select an MVP card or leave it blank.".into()
                })));
            }
        }
    }
    Ok(Ok(details))
}
