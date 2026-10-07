//! Moving a whole installation's history between servers (`PortableExport`,
//! `PortableImport`, `PortableFile`, `PortableCatalog`).
//!
//! The export holds players, decks, games (by stable `portable_id`), the catalog cards and
//! printings they reference, and sheet receipts, with local ids only as references inside
//! the file; no accounts, Discord identities, or creators. Importing maps every record onto
//! an existing one (players by name, decks by owner and name, games by portable or source
//! identity) or creates it, all in one transaction; the preview runs the same import and
//! rolls it back. No network requests, account linking, or background jobs run.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use crate::catalog;
use crate::changeset::Changeset;
use crate::db::{self, IsoDate, UtcDateTime};
use crate::games::DeckLinks;
use crate::games::model::select_decks;
use crate::games::{Deck, Player, deck, fold_name, load_games, player, record_game};
use crate::state::AppState;
use crate::validation::ValidationError;

use super::ImportError;

const COLLECTIONS: [&str; 6] = [
    "players",
    "decks",
    "games",
    "cards",
    "printings",
    "sheet_receipts",
];
const PLAYER_FIELDS: [&str; 2] = ["name", "archived_at"];
const DECK_FIELDS: [&str; 12] = [
    "name",
    "commander_card_id",
    "commander_name",
    "commander_printing_id",
    "partner_card_id",
    "partner_name",
    "partner_printing_id",
    "color_identity",
    "decklist_url",
    "archived_at",
    "skip_count",
    "included_for_play",
];
/// Elixir exported no `format`, so a Two-Headed Giant game (two winners) failed
/// validation on import; it is exported and restored here.
const GAME_FIELDS: [&str; 9] = [
    "portable_id",
    "played_at",
    "duration_minutes",
    "turns",
    "win_condition",
    "format",
    "notes",
    "source",
    "external_id",
];
const SEAT_FIELDS: [&str; 10] = [
    "player_id",
    "deck_id",
    "seat",
    "result",
    "kills",
    "eliminated_turn",
    "eliminated_by_player_id",
    "mvp_card_id",
    "mvp_card_name",
    "notes",
];
const SOURCES: [&str; 4] = ["manual", "csv", "mythic_track", "discord"];
const INVALID_FILE: &str = "Choose a valid The Gathering JSON export (format version 1). Records need unique IDs and games need 2–6 seats.";

/// Created and reused records of one kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// New records.
    pub created: i64,
    /// Existing records the file mapped onto.
    pub reused: i64,
}

/// What an import does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Players.
    pub players: Counts,
    /// Decks.
    pub decks: Counts,
    /// Games.
    pub games: Counts,
}

// Export

fn deck_json(deck: &Deck) -> Value {
    json!({
        "id": deck.id,
        "player_id": deck.player_id,
        "name": deck.name,
        "commander_card_id": deck.commander_card_id,
        "commander_name": deck.commander_name,
        "commander_printing_id": deck.commander_printing_id,
        "partner_card_id": deck.partner_card_id,
        "partner_name": deck.partner_name,
        "partner_printing_id": deck.partner_printing_id,
        "color_identity": deck.color_identity,
        "decklist_url": deck.decklist_url,
        "archived_at": deck.archived_at,
        "skip_count": deck.skip_count,
        "included_for_play": deck.included_for_play,
    })
}

fn card_json(card: &catalog::Card) -> Value {
    json!({
        "id": card.id,
        "oracle_id": card.oracle_id,
        "name": card.name,
        "normalized_name": card.normalized_name,
        "mana_cost": card.mana_cost,
        "cmc": card.cmc,
        "type_line": card.type_line,
        "oracle_text": card.oracle_text,
        "colors": card.colors,
        "color_identity": card.color_identity,
        "image_uris": card.image_uris,
        "set_code": card.set_code,
        "collector_number": card.collector_number,
        "released_at": card.released_at,
        "layout": card.layout,
        "rarity": card.rarity,
        "game_changer": card.game_changer,
        "commander_legal": card.commander_legal,
        "can_be_commander": card.can_be_commander,
        "commander_pairing": card.commander_pairing,
    })
}

