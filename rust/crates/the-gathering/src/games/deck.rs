//! Decks: `Deck.changeset/2`, `Deck.update_changeset/2` (with `DeckPrintings.validate/1`),
//! lookups by name or commander pairing, and `DeleteDeck`.

use serde_json::Value;
use sqlx::{Connection, SqliteConnection};

use crate::catalog;
use crate::changeset::{Change, Changeset, TAKEN};
use crate::db::{self, UtcDateTime};
use crate::error::Errors;

use super::color_identity;
use super::model::{Deck, DecklistSource, GameFormat, GameResult, Player, select_decks};
use super::player::SeatGame;
use super::{GamesError, fold_name};

const COLOR_MESSAGE: &str = "must contain each of W, U, B, R, and G at most once";
const PRINTING_MESSAGE: &str = "must be a printing of the selected card";

/// The writable columns of a deck.
#[derive(Clone, Debug, PartialEq)]
struct Fields {
    player_id: Option<i64>,
    name: Option<String>,
    commander_card_id: Option<String>,
    commander_name: Option<String>,
    commander_printing_id: Option<String>,
    partner_card_id: Option<String>,
    partner_name: Option<String>,
    partner_printing_id: Option<String>,
    color_identity: Option<String>,
    decklist_url: Option<String>,
    archived_at: Option<UtcDateTime>,
    included_for_play: bool,
}

impl Fields {
    fn of(deck: &Deck) -> Self {
        Self {
            player_id: Some(deck.player_id),
            name: Some(deck.name.clone()),
            commander_card_id: deck.commander_card_id.clone(),
            commander_name: Some(deck.commander_name.clone()),
            commander_printing_id: deck.commander_printing_id.clone(),
            partner_card_id: deck.partner_card_id.clone(),
            partner_name: deck.partner_name.clone(),
            partner_printing_id: deck.partner_printing_id.clone(),
            color_identity: Some(deck.color_identity.clone()),
            decklist_url: deck.decklist_url.clone(),
            archived_at: deck.archived_at,
            included_for_play: deck.included_for_play,
        }
    }

    fn new() -> Self {
        Self {
            player_id: None,
            name: None,
            commander_card_id: None,
            commander_name: None,
            commander_printing_id: None,
            partner_card_id: None,
            partner_name: None,
            partner_printing_id: None,
            color_identity: Some(String::new()),
            decklist_url: None,
            archived_at: None,
            included_for_play: true,
        }
    }
}

/// Whether `identity` is unique WUBRG letters (`~r/^(?!.*(.).*\1)[WUBRG]*$/`).
fn valid_identity(identity: &str) -> bool {
    let mut seen = String::new();
    identity.chars().all(|letter| {
        let fresh = "WUBRG".contains(letter) && !seen.contains(letter);
        seen.push(letter);
        fresh
    })
}

/// `DeckPrintings.validate_printing/4`: the printing must be of the card the slot names.
async fn printing_matches(
    conn: &mut SqliteConnection,
    id: Option<&str>,
    name: Option<&str>,
    printing_id: &str,
) -> Result<bool, sqlx::Error> {
    let Some(name) = name else { return Ok(false) };
    let Some(card) = catalog::resolve_card_in(conn, id, Some(name)).await? else {
        return Ok(false);
    };
    let by_name = catalog::find_card_by_name_in(conn, name).await?;
    if by_name.is_none_or(|by_name| by_name.oracle_id != card.oracle_id) {
        return Ok(false);
    }
    Ok(catalog::get_printing_in(conn, printing_id)
        .await?
        .is_some_and(|printing| printing.oracle_id == card.oracle_id))
}

