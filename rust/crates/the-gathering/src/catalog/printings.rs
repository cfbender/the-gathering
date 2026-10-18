//! Alternate printings, full printing details, and rulings, read from the printings and
//! rulings each catalog sync imports from Scryfall's bulk files.
//!
//! Every printing keeps a trimmed copy of its Scryfall card object (`card_printings.card`),
//! and these work on that raw JSON: faces are selected by copying a face's fields over the
//! card, and the details keep fields (prices, `scryfall_uri`) the shared card model omits.

use std::collections::BTreeMap;

use lotus::scryfall::catalog::EXCLUDED_SET_TYPES;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use super::{Card, Printing, parse_image_uris, printing_id};
use crate::db::{IsoDate, Pool};

/// Printings per picker page.
const PAGE_SIZE: usize = 175;
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
/// The card fields a stored printing keeps besides its images, prices, and faces.
const CARD_FIELDS: [&str; 19] = [
    "id",
    "oracle_id",
    "name",
    "set",
    "set_name",
    "collector_number",
    "lang",
    "game_changer",
    "mana_cost",
    "type_line",
    "oracle_text",
    "flavor_text",
    "power",
    "toughness",
    "loyalty",
    "layout",
    "rarity",
    "released_at",
    "scryfall_uri",
];
/// The card fields `card_printings` keeps in its own columns rather than in `card`.
const COLUMN_FIELDS: [&str; 10] = [
    "id",
    "oracle_id",
    "name",
    "set",
    "set_name",
    "collector_number",
    "lang",
    "game_changer",
    "released_at",
    "image_uris",
];
const IMAGE_VARIANTS: [&str; 3] = ["small", "normal", "art_crop"];
const PRICES: [&str; 3] = ["usd", "usd_foil", "usd_etched"];