/// `PortableExport.run/0`: one consistent read of everything.
pub async fn export(state: &AppState) -> Result<Value, sqlx::Error> {
    let mut tx = state.pool.begin().await?;
    let conn: &mut SqliteConnection = &mut tx;
    let players = sqlx::query!(
        r#"SELECT id AS "id!", name, archived_at AS "archived_at: UtcDateTime" FROM players ORDER BY id"#
    )
    .fetch_all(&mut *conn)
    .await?;
    let decks = select_decks!("ORDER BY id").fetch_all(&mut *conn).await?;
    let ids: Vec<i64> = sqlx::query_scalar!(r#"SELECT id AS "id!: i64" FROM games ORDER BY id"#)
        .fetch_all(&mut *conn)
        .await?;
    let games = load_games(conn, &ids).await?;
    let portable_ids: HashMap<i64, Option<String>> = games
        .iter()
        .map(|game| (game.id, game.portable_id.clone()))
        .collect();

    let mut references: Vec<(Option<String>, Option<String>)> = Vec::new();
    for deck in &decks {
        references.push((
            deck.commander_card_id.clone(),
            Some(deck.commander_name.clone()),
        ));
        references.push((deck.partner_card_id.clone(), deck.partner_name.clone()));
    }
    for game in &games {
        for seat in &game.seats {
            references.push((seat.mvp_card_id.clone(), seat.mvp_card_name.clone()));
        }
    }
    let mut seen = Vec::new();
    let mut cards: Vec<catalog::Card> = Vec::new();
    for reference in references {
        if seen.contains(&reference) {
            continue;
        }
        if let Some(card) =
            catalog::resolve_card_in(conn, reference.0.as_deref(), reference.1.as_deref()).await?
            && !cards.iter().any(|existing| existing.id == card.id)
        {
            cards.push(card);
        }
        seen.push(reference);
    }
    cards.sort_by(|a, b| a.id.cmp(&b.id));

    let mut printing_ids: Vec<&str> = decks
        .iter()
        .flat_map(|deck| [&deck.commander_printing_id, &deck.partner_printing_id])
        .filter_map(Option::as_deref)
        .collect();
    printing_ids.sort_unstable();
    printing_ids.dedup();
    let mut printings = Vec::new();
    for id in printing_ids {
        if let Some(printing) = catalog::get_printing_in(conn, id).await? {
            printings.push(printing);
        }
    }
    let receipts = sqlx::query!(r#"SELECT key AS "key!", game_id FROM sheet_import_receipts"#)
        .fetch_all(&mut *conn)
        .await?;
    let data = json!({
        "format": "the-gathering",
        "version": 1,
        "exported_at": UtcDateTime::now(),
        "players": players.iter().map(|player| json!({
            "id": player.id, "name": player.name, "archived_at": player.archived_at,
        })).collect::<Vec<_>>(),
        "decks": decks.iter().map(deck_json).collect::<Vec<_>>(),
        "games": games.iter().map(|game| {
            let mut seats: Vec<&crate::games::Seat> = game.seats.iter().collect();
            seats.sort_by_key(|seat| seat.seat);
            json!({
                "portable_id": game.portable_id,
                "played_at": game.played_at,
                "duration_minutes": game.duration_minutes,
                "turns": game.turns,
                "win_condition": game.win_condition,
                "format": game.format,
                "notes": game.notes,
                "source": game.source,
                "external_id": game.external_id,
                "seats": seats.iter().map(|seat| json!({
                    "player_id": seat.player_id,
                    "deck_id": seat.deck_id,
                    "seat": seat.seat,
                    "result": seat.result,
                    "kills": seat.kills,
                    "eliminated_turn": seat.eliminated_turn,
                    "eliminated_by_player_id": seat.eliminated_by_player_id,
                    "mvp_card_id": seat.mvp_card_id,
                    "mvp_card_name": seat.mvp_card_name,
                    "notes": seat.notes,
                })).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "cards": cards.iter().map(card_json).collect::<Vec<_>>(),
        "printings": printings,
        "sheet_receipts": receipts.iter().map(|receipt| json!({
            "key": receipt.key,
            "game_portable_id": portable_ids.get(&receipt.game_id).cloned().flatten(),
        })).collect::<Vec<_>>(),
    });
    tx.commit().await?;
    Ok(data)
}

// File

fn records(value: Option<&Value>) -> Option<&Vec<Value>> {
    value
        .and_then(Value::as_array)
        .filter(|rows| rows.iter().all(Value::is_object))
}

fn local_id(value: &Value) -> Option<i64> {
    value.as_i64().filter(|id| value.is_i64() && *id > 0)
}

/// `Ecto.UUID.cast(id) == {:ok, id}`: lowercase and hyphenated.
fn canonical_uuid(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        id.len() == 36
            && id.char_indices().all(|(index, ch)| match index {
                8 | 13 | 18 | 23 => ch == '-',
                _ => ch.is_ascii_digit() || ('a'..='f').contains(&ch),
            })
    })
}

fn unique_ids(rows: &[Value], key: &str, valid: impl Fn(&Value) -> bool) -> bool {
    let ids: Vec<&Value> = rows
        .iter()
        .map(|row| row.get(key).unwrap_or(&Value::Null))
        .collect();
    ids.iter().all(|id| valid(id))
        && ids
            .iter()
            .enumerate()
            .all(|(index, id)| !ids.iter().take(index).any(|earlier| earlier == id))
}

fn valid_game(game: &Value) -> bool {
    let seats = records(game.get("seats"));
    seats.is_some_and(|seats| (2..=6).contains(&seats.len()))
        && game
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(|source| SOURCES.contains(&source))
        && game
            .get("external_id")
            .is_none_or(|id| id.is_null() || id.is_string())
}

/// `PortableFile.decode/1`.
pub fn decode(json: &str) -> Result<Map<String, Value>, String> {
    let invalid = || INVALID_FILE.to_owned();
    let Ok(Value::Object(data)) = serde_json::from_str::<Value>(json) else {
        return Err(invalid());
    };
    let version_one = data
        .get("version")
        .is_some_and(|version| version.is_i64() && version.as_i64() == Some(1));
    if data.get("format").and_then(Value::as_str) != Some("the-gathering") || !version_one {
        return Err(invalid());
    }
    if !COLLECTIONS
        .iter()
        .all(|collection| records(data.get(*collection)).is_some())
    {
        return Err(invalid());
    }
    let rows = |key: &str| {
        records(data.get(key))
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let valid = unique_ids(rows("players"), "id", |id| local_id(id).is_some())
        && unique_ids(rows("decks"), "id", |id| local_id(id).is_some())
        && unique_ids(rows("games"), "portable_id", canonical_uuid)
        && rows("games").iter().all(valid_game);
    if valid { Ok(data) } else { Err(invalid()) }
}

/// `PortableFile.attrs/2`: only the listed fields.
fn attrs(row: &Value, fields: &[&str]) -> Map<String, Value> {
    fields
        .iter()
        .filter_map(|field| {
            row.get(*field)
                .map(|value| ((*field).to_owned(), value.clone()))
        })
        .collect()
}

// Catalog

fn cast_text(value: Option<&Value>) -> Result<Option<String>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(()),
    }
}

fn cast_bool(value: Option<&Value>) -> Result<Option<bool>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(flag)) => Ok(Some(*flag)),
        Some(Value::String(text)) => match text.as_str() {
            "true" | "1" => Ok(Some(true)),
            "false" | "0" => Ok(Some(false)),
            text if text.trim().is_empty() => Ok(None),
            _ => Err(()),
        },
        Some(_) => Err(()),
    }
}