/// Casts and validates `attrs` onto `current` (or a new deck when `None`).
async fn changeset(
    conn: &mut SqliteConnection,
    current: Option<&Deck>,
    attrs: &Value,
) -> Result<Fields, GamesError> {
    let base = current.map_or_else(Fields::new, Fields::of);
    let mut cs = Changeset::new(attrs);
    let mut fields = base.clone();
    if current.is_none() {
        fields.player_id = cs.integer("player_id").or(base.player_id);
    }
    fields.name = cs
        .string("name")
        .map(|name| name.trim().to_owned())
        .or(base.name.clone());
    fields.commander_card_id = cs
        .string("commander_card_id")
        .or(base.commander_card_id.clone());
    fields.commander_name = cs
        .string("commander_name")
        .map(|name| name.trim().to_owned())
        .or(base.commander_name.clone());
    fields.commander_printing_id = cs
        .string("commander_printing_id")
        .or(base.commander_printing_id.clone());
    fields.partner_card_id = cs
        .string("partner_card_id")
        .or(base.partner_card_id.clone());
    fields.partner_name = cs.string("partner_name").or(base.partner_name.clone());
    fields.partner_printing_id = cs
        .string("partner_printing_id")
        .or(base.partner_printing_id.clone());
    fields.color_identity = cs.string("color_identity").or(base.color_identity.clone());
    fields.decklist_url = cs.string("decklist_url").or(base.decklist_url.clone());
    fields.archived_at = cs.datetime("archived_at").or(base.archived_at);
    // `included_for_play: null` would insert NULL into a NOT NULL column (a raise in
    // Elixir); it keeps the current value instead.
    if let Change::Set(Some(included)) = cs.boolean("included_for_play") {
        fields.included_for_play = included;
    }

    // DeckPrintings.validate/1
    for commander in [true, false] {
        let printing_field = if commander {
            "commander_printing_id"
        } else {
            "partner_printing_id"
        };
        let (id, name, printing, base_id, base_name, base_printing) = if commander {
            (
                fields.commander_card_id.clone(),
                fields.commander_name.clone(),
                fields.commander_printing_id.clone(),
                &base.commander_card_id,
                &base.commander_name,
                &base.commander_printing_id,
            )
        } else {
            (
                fields.partner_card_id.clone(),
                fields.partner_name.clone(),
                fields.partner_printing_id.clone(),
                &base.partner_card_id,
                &base.partner_name,
                &base.partner_printing_id,
            )
        };
        let identity_changed = &id != base_id || &name != base_name;
        let supplied = cs.raw(printing_field).is_some();
        if identity_changed && !supplied {
            if commander {
                fields.commander_printing_id = None;
            } else {
                fields.partner_printing_id = None;
            }
        } else if (identity_changed || &printing != base_printing)
            && let Some(printing_id) = printing.as_deref()
            && !printing_matches(conn, id.as_deref(), name.as_deref(), printing_id).await?
        {
            cs.add_error(printing_field, PRINTING_MESSAGE);
        }
    }

    cs.required_value("player_id", fields.player_id.as_ref());
    cs.required("name", fields.name.as_ref());
    cs.required("commander_name", fields.commander_name.as_ref());
    cs.length("name", fields.name.as_deref(), Some(1), Some(100));

    // validate_color_identity/1
    let identity = fields.color_identity.clone().unwrap_or_default();
    if valid_identity(&identity) {
        let identity_changed = fields.color_identity != base.color_identity
            || fields.commander_card_id != base.commander_card_id
            || fields.commander_name != base.commander_name
            || fields.partner_card_id != base.partner_card_id
            || fields.partner_name != base.partner_name;
        if identity_changed {
            let refs: Vec<(Option<String>, Option<String>)> = [
                (
                    fields.commander_card_id.clone(),
                    fields.commander_name.clone(),
                ),
                (fields.partner_card_id.clone(), fields.partner_name.clone()),
            ]
            .into_iter()
            .filter(|(id, name)| id.is_some() || name.is_some())
            .collect();
            let summaries = catalog::card_summaries_in(conn, &refs).await?;
            let commander_colors: String = refs
                .iter()
                .filter_map(|(id, name)| summaries.get(id.as_deref(), name.as_deref()))
                .map(|summary| summary.color_identity.as_str())
                .collect();
            fields.color_identity = Some(color_identity::canonical(&format!(
                "{identity}{commander_colors}"
            )));
        }
    } else {
        cs.add_error("color_identity", COLOR_MESSAGE);
    }

    // assoc_constraint(:player); SQLite foreign-key errors carry no name, so check first.
    if current.is_none()
        && cs.is_valid()
        && let Some(player_id) = fields.player_id
        && super::model::get_player(conn, player_id).await?.is_none()
    {
        cs.add_error("player", "does not exist");
    }
    cs.finish()?;
    Ok(fields)
}

fn decklist_source(fields: &Fields) -> Option<DecklistSource> {
    fields
        .decklist_url
        .as_deref()
        .filter(|url| !url.is_empty())
        .map(DecklistSource::of_url)
}