/// Why a printing lookup failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupError {
    /// A malformed id or a face the printing does not have (400).
    BadRequest,
    /// Unknown card or printing (404).
    NotFound,
    /// The database failed (500; logged).
    Database,
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(database)`
fn database(error: sqlx::Error) -> LookupError {
    tracing::error!("printing lookup: {error}");
    LookupError::Database
}

/// The value when present and neither `null` nor `false`.
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

/// The card with one face's fields copied over it and `id` as its id.
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

/// The `card_printings` row of a card object.
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

/// Everything the card preview shows.
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

/// The `fields` of `object` that are set, with image URLs cut to the variants the app serves.
fn keep(object: &Map<String, Value>, fields: &[&str]) -> Map<String, Value> {
    let mut kept: Map<String, Value> = fields
        .iter()
        .filter_map(|field| {
            object
                .get(*field)
                .filter(|value| !value.is_null())
                .map(|value| ((*field).to_owned(), value.clone()))
        })
        .collect();
    if let Some(Value::Object(images)) = object.get("image_uris") {
        kept.insert(
            "image_uris".to_owned(),
            Value::Object(keep(images, &IMAGE_VARIANTS)),
        );
    }
    kept
}

/// A printing as the catalog sync stores it: its `card_printings` columns, and in `card` the
/// rest of the card object the details and pickers read.
#[derive(Clone, Debug, PartialEq)]
pub struct BulkPrinting {
    printing: Printing,
    selectable: bool,
    released_at: Option<IsoDate>,
    card: String,
}

/// The stored form of a Scryfall card object, listed by pickers when it is English, paper,
/// and not from a token or memorabilia set. `None` without an id, name, or oracle id;
/// reversible cards carry the oracle id on each face, and the front face's stands for the
/// printing.
pub fn from_bulk(raw: &Value) -> Option<BulkPrinting> {
    let object = raw.as_object()?;
    object.get("id").and_then(Value::as_str)?;
    object.get("name").and_then(Value::as_str)?;
    let oracle_id = object
        .get("oracle_id")
        .or_else(|| front(raw).and_then(|face| face.get("oracle_id")))
        .and_then(Value::as_str)?;
    let mut card = keep(object, &CARD_FIELDS);
    card.insert("oracle_id".to_owned(), Value::String(oracle_id.to_owned()));
    if let Some(Value::Object(prices)) = object.get("prices") {
        card.insert("prices".to_owned(), Value::Object(keep(prices, &PRICES)));
    }
    let face_fields: Vec<&str> = std::iter::once("oracle_id").chain(FACE_FIELDS).collect();
    let faces: Vec<Value> = faces(raw)
        .iter()
        .filter_map(Value::as_object)
        .map(|face| Value::Object(keep(face, &face_fields)))
        .collect();
    if !faces.is_empty() {
        card.insert("card_faces".to_owned(), Value::Array(faces));
    }
    let card = Value::Object(card);
    let printing = printing_data(&card);
    let released_at = card
        .get("released_at")
        .and_then(Value::as_str)
        .and_then(IsoDate::parse);
    let mut extras = card;
    if let Some(extras) = extras.as_object_mut() {
        for field in COLUMN_FIELDS {
            extras.remove(field);
        }
    }
    let paper = object
        .get("games")
        .and_then(Value::as_array)
        .is_some_and(|games| games.iter().any(|game| game == "paper"));
    let excluded_set = object
        .get("set_type")
        .and_then(Value::as_str)
        .is_some_and(|set_type| EXCLUDED_SET_TYPES.contains(&set_type));
    Some(BulkPrinting {
        printing,
        selectable: object.get("lang").and_then(Value::as_str) == Some("en")
            && paper
            && !excluded_set,
        released_at,
        card: extras.to_string(),
    })
}

/// A `card_printings` row the sync stored.
struct StoredPrinting {
    id: String,
    oracle_id: String,
    name: String,
    set_code: String,
    set_name: String,
    collector_number: String,
    lang: String,
    image_uris: String,
    game_changer: bool,
    released_at: Option<IsoDate>,
    card: String,
}

impl StoredPrinting {
    /// The card object: `card` with the printing's columns put back. The image column holds
    /// the front face's images when the card has none of its own, which reads the same.
    fn card(self) -> Option<Value> {
        let Ok(Value::Object(mut card)) = serde_json::from_str(&self.card) else {
            return None;
        };
        let images = parse_image_uris(&self.image_uris);
        for (field, value) in [
            ("id", json!(self.id)),
            ("oracle_id", json!(self.oracle_id)),
            ("name", json!(self.name)),
            ("set", json!(self.set_code)),
            ("set_name", json!(self.set_name)),
            ("collector_number", json!(self.collector_number)),
            ("lang", json!(self.lang)),
            ("game_changer", json!(self.game_changer)),
            ("released_at", json!(self.released_at)),
            ("image_uris", json!(images)),
        ] {
            if !(value.is_null() || field == "image_uris" && images.is_empty()) {
                card.insert(field.to_owned(), value);
            }
        }
        Some(Value::Object(card))
    }
}

/// Inserts or refreshes a printing, recording catalog sync `sync` as the run that listed it.
pub async fn store(
    conn: &mut SqliteConnection,
    row: &BulkPrinting,
    sync: i64,
) -> Result<(), sqlx::Error> {
    let printing = &row.printing;
    let images = serde_json::to_string(&printing.image_uris).unwrap_or_else(|_| "{}".to_owned());
    sqlx::query!(
        "INSERT INTO card_printings (id, oracle_id, name, set_code, set_name, collector_number, lang, image_uris,
                                     game_changer, card, catalog_sync_id, selectable, released_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET oracle_id = excluded.oracle_id, name = excluded.name,
           set_code = excluded.set_code, set_name = excluded.set_name, collector_number = excluded.collector_number,
           lang = excluded.lang, image_uris = excluded.image_uris, game_changer = excluded.game_changer,
           card = excluded.card, catalog_sync_id = excluded.catalog_sync_id, selectable = excluded.selectable,
           released_at = excluded.released_at",
        printing.id,
        printing.oracle_id,
        printing.name,
        printing.set_code,
        printing.set_name,
        printing.collector_number,
        printing.lang,
        images,
        printing.game_changer,
        row.card,
        sync,
        row.selectable,
        row.released_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the printings catalog sync `sync` did not list, except those a deck shows.
pub async fn prune(conn: &mut SqliteConnection, sync: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM card_printings WHERE catalog_sync_id IS NOT ?
           AND id NOT IN (SELECT commander_printing_id FROM decks WHERE commander_printing_id IS NOT NULL)
           AND id NOT IN (SELECT partner_printing_id FROM decks WHERE partner_printing_id IS NOT NULL)",
        sync
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One page of a card's English paper printings, newest first. `name` keeps the requested
/// half of a split or flip card selected.
pub async fn list(
    pool: &Pool,
    card: &Card,
    page: u32,
    name: Option<&str>,
) -> Result<(Vec<Printing>, bool), LookupError> {
    let limit = i64::try_from(PAGE_SIZE + 1).unwrap_or(i64::MAX);
    let offset = i64::try_from(PAGE_SIZE)
        .unwrap_or(i64::MAX)
        .saturating_mul(i64::from(page.saturating_sub(1)));
    let mut cards = sqlx::query_as!(
        StoredPrinting,
        r#"SELECT id AS "id!", oracle_id, name, set_code, set_name, collector_number, lang, image_uris,
                  game_changer AS "game_changer: bool", released_at AS "released_at: IsoDate", card AS "card!"
           FROM card_printings
           WHERE oracle_id = ? AND selectable AND card IS NOT NULL
           ORDER BY released_at DESC, set_code DESC, CAST(collector_number AS INTEGER) DESC,
                    collector_number DESC, id DESC
           LIMIT ? OFFSET ?"#,
        card.oracle_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await
    .map_err(database)?;
    let has_more = cards.len() > PAGE_SIZE;
    cards.truncate(PAGE_SIZE);
    let rows = cards
        .into_iter()
        .filter_map(StoredPrinting::card)
        .map(|card| printing_data(&select_named_half(card, name)))
        .collect();
    Ok((rows, has_more))
}

/// Rules text, cost, type, set, prices, and images of a printing by Scryfall id (with `-1`
/// for the second face).
pub async fn details(pool: &Pool, id: &str) -> Result<Value, LookupError> {
    let (card_id, face) = printing_id::parse(id).ok_or(LookupError::BadRequest)?;
    let card = sqlx::query_as!(
        StoredPrinting,
        r#"SELECT id AS "id!", oracle_id, name, set_code, set_name, collector_number, lang, image_uris,
                  game_changer AS "game_changer: bool", released_at AS "released_at: IsoDate", card AS "card!"
           FROM card_printings WHERE id = ? AND card IS NOT NULL"#,
        card_id
    )
    .fetch_optional(pool)
    .await
    .map_err(database)?
    .and_then(StoredPrinting::card)
    .ok_or(LookupError::NotFound)?;
    let card = select_face(&card, face, id)?;
    let row = printing_data(&card);
    Ok(details_data(&card, &row))
}

/// One Scryfall ruling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ruling {
    /// `wotc` or `scryfall`.
    pub source: Option<String>,
    /// Publication date.
    pub published_at: Option<IsoDate>,
    /// The ruling text.
    pub comment: String,
}

/// The rulings of a printing's card (the second face's id included), oldest first.
pub async fn rulings(pool: &Pool, id: &str) -> Result<Vec<Ruling>, LookupError> {
    let (card_id, _face) = printing_id::parse(id).ok_or(LookupError::BadRequest)?;
    let oracle_id =
        sqlx::query_scalar!("SELECT oracle_id FROM card_printings WHERE id = ?", card_id)
            .fetch_optional(pool)
            .await
            .map_err(database)?
            .ok_or(LookupError::NotFound)?;
    sqlx::query_as!(
        Ruling,
        r#"SELECT source, published_at AS "published_at: IsoDate", comment
           FROM card_rulings WHERE oracle_id = ? ORDER BY published_at, id"#,
        oracle_id
    )
    .fetch_all(pool)
    .await
    .map_err(database)
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