fn cast_list(value: Option<&Value>) -> Result<Option<Vec<String>>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or(()))
            .collect::<Result<Vec<_>, ()>>()
            .map(Some),
        Some(_) => Err(()),
    }
}

fn cast_map(value: Option<&Value>) -> Result<Option<String>, ()> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(map @ Value::Object(_)) => Ok(Some(map.to_string())),
        Some(_) => Err(()),
    }
}

fn card_data_error() -> ImportError {
    ImportError::Message("Invalid card-art data in export.".to_owned())
}

fn restore_error() -> ImportError {
    ImportError::Message("Could not restore card-art data.".to_owned())
}

async fn restore_card(conn: &mut SqliteConnection, row: &Value) -> Result<(), ImportError> {
    let bad = |()| card_data_error();
    let text = |key: &str| cast_text(row.get(key)).map_err(bad);
    let required = |value: Option<String>| value.ok_or_else(card_data_error);
    let id = required(text("id")?)?;
    let oracle_id = required(text("oracle_id")?)?;
    let name = required(text("name")?)?;
    let normalized_name = required(text("normalized_name")?)?;
    let mana_cost = text("mana_cost")?;
    let cmc = match row.get("cmc") {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => {
            Some(text.trim().parse::<f64>().map_err(|_| card_data_error())?)
        }
        None | Some(Value::Null) => None,
        Some(_) => return Err(card_data_error()),
    }
    .ok_or_else(card_data_error)?;
    let type_line = required(text("type_line")?)?;
    let oracle_text = text("oracle_text")?;
    let colors = cast_list(row.get("colors"))
        .map_err(bad)?
        .ok_or_else(card_data_error)?;
    let color_identity = cast_list(row.get("color_identity"))
        .map_err(bad)?
        .ok_or_else(card_data_error)?;
    let image_uris = required(cast_map(row.get("image_uris")).map_err(bad)?)?;
    let set_code = required(text("set_code")?)?;
    let collector_number = required(text("collector_number")?)?;
    let released_at = match text("released_at")? {
        Some(value) => Some(IsoDate::parse(&value).ok_or_else(card_data_error)?),
        None => None,
    };
    let layout = required(text("layout")?)?;
    let rarity = required(text("rarity")?)?;
    let game_changer = cast_bool(row.get("game_changer"))
        .map_err(bad)?
        .unwrap_or(false);
    let commander_legal = cast_bool(row.get("commander_legal"))
        .map_err(bad)?
        .ok_or_else(card_data_error)?;
    let can_be_commander = cast_bool(row.get("can_be_commander"))
        .map_err(bad)?
        .ok_or_else(card_data_error)?;
    let commander_pairing = text("commander_pairing")?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM cards WHERE id = ? OR oracle_id = ?) AS "e!: bool""#,
        id,
        oracle_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if exists {
        return Ok(());
    }
    let colors = serde_json::to_string(&colors).unwrap_or_else(|_| "[]".into());
    let color_identity = serde_json::to_string(&color_identity).unwrap_or_else(|_| "[]".into());
    let now = UtcDateTime::now();
    sqlx::query!(
        "INSERT INTO cards (id, oracle_id, name, normalized_name, mana_cost, cmc, type_line, oracle_text, colors,
                            color_identity, image_uris, set_code, collector_number, released_at, layout, rarity,
                            game_changer, commander_legal, can_be_commander, commander_pairing, inserted_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        oracle_id,
        name,
        normalized_name,
        mana_cost,
        cmc,
        type_line,
        oracle_text,
        colors,
        color_identity,
        image_uris,
        set_code,
        collector_number,
        released_at,
        layout,
        rarity,
        game_changer,
        commander_legal,
        can_be_commander,
        commander_pairing,
        now,
        now
    )
    .execute(&mut *conn)
    .await
    .map_err(|_| restore_error())?;
    Ok(())
}