fn unique_error(error: sqlx::Error) -> GamesError {
    if db::is_unique_violation(&error, &[]) {
        GamesError::Invalid(Errors::single("name", TAKEN))
    } else {
        GamesError::Database(error)
    }
}

/// `Games.create_deck/1`: casts `player_id`, names, commander/partner cards and printings,
/// `color_identity`, `decklist_url`, `archived_at`, and `included_for_play`.
pub async fn create_deck(conn: &mut SqliteConnection, attrs: &Value) -> Result<Deck, GamesError> {
    let fields = changeset(conn, None, attrs).await?;
    let source = decklist_source(&fields);
    let now = UtcDateTime::now();
    let id = sqlx::query_scalar!(
        r#"INSERT INTO decks (player_id, name, commander_card_id, commander_name, commander_printing_id,
                              partner_card_id, partner_name, partner_printing_id, color_identity, decklist_url,
                              decklist_source, archived_at, skip_count, included_for_play, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?) RETURNING id AS "id!: i64""#,
        fields.player_id,
        fields.name,
        fields.commander_card_id,
        fields.commander_name,
        fields.commander_printing_id,
        fields.partner_card_id,
        fields.partner_name,
        fields.partner_printing_id,
        fields.color_identity,
        fields.decklist_url,
        source,
        fields.archived_at,
        fields.included_for_play,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(unique_error)?;
    super::model::get_deck(conn, id)
        .await?
        .ok_or(GamesError::NotFound)
}

/// `Games.update_deck/2`: the same fields as [`create_deck`] except the owner.
pub async fn update_deck(
    conn: &mut SqliteConnection,
    deck: &Deck,
    attrs: &Value,
) -> Result<Deck, GamesError> {
    let fields = changeset(conn, Some(deck), attrs).await?;
    let source = decklist_source(&fields);
    if fields == Fields::of(deck) && source == deck.decklist_source {
        return Ok(deck.clone());
    }
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE decks SET name = ?, commander_card_id = ?, commander_name = ?, commander_printing_id = ?,
                          partner_card_id = ?, partner_name = ?, partner_printing_id = ?, color_identity = ?,
                          decklist_url = ?, decklist_source = ?, archived_at = ?, included_for_play = ?, updated_at = ?
         WHERE id = ?",
        fields.name,
        fields.commander_card_id,
        fields.commander_name,
        fields.commander_printing_id,
        fields.partner_card_id,
        fields.partner_name,
        fields.partner_printing_id,
        fields.color_identity,
        fields.decklist_url,
        source,
        fields.archived_at,
        fields.included_for_play,
        now,
        deck.id
    )
    .execute(&mut *conn)
    .await
    .map_err(unique_error)?;
    super::model::get_deck(conn, deck.id)
        .await?
        .ok_or(GamesError::NotFound)
}

/// `Games.list_decks/1`: by case-folded name, with each deck's player.
pub async fn list_decks(
    conn: &mut SqliteConnection,
    include_archived: bool,
    player_id: Option<i64>,
) -> Result<Vec<(Deck, Player)>, sqlx::Error> {
    let decks = select_decks!(
        "WHERE (? OR archived_at IS NULL) AND (? IS NULL OR player_id = ?) ORDER BY lower(name)",
        include_archived,
        player_id,
        player_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let players = super::player::list_players(conn, true).await?;
    Ok(decks
        .into_iter()
        .filter_map(|deck| {
            let player = players
                .iter()
                .find(|player| player.id == deck.player_id)?
                .clone();
            Some((
                deck,
                Player {
                    avatar_url: None,
                    ..player
                },
            ))
        })
        .collect())
}

/// The deck's seats with their games, newest first (`Games.get_deck!/1` preloads only the
/// game, so each seat's `deck` stays `None`).
pub async fn deck_seat_games(
    conn: &mut SqliteConnection,
    deck_id: i64,
) -> Result<Vec<SeatGame>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT s.game_id, s.result AS "result: GameResult", g.played_at AS "played_at: UtcDateTime",
                  g.format AS "format: GameFormat"
           FROM game_players s JOIN games g ON g.id = s.game_id
           WHERE s.deck_id = ?
           ORDER BY g.played_at DESC"#,
        deck_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|row| SeatGame { game_id: row.game_id, played_at: row.played_at, format: row.format, result: row.result, deck: None })
    .collect())
}

