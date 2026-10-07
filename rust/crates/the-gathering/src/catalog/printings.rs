//! Alternate printings and full printing details fetched from Scryfall on demand
//! plus cached rulings.
//!
//! These work on Scryfall's raw JSON: faces are selected by copying a face's fields over the
//! card, and the details keep fields (prices, `scryfall_uri`) the shared card model omits.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::scryfall::{Failure, Scryfall};
use super::{Card, Printing, parse_image_uris, printing_id};
use crate::db::{self, Pool, UtcDateTime};

/// Scryfall syncs prices from its affiliates every 24 hours, so a day-old copy loses little.
const DETAILS_TTL_SECONDS: i64 = 86_400;
const RULINGS_TTL_SECONDS: i64 = 86_400;
const FACE_LAYOUTS: [&str; 6] = [
    "transform",
    "modal_dfc",
    "reversible_card",
    "double_faced_token",
    "split",
    "flip",
];
const FACE_FIELDS: [&str; 9] = [
    "name",
    "image_uris",
    "mana_cost",
    "type_line",
    "oracle_text",
    "flavor_text",
    "power",
    "toughness",
    "loyalty",
];
const HALF_LAYOUTS: [&str; 2] = ["split", "flip"];

/// Why a printing lookup failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupError {
    /// A malformed id or a face the printing does not have (400).
    BadRequest,
    /// Unknown card or printing (404).
    NotFound,
    /// Scryfall failed (502).
    BadGateway,
    /// The database failed (500; logged).
    Database,
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(database)`
fn database(error: sqlx::Error) -> LookupError {
    tracing::error!("printing cache: {error}");
    LookupError::Database
}

impl From<Failure> for LookupError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::NotFound => Self::NotFound,
            Failure::BadGateway => Self::BadGateway,
        }
    }
}

/// Elixir truthiness: present and neither `null` nor `false`.
fn truthy(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !matches!(value, Value::Null | Value::Bool(false)))
}

fn faces(card: &Value) -> &[Value] {
    card.get("card_faces")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn front(card: &Value) -> Option<&Value> {
    faces(card).first()
}

fn layout(card: &Value) -> Option<&str> {
    card.get("layout").and_then(Value::as_str)
}

/// `select_face/3`: the card with one face's fields copied over it and `id` as its id.
fn select_face(card: &Value, index: usize, id: &str) -> Result<Value, LookupError> {
    let layout = layout(card);
    if layout.is_some_and(|layout| FACE_LAYOUTS.contains(&layout))
        && let Some(Value::Array(faces)) = card.get("card_faces")
    {
        let face = faces
            .get(index)
            .and_then(Value::as_object)
            .filter(|face| face.contains_key("name"));
        let Some(face) = face else {
            return Err(LookupError::BadRequest);
        };
        // Split/flip halves share the front scan; do not invent a /back image URL or
        // fall back to front-side images for a genuinely double-faced card.
        let images = truthy(face.get("image_uris"))
            .or_else(|| {
                layout
                    .filter(|layout| HALF_LAYOUTS.contains(layout))
                    .and_then(|_| card.get("image_uris"))
            })
            .cloned()
            .unwrap_or(Value::Null);
        let mut selected: Map<String, Value> = card.as_object().cloned().unwrap_or_default();
        selected.remove("card_faces");
        for field in FACE_FIELDS {
            selected.remove(field);
        }
        for field in std::iter::once("oracle_id").chain(FACE_FIELDS) {
            if let Some(value) = face.get(field) {
                selected.insert(field.to_owned(), value.clone());
            }
        }
        selected.insert("image_uris".to_owned(), images);
        selected.insert("id".to_owned(), Value::String(id.to_owned()));
        return Ok(Value::Object(selected));
    }
    if index == 0 {
        Ok(card.clone())
    } else {
        Err(LookupError::BadRequest)
    }
}

/// A webcam preview requests a half by name; the deck picker requests the combined name.
/// Keep the same half selected while cycling alternate printings of a split/flip card.
fn select_named_half(card: Value, name: Option<&str>) -> Value {
    let (Some(name), Some(layout)) = (name, layout(&card)) else {
        return card;
    };
    if !HALF_LAYOUTS.contains(&layout) || card.get("card_faces").and_then(Value::as_array).is_none()
    {
        return card;
    }
    let wanted = lotus::normalize_name(name);
    let index = faces(&card).iter().position(|face| {
        face.get("name")
            .and_then(Value::as_str)
            .is_some_and(|face_name| lotus::normalize_name(face_name) == wanted)
    });
    match index {
        Some(index @ (0 | 1)) => {
            let base = card.get("id").and_then(Value::as_str).unwrap_or_default();
            let id = if index == 1 {
                format!("{base}-1")
            } else {
                base.to_owned()
            };
            select_face(&card, index, &id).unwrap_or(card)
        }
        _ => card,
    }
}

fn string_field(card: &Value, key: &str) -> Option<String> {
    card.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// `printing_data/1`: the `card_printings` row of a card object.
fn printing_data(card: &Value) -> Printing {
    let images = truthy(card.get("image_uris"))
        .or_else(|| front(card).and_then(|face| truthy(face.get("image_uris"))))
        .and_then(Value::as_object);
    let image_uris: BTreeMap<String, String> = images
        .map(|images| {
            ["small", "normal", "art_crop"]
                .into_iter()
                .filter_map(|variant| {
                    images
                        .get(variant)
                        .and_then(Value::as_str)
                        .map(|url| (variant.to_owned(), url.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    Printing {
        id: string_field(card, "id").unwrap_or_default(),
        oracle_id: string_field(card, "oracle_id").unwrap_or_default(),
        name: string_field(card, "name").unwrap_or_default(),
        set_code: string_field(card, "set").unwrap_or_default(),
        set_name: string_field(card, "set_name").unwrap_or_default(),
        collector_number: string_field(card, "collector_number").unwrap_or_default(),
        lang: string_field(card, "lang").unwrap_or_else(|| "en".to_owned()),
        image_uris,
        game_changer: card.get("game_changer") == Some(&Value::Bool(true)),
    }
}

/// Multi-face cards keep their text per face; join it so the preview shows every half.
fn face_text(card: &Value, key: &str) -> Value {
    if let Some(Value::String(text)) = card.get(key) {
        return Value::String(text.clone());
    }
    let texts: Vec<&str> = faces(card)
        .iter()
        .filter_map(|face| face.get(key).and_then(Value::as_str))
        .collect();
    if texts.is_empty() {
        Value::Null
    } else {
        Value::String(texts.join("\n//\n"))
    }
}

fn card_or_front(card: &Value, key: &str) -> Value {
    truthy(card.get(key))
        .or_else(|| front(card).and_then(|face| truthy(face.get(key))))
        .cloned()
        .unwrap_or(Value::Null)
}

/// `details_data/2`: everything the card preview shows.
fn details_data(card: &Value, row: &Printing) -> Value {
    let prices: Map<String, Value> = ["usd", "usd_foil", "usd_etched"]
        .into_iter()
        .map(|key| {
            let price = card
                .get("prices")
                .and_then(|prices| prices.get(key))
                .cloned()
                .unwrap_or(Value::Null);
            (key.to_owned(), price)
        })
        .collect();
    let type_line = match card_or_front(card, "type_line") {
        Value::Null => Value::String(String::new()),
        other => other,
    };
    json!({
        "id": row.id,
        "oracle_id": row.oracle_id,
        "name": row.name,
        "set_code": row.set_code,
        "set_name": row.set_name,
        "collector_number": row.collector_number,
        "lang": row.lang,
        "game_changer": row.game_changer,
        "image_uris": row.image_uris,
        "mana_cost": card_or_front(card, "mana_cost"),
        "type_line": type_line,
        "oracle_text": face_text(card, "oracle_text"),
        "flavor_text": face_text(card, "flavor_text"),
        "power": card_or_front(card, "power"),
        "toughness": card_or_front(card, "toughness"),
        "loyalty": card_or_front(card, "loyalty"),
        "layout": card.get("layout").cloned().unwrap_or_else(|| Value::String("normal".to_owned())),
        "rarity": card.get("rarity").cloned().unwrap_or(Value::Null),
        "released_at": card.get("released_at").cloned().unwrap_or(Value::Null),
        "prices": prices,
        "scryfall_uri": card.get("scryfall_uri").cloned().unwrap_or(Value::Null),
    })
}

async fn upsert_printing(
    conn: &mut sqlx::SqliteConnection,
    printing: &Printing,
) -> Result<(), sqlx::Error> {
    let images = serde_json::to_string(&printing.image_uris).unwrap_or_else(|_| "{}".to_owned());
    sqlx::query!(
        "INSERT INTO card_printings (id, oracle_id, name, set_code, set_name, collector_number, lang, image_uris, game_changer)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET oracle_id = excluded.oracle_id, name = excluded.name,
           set_code = excluded.set_code, set_name = excluded.set_name, collector_number = excluded.collector_number,
           lang = excluded.lang, image_uris = excluded.image_uris, game_changer = excluded.game_changer",
        printing.id,
        printing.oracle_id,
        printing.name,
        printing.set_code,
        printing.set_name,
        printing.collector_number,
        printing.lang,
        images,
        printing.game_changer
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// `Printings.list/3`: one page of a card's English paper printings, cached in
/// `card_printings`. `name` keeps the requested half of a split or flip card selected.
pub async fn list(
    pool: &Pool,
    scryfall: &Scryfall,
    card: &Card,
    page: u32,
    name: Option<&str>,
) -> Result<(Vec<Printing>, bool), LookupError> {
    let (cards, has_more) = scryfall.printings(&card.oracle_id, page).await?;
    let rows: Vec<Printing> = cards
        .into_iter()
        .filter(|printing| {
            printing.get("oracle_id").and_then(Value::as_str) == Some(card.oracle_id.as_str())
                && printing
                    .get("games")
                    .and_then(Value::as_array)
                    .is_some_and(|games| games.iter().any(|game| game == "paper"))
                && printing.get("lang").and_then(Value::as_str) == Some("en")
        })
        // Scryfall includes memorabilia basics; the catalog intentionally does not describe them.
        .filter(|printing| {
            !matches!(
                printing.get("set_type").and_then(Value::as_str),
                Some("token" | "memorabilia")
            )
        })
        .map(|printing| printing_data(&select_named_half(printing, name)))
        .collect();
    if !rows.is_empty() {
        let mut tx = db::begin(pool).await.map_err(database)?;
        for row in &rows {
            upsert_printing(&mut tx, row).await.map_err(database)?;
        }
        tx.commit().await.map_err(database)?;
    }
    Ok((rows, has_more))
}

/// `Printings.details/1`: rules text, cost, type, set, prices, and images of any printing
/// by Scryfall id (with `-1` for the second face), cached for a day in `card_details_cache`
/// so every seat at a table, and every later game, reads it locally. The printing's image
/// and set are cached in `card_printings` on the way through.
pub async fn details(pool: &Pool, scryfall: &Scryfall, id: &str) -> Result<Value, LookupError> {
    let (card_id, face) = printing_id::parse(id).ok_or(LookupError::BadRequest)?;
    let now = UtcDateTime::now();
    let cached = sqlx::query!(
        r#"SELECT details, fetched_at AS "fetched_at: UtcDateTime" FROM card_details_cache WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(database)?;
    if let Some(row) = cached
        && now.unix() - row.fetched_at.unix() < DETAILS_TTL_SECONDS
        && let Ok(details) = serde_json::from_str::<Value>(&row.details)
    {
        return Ok(details);
    }
    let card = scryfall.card(card_id).await?;
    let card = select_face(&card, face, id)?;
    let row = printing_data(&card);
    let details = details_data(&card, &row);
    let encoded = details.to_string();
    let mut tx = db::begin(pool).await.map_err(database)?;
    upsert_printing(&mut tx, &row).await.map_err(database)?;
    sqlx::query!(
        "INSERT INTO card_details_cache (id, details, fetched_at) VALUES (?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET details = excluded.details, fetched_at = excluded.fetched_at",
        id,
        encoded,
        now
    )
    .execute(&mut *tx)
    .await
    .map_err(database)?;
    tx.commit().await.map_err(database)?;
    Ok(details)
}

/// `Rulings.get/1`: a printing's rulings, cached for a day by printing id (face suffix
/// included) in `card_rulings_cache`.
pub async fn rulings(
    pool: &Pool,
    scryfall: &Scryfall,
    id: &str,
) -> Result<Vec<Value>, LookupError> {
    let (card_id, _face) = printing_id::parse(id).ok_or(LookupError::BadRequest)?;
    let now = UtcDateTime::now();
    let cached = sqlx::query!(
        r#"SELECT rulings, fetched_at AS "fetched_at: UtcDateTime" FROM card_rulings_cache WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(database)?;
    if let Some(row) = cached
        && now.unix() - row.fetched_at.unix() < RULINGS_TTL_SECONDS
        && let Ok(rulings) = serde_json::from_str::<Vec<Value>>(&row.rulings)
    {
        return Ok(rulings);
    }
    let rulings = scryfall.rulings(card_id).await?;
    let encoded = Value::Array(rulings.clone()).to_string();
    sqlx::query!(
        "INSERT INTO card_rulings_cache (id, rulings, fetched_at) VALUES (?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET rulings = excluded.rulings, fetched_at = excluded.fetched_at",
        id,
        encoded,
        now
    )
    .execute(pool)
    .await
    .map_err(database)?;
    Ok(rulings)
}

/// `details` with its `image_uris` pointed at the image cache, as the API renders it.
pub fn render_details(details: &Value) -> Value {
    let mut rendered = details.clone();
    if let Some(object) = rendered.as_object_mut() {
        let images = object
            .get("image_uris")
            .map(|images| parse_image_uris(&images.to_string()))
            .unwrap_or_default();
        object.insert("image_uris".to_owned(), json!(super::images::urls(&images)));
    }
    rendered
}