async fn restore_printing(conn: &mut SqliteConnection, row: &Value) -> Result<(), ImportError> {
    let bad = |()| card_data_error();
    let required = |key: &str| {
        cast_text(row.get(key))
            .map_err(bad)?
            .ok_or_else(card_data_error)
    };
    let id = required("id")?;
    let oracle_id = required("oracle_id")?;
    let name = required("name")?;
    let set_code = required("set_code")?;
    let set_name = required("set_name")?;
    let collector_number = required("collector_number")?;
    let lang = match row.get("lang") {
        None => "en".to_owned(),
        Some(_) => required("lang")?,
    };
    let image_uris = cast_map(row.get("image_uris"))
        .map_err(bad)?
        .ok_or_else(card_data_error)?;
    let game_changer = match row.get("game_changer") {
        None => false,
        Some(value) => cast_bool(Some(value))
            .map_err(bad)?
            .ok_or_else(card_data_error)?,
    };
    if catalog::get_printing_in(conn, &id).await?.is_some() {
        return Ok(());
    }
    sqlx::query!(
        "INSERT INTO card_printings (id, oracle_id, name, set_code, set_name, collector_number, lang, image_uris,
                                     game_changer)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        oracle_id,
        name,
        set_code,
        set_name,
        collector_number,
        lang,
        image_uris,
        game_changer
    )
    .execute(&mut *conn)
    .await
    .map_err(|_| restore_error())?;
    Ok(())
}