/// `Games.find_deck/4`: a player's deck by case-folded name, else by commander pairing
/// (order-insensitive across commander and partner; the earliest deck wins).
pub async fn find_deck(
    conn: &mut SqliteConnection,
    player_id: i64,
    name: &str,
    commander_name: Option<&str>,
    partner_name: Option<&str>,
) -> Result<Option<Deck>, sqlx::Error> {
    if let Some(deck) = deck_by_name(conn, player_id, name).await? {
        return Ok(Some(deck));
    }
    let key = commander_key(commander_name, partner_name);
    if key.is_empty() {
        return Ok(None);
    }
    let decks = select_decks!("WHERE player_id = ? ORDER BY id", player_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(decks.into_iter().find(|deck| {
        commander_key(Some(&deck.commander_name), deck.partner_name.as_deref()) == key
    }))
}

async fn deck_by_name(
    conn: &mut SqliteConnection,
    player_id: i64,
    name: &str,
) -> Result<Option<Deck>, sqlx::Error> {
    let folded = fold_name(name);
    select_decks!("WHERE player_id = ? AND lower(name) = ?", player_id, folded)
        .fetch_optional(&mut *conn)
        .await
}

fn commander_key(commander_name: Option<&str>, partner_name: Option<&str>) -> Vec<String> {
    let mut key: Vec<String> = [commander_name, partner_name]
        .into_iter()
        .flatten()
        .filter(|name| !name.is_empty())
        .map(fold_name)
        .collect();
    key.sort();
    key
}

/// `Games.find_or_create_deck/3`: `attrs` (an object) is merged with `player_id` and `name`
/// when creating; its `commander_name`/`partner_name` drive the pairing fallback. A
/// concurrent insert of the same name returns that deck.
pub async fn find_or_create_deck(
    conn: &mut SqliteConnection,
    player_id: i64,
    name: &str,
    attrs: &Value,
) -> Result<Deck, GamesError> {
    let commander = attrs.get("commander_name").and_then(Value::as_str);
    let partner = attrs.get("partner_name").and_then(Value::as_str);
    if let Some(deck) = find_deck(conn, player_id, name, commander, partner).await? {
        return Ok(deck);
    }
    let mut merged = attrs.as_object().cloned().unwrap_or_default();
    merged.insert("player_id".into(), Value::from(player_id));
    merged.insert("name".into(), Value::String(name.to_owned()));
    match create_deck(conn, &Value::Object(merged)).await {
        Err(GamesError::Invalid(errors)) => match deck_by_name(conn, player_id, name).await? {
            Some(deck) => Ok(deck),
            None => Err(GamesError::Invalid(errors)),
        },
        other => other,
    }
}

/// `DeleteDeck.run/2`: seats that used the deck move to `replacement` (another deck of the
/// same player) or lose their deck. A replacement that is the deck itself or belongs to
/// another player is a bad request.
pub async fn delete_deck(
    conn: &mut SqliteConnection,
    deck: &Deck,
    replacement: Option<&Deck>,
) -> Result<Deck, GamesError> {
    if let Some(replacement) = replacement
        && (replacement.id == deck.id || replacement.player_id != deck.player_id)
    {
        return Err(GamesError::BadRequest);
    }
    let replacement_id = replacement.map(|replacement| replacement.id);
    let mut tx = conn.begin().await?;
    sqlx::query!(
        "UPDATE game_players SET deck_id = ? WHERE deck_id = ?",
        replacement_id,
        deck.id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM decks WHERE id = ?", deck.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(deck.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_and_sources() {
        assert!(valid_identity(""));
        assert!(valid_identity("GB"));
        assert!(!valid_identity("UURG"));
        assert!(!valid_identity("X"));
        assert_eq!(
            DecklistSource::of_url("https://www.moxfield.com/decks/x"),
            DecklistSource::Moxfield
        );
        assert_eq!(
            DecklistSource::of_url("https://archidekt.com/decks/1"),
            DecklistSource::Archidekt
        );
        assert_eq!(
            DecklistSource::of_url("https://me.manavault.app/share/d/x"),
            DecklistSource::Manavault
        );
        assert_eq!(
            DecklistSource::of_url("https://evilmoxfield.com/"),
            DecklistSource::Other
        );
        assert_eq!(
            commander_key(Some("Tymna"), Some(" Thrasios ")),
            vec!["thrasios".to_owned(), "tymna".to_owned()]
        );
    }
}