// Import

fn invalid(label: &str, errors: &ValidationError) -> ImportError {
    ImportError::Message(format!("{label}: {errors}"))
}

fn labeled(label: &str, error: crate::games::GamesError) -> ImportError {
    match error {
        crate::games::GamesError::Invalid(errors) => invalid(label, &errors),
        other => other.into(),
    }
}

fn reference<'a, T>(
    mapping: &'a HashMap<i64, T>,
    key: Option<&Value>,
    label: &str,
) -> Result<&'a T, ImportError> {
    key.and_then(local_id)
        .and_then(|key| mapping.get(&key))
        .ok_or_else(|| ImportError::Message(format!("Unknown {label} reference in export.")))
}

fn optional_reference<'a, T>(
    mapping: &'a HashMap<i64, T>,
    key: Option<&Value>,
    label: &str,
) -> Result<Option<&'a T>, ImportError> {
    match key {
        None | Some(Value::Null) => Ok(None),
        key => reference(mapping, key, label).map(Some),
    }
}

fn bump(counts: &mut Counts, created: bool) {
    if created {
        counts.created += 1;
    } else {
        counts.reused += 1;
    }
}

async fn restore_player(
    conn: &mut SqliteConnection,
    row: &Value,
) -> Result<(Player, bool), ImportError> {
    let attrs = Value::Object(attrs(row, &PLAYER_FIELDS));
    let mut cs = Changeset::new(&attrs);
    let name = crate::changeset::trim(cs.string("name").or(None));
    let _ = cs.datetime("archived_at");
    cs.required("name", name.as_ref());
    cs.length("name", name.as_deref(), Some(1), Some(100));
    cs.finish().map_err(|errors| invalid("Player", &errors))?;
    let name = name.unwrap_or_default();
    if let Some(existing) = player::find_player_by_name(conn, &name).await? {
        return Ok((existing, false));
    }
    let created = player::create_player(conn, &attrs, None)
        .await
        .map_err(|error| labeled(&format!("Player {name}"), error))?;
    Ok((created, true))
}

fn commander_pair(commander: Option<&str>, partner: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = [commander, partner]
        .into_iter()
        .flatten()
        .map(fold_name)
        .collect();
    names.sort();
    names
}

async fn restore_deck(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    row: &Value,
    players: &HashMap<i64, Player>,
) -> Result<(Deck, bool), ImportError> {
    let owner = reference(players, row.get("player_id"), "deck owner")?;
    let mut fields = attrs(row, &DECK_FIELDS);
    fields.insert("player_id".into(), json!(owner.id));
    let attrs = Value::Object(fields);
    let mut errors = match deck::validate_new_deck(conn, &attrs).await {
        Ok(()) => ValidationError::new(),
        Err(crate::games::GamesError::Invalid(errors)) => errors,
        Err(other) => return Err(other.into()),
    };
    // `cast(attrs, [:skip_count])`, `validate_required/2`, and `validate_number/3`.
    let skip_count = match attrs.get("skip_count") {
        None => Some(0),
        Some(value)
            if value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty()) =>
        {
            errors.add("skip_count", "can't be blank");
            None
        }
        Some(value) => match crate::changeset::cast_integer(value) {
            Some(count) if count < 0 => {
                errors.add("skip_count", "must be greater than or equal to 0");
                None
            }
            Some(count) => Some(count),
            None => {
                errors.add("skip_count", "is invalid");
                None
            }
        },
    };
    if attrs.get("included_for_play").is_some_and(Value::is_null) {
        errors.add("included_for_play", "can't be blank");
    }
    if !errors.is_empty() {
        return Err(invalid("Deck", &errors));
    }
    let name = attrs
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if let Some(existing) = deck::find_deck(conn, owner.id, &name, None, None).await? {
        let commander = attrs
            .get("commander_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let partner = attrs
            .get("partner_name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty());
        if commander_pair(
            Some(&existing.commander_name),
            existing.partner_name.as_deref(),
        ) != commander_pair(commander, partner)
        {
            return Err(ImportError::Message(format!(
                "{} already has a deck named {name} with different commanders. Rename one before importing.",
                owner.name
            )));
        }
        return Ok((existing, false));
    }
    let created = deck::create_deck(conn, links, &attrs)
        .await
        .map_err(|error| labeled(&format!("Deck {name}"), error))?;
    let skip_count = skip_count.unwrap_or_default();
    if skip_count != created.skip_count {
        sqlx::query!(
            "UPDATE decks SET skip_count = ? WHERE id = ?",
            skip_count,
            created.id
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok((
        Deck {
            skip_count,
            ..created
        },
        true,
    ))
}

async fn restore_game(
    conn: &mut SqliteConnection,
    row: &Value,
    players: &HashMap<i64, Player>,
    decks: &HashMap<i64, Deck>,
    user_id: Option<i64>,
) -> Result<(i64, bool), ImportError> {
    let mut seats = Vec::new();
    for seat in row
        .get("seats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let player = reference(players, seat.get("player_id"), "seat player")?;
        let deck = optional_reference(decks, seat.get("deck_id"), "seat deck")?;
        let eliminated_by = optional_reference(
            players,
            seat.get("eliminated_by_player_id"),
            "eliminating player",
        )?;
        if deck.is_some_and(|deck| deck.player_id != player.id) {
            return Err(ImportError::Message(
                "A seat uses another player's deck.".to_owned(),
            ));
        }
        let mut fields = attrs(seat, &SEAT_FIELDS);
        fields.insert("player_id".into(), json!(player.id));
        fields.insert("deck_id".into(), json!(deck.map(|deck| deck.id)));
        fields.insert(
            "eliminated_by_player_id".into(),
            json!(eliminated_by.map(|player| player.id)),
        );
        seats.push(Value::Object(fields));
    }
    let mut fields = attrs(row, &GAME_FIELDS);
    fields.insert("seats".into(), Value::Array(seats));
    let attrs = Value::Object(fields);
    let portable_id = row
        .get("portable_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let source = row
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("manual");
    let external_id = row.get("external_id").and_then(Value::as_str);
    let identity = (portable_id, source, external_id);
    let label = format!("Game {portable_id}");
    record_game::validate_portable(conn, &attrs, user_id, identity)
        .await
        .map_err(|error| labeled(&label, error))?;
    let portable = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM games WHERE portable_id = ?"#,
        portable_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    let external = match external_id {
        Some(external_id) => {
            sqlx::query_scalar!(
                r#"SELECT id AS "id!: i64" FROM games WHERE source = ? AND external_id = ?"#,
                source,
                external_id
            )
            .fetch_optional(&mut *conn)
            .await?
        }
        None => None,
    };
    if let (Some(portable), Some(external)) = (portable, external)
        && portable != external
    {
        return Err(ImportError::Message(
            "Game identities refer to different existing games.".to_owned(),
        ));
    }
    if let Some(existing) = portable.or(external) {
        return Ok((existing, false));
    }
    let game = record_game::insert_portable(conn, &attrs, user_id, identity)
        .await
        .map_err(|error| labeled("Game", error))?;
    Ok((game.id, true))
}

async fn restore_receipt(
    conn: &mut SqliteConnection,
    row: &Value,
    games: &HashMap<String, i64>,
) -> Result<(), ImportError> {
    let game_id = row
        .get("game_portable_id")
        .and_then(Value::as_str)
        .and_then(|key| games.get(key))
        .copied()
        .ok_or_else(|| {
            ImportError::Message("Unknown reconciled game reference in export.".to_owned())
        })?;
    let key = row
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ImportError::Message("Invalid sheet receipt key.".to_owned()))?;
    let existing = sqlx::query_scalar!(
        r#"SELECT game_id AS "game_id!: i64" FROM sheet_import_receipts WHERE key = ?"#,
        key
    )
    .fetch_optional(&mut *conn)
    .await?;
    match existing {
        None => {
            sqlx::query!(
                "INSERT INTO sheet_import_receipts (key, game_id) VALUES (?, ?)",
                key,
                game_id
            )
            .execute(&mut *conn)
            .await?;
            Ok(())
        }
        Some(id) if id == game_id => Ok(()),
        Some(_) => Err(ImportError::Message(
            "A sheet receipt already belongs to another game.".to_owned(),
        )),
    }
}

fn rows<'a>(data: &'a Map<String, Value>, key: &str) -> &'a [Value] {
    data.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

async fn restore(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    data: &Map<String, Value>,
    user_id: Option<i64>,
) -> Result<Summary, ImportError> {
    for card in rows(data, "cards") {
        restore_card(conn, card).await?;
    }
    for printing in rows(data, "printings") {
        restore_printing(conn, printing).await?;
    }
    let mut summary = Summary::default();
    let mut players = HashMap::new();
    for row in rows(data, "players") {
        let (player, created) = restore_player(conn, row).await?;
        bump(&mut summary.players, created);
        if let Some(id) = row.get("id").and_then(local_id) {
            players.insert(id, player);
        }
    }
    let mut decks = HashMap::new();
    for row in rows(data, "decks") {
        let (deck, created) = restore_deck(conn, links, row, &players).await?;
        bump(&mut summary.decks, created);
        if let Some(id) = row.get("id").and_then(local_id) {
            decks.insert(id, deck);
        }
    }
    let mut games = HashMap::new();
    for row in rows(data, "games") {
        let (game_id, created) = restore_game(conn, row, &players, &decks, user_id).await?;
        bump(&mut summary.games, created);
        if let Some(portable_id) = row.get("portable_id").and_then(Value::as_str) {
            games.insert(portable_id.to_owned(), game_id);
        }
    }
    for row in rows(data, "sheet_receipts") {
        restore_receipt(conn, row, &games).await?;
    }
    Ok(summary)
}

/// `PortableImport.preview/1`: the import's counts, with every write rolled back.
pub async fn preview(state: &AppState, json: &str) -> Result<Summary, ImportError> {
    let data = decode(json).map_err(ImportError::Message)?;
    let mut tx = db::begin(&state.pool).await?;
    let result = restore(&mut tx, state.games.deck_links(), &data, None).await;
    tx.rollback().await?;
    result
}

/// `PortableImport.run/2`.
pub async fn run(
    state: &AppState,
    json: &str,
    user_id: Option<i64>,
) -> Result<Summary, ImportError> {
    let data = decode(json).map_err(ImportError::Message)?;
    let mut tx = db::begin(&state.pool).await?;
    let summary = restore(&mut tx, state.games.deck_links(), &data, user_id).await?;
    tx.commit().await?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_records_name_the_record_and_its_failures() {
        let ImportError::Message(message) = invalid(
            "Player Drew",
            &ValidationError::single("name", crate::validation::TAKEN),
        ) else {
            panic!("expected a message");
        };
        assert_eq!(message, "Player Drew: Name has already been taken");
    }
}
