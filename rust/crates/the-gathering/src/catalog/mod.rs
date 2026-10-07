//! The local card catalog synchronized from Scryfall (`TheGathering.Catalog`): lookups,
//! search, and the batched summaries other areas use for names, art, and identities, plus
//! the write side (sync, backfill) and the on-demand Scryfall lookups (printings, details,
//! rulings, images).

pub mod backfill;
pub mod card_data;
pub mod image_cache;
pub mod images;
pub mod printing_id;
pub mod printings;
pub mod scryfall;
pub mod sync;
pub mod sync_server;

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use sqlx::SqliteConnection;

use crate::db::{IsoDate, Pool, UtcDateTime};
use crate::games::color_identity;

/// A catalog card: one row per Oracle card, represented by its preferred printing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Card {
    /// Scryfall id of the preferred printing.
    pub id: String,
    /// Oracle id.
    pub oracle_id: String,
    /// Printed name.
    pub name: String,
    /// [`lotus::normalize_name`] of the name.
    pub normalized_name: String,
    /// Mana cost.
    pub mana_cost: Option<String>,
    /// Mana value.
    pub cmc: f64,
    /// Type line.
    pub type_line: String,
    /// Oracle text.
    pub oracle_text: Option<String>,
    /// Colors.
    pub colors: Vec<String>,
    /// Color identity.
    pub color_identity: Vec<String>,
    /// Scryfall image URLs (`small`, `normal`, `art_crop`).
    pub image_uris: BTreeMap<String, String>,
    /// Set code.
    pub set_code: String,
    /// Collector number.
    pub collector_number: String,
    /// Release date.
    pub released_at: Option<IsoDate>,
    /// Layout.
    pub layout: String,
    /// Rarity.
    pub rarity: String,
    /// On the Game Changers list.
    pub game_changer: bool,
    /// Legal in Commander.
    pub commander_legal: bool,
    /// May lead a Commander deck.
    pub can_be_commander: bool,
    /// Pairing mechanic (`partner`, `background`, ...).
    pub commander_pairing: Option<String>,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
}

/// A `cards` row as stored (JSON columns as text).
#[derive(Debug, sqlx::FromRow)]
pub struct CardRow {
    id: String,
    oracle_id: String,
    name: String,
    normalized_name: String,
    mana_cost: Option<String>,
    cmc: f64,
    type_line: String,
    oracle_text: Option<String>,
    colors: String,
    color_identity: String,
    image_uris: String,
    set_code: String,
    collector_number: String,
    released_at: Option<IsoDate>,
    layout: String,
    rarity: String,
    game_changer: bool,
    commander_legal: bool,
    can_be_commander: bool,
    commander_pairing: Option<String>,
    inserted_at: UtcDateTime,
    updated_at: UtcDateTime,
}

impl From<CardRow> for Card {
    fn from(row: CardRow) -> Self {
        Self {
            id: row.id,
            oracle_id: row.oracle_id,
            name: row.name,
            normalized_name: row.normalized_name,
            mana_cost: row.mana_cost,
            cmc: row.cmc,
            type_line: row.type_line,
            oracle_text: row.oracle_text,
            colors: serde_json::from_str(&row.colors).unwrap_or_default(),
            color_identity: serde_json::from_str(&row.color_identity).unwrap_or_default(),
            image_uris: parse_image_uris(&row.image_uris),
            set_code: row.set_code,
            collector_number: row.collector_number,
            released_at: row.released_at,
            layout: row.layout,
            rarity: row.rarity,
            game_changer: row.game_changer,
            commander_legal: row.commander_legal,
            can_be_commander: row.can_be_commander,
            commander_pairing: row.commander_pairing,
            inserted_at: row.inserted_at,
            updated_at: row.updated_at,
        }
    }
}

/// Decodes an `image_uris` JSON column, keeping string values only.
pub fn parse_image_uris(json: &str) -> BTreeMap<String, String> {
    serde_json::from_str::<BTreeMap<String, serde_json::Value>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key, value.to_owned())))
        .collect()
}

/// Selects `CardRow`s: `select_cards!("WHERE id = ?", id)`.
macro_rules! select_cards {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            CardRow,
            r#"SELECT id AS "id!", oracle_id, name, normalized_name, mana_cost, cmc AS "cmc: f64", type_line,
                oracle_text, colors, color_identity, image_uris, set_code, collector_number,
                released_at AS "released_at: IsoDate", layout, rarity, game_changer AS "game_changer: bool",
                commander_legal AS "commander_legal: bool", can_be_commander AS "can_be_commander: bool",
                commander_pairing, inserted_at AS "inserted_at: UtcDateTime", updated_at AS "updated_at: UtcDateTime"
               FROM cards "# + $tail
            $(, $arg)*
        )
    };
}
#[allow(unused_imports)] // for other areas' card queries
pub(crate) use select_cards;

/// A cached printing (`card_printings`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Printing {
    /// Scryfall id.
    pub id: String,
    /// Oracle id.
    pub oracle_id: String,
    /// Printed name.
    pub name: String,
    /// Set code.
    pub set_code: String,
    /// Set name.
    pub set_name: String,
    /// Collector number.
    pub collector_number: String,
    /// Language.
    pub lang: String,
    /// Scryfall image URLs.
    pub image_uris: BTreeMap<String, String>,
    /// On the Game Changers list.
    pub game_changer: bool,
}

/// Card lookups need the pool only.
#[derive(Clone, Debug)]
pub struct Catalog {
    /// Database.
    pub pool: Pool,
}

/// A batched name/art/identity lookup result (`Catalog.card_summaries/1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CardSummary {
    /// Scryfall id.
    pub id: String,
    /// Name.
    pub name: String,
    /// On the Game Changers list.
    pub game_changer: bool,
    /// Cached art crop URL.
    pub art_crop_url: Option<String>,
    /// Cached normal image URL.
    pub image_url: Option<String>,
    /// Canonical WUBRG identity letters.
    pub color_identity: String,
}

/// Summaries keyed by id and by normalized name.
#[derive(Clone, Debug, Default)]
pub struct CardSummaries {
    by_id: HashMap<String, CardSummary>,
    by_name: HashMap<String, CardSummary>,
}

impl CardSummaries {
    /// `Catalog.card_summary/3`: the stored id wins, then the name.
    pub fn get(&self, id: Option<&str>, name: Option<&str>) -> Option<&CardSummary> {
        id.and_then(|id| self.by_id.get(id))
            .or_else(|| name.and_then(|name| self.by_name.get(&lotus::normalize_name(name))))
    }
}

/// Art and image URLs for card references and exact printings (`Catalog.art_crop_urls/1`).
#[derive(Clone, Debug, Default)]
pub struct ArtUrls {
    summaries: CardSummaries,
    printings: HashMap<String, (Option<String>, Option<String>)>,
}

impl ArtUrls {
    /// `Catalog.art_crop_url/4`: the printing's crop, else the card's.
    pub fn art_crop_url(&self, id: Option<&str>, name: Option<&str>, printing_id: Option<&str>) -> Option<String> {
        printing_id
            .and_then(|printing| self.printings.get(printing))
            .and_then(|(crop, _)| crop.clone())
            .or_else(|| self.summaries.get(id, name).and_then(|summary| summary.art_crop_url.clone()))
    }

    /// `Catalog.card_image_url/4`.
    pub fn card_image_url(&self, id: Option<&str>, name: Option<&str>, printing_id: Option<&str>) -> Option<String> {
        printing_id
            .and_then(|printing| self.printings.get(printing))
            .and_then(|(_, image)| image.clone())
            .or_else(|| self.summaries.get(id, name).and_then(|summary| summary.image_url.clone()))
    }

    /// `Catalog.game_changer?/3`.
    pub fn game_changer(&self, id: Option<&str>, name: Option<&str>) -> bool {
        self.summaries.get(id, name).is_some_and(|summary| summary.game_changer)
    }

    /// The summaries behind these URLs.
    pub fn summaries(&self) -> &CardSummaries {
        &self.summaries
    }
}

/// A reference to resolve: a card id and/or name, or an exact printing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CardRef {
    /// A stored card id and/or printed name.
    Card(Option<String>, Option<String>),
    /// An exact printing id.
    Printing(Option<String>),
}

fn escape_like(value: &str) -> String {
    value.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn strip_search_punctuation(value: &str) -> String {
    value.replace(['\'', '\u{2019}', ','], "")
}

/// `face_query/1`: a double-faced, split, or flip card one of whose halves is `normalized`.
async fn find_by_face(conn: &mut SqliteConnection, normalized: &str) -> Result<Option<Card>, sqlx::Error> {
    let prefix = format!("{normalized} // ");
    let suffix = format!(" // {normalized}");
    let prefix_len = i64::try_from(prefix.chars().count()).unwrap_or(i64::MAX);
    let suffix_start = -i64::try_from(suffix.chars().count()).unwrap_or(0);
    Ok(select_cards!(
        "WHERE (substr(normalized_name, 1, ?) = ?
                OR (layout IN ('split', 'flip') AND substr(normalized_name, ?) = ?))
               AND name NOT LIKE 'A-%'
         ORDER BY can_be_commander DESC, released_at DESC LIMIT 1",
        prefix_len,
        prefix,
        suffix_start,
        suffix
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(Card::from))
}

impl Catalog {
    /// A card by id.
    pub async fn get_card(&self, id: &str) -> Result<Option<Card>, sqlx::Error> {
        get_card_in(&mut *self.pool.acquire().await?, id).await
    }

    /// How many cards are cached.
    pub async fn count_cards(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(r#"SELECT count(*) AS "count!: i64" FROM cards"#).fetch_one(&self.pool).await
    }

    /// `resolve_card/2`: by id, else by name.
    pub async fn resolve_card(&self, id: Option<&str>, name: Option<&str>) -> Result<Option<Card>, sqlx::Error> {
        resolve_card_in(&mut *self.pool.acquire().await?, id, name).await
    }

    /// A cached printing.
    pub async fn get_printing(&self, id: &str) -> Result<Option<Printing>, sqlx::Error> {
        get_printing_in(&mut *self.pool.acquire().await?, id).await
    }

    /// `find_card_by_name/1`: exact normalized name (commanders and newer printings first),
    /// else a face of a multi-faced card.
    pub async fn find_card_by_name(&self, name: &str) -> Result<Option<Card>, sqlx::Error> {
        find_card_by_name_in(&mut *self.pool.acquire().await?, name).await
    }

    /// `cards_by_name/1`: every given name that resolves, keyed by the given name.
    pub async fn cards_by_name(&self, names: &[String]) -> Result<HashMap<String, Card>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        let mut found = HashMap::new();
        let mut exact: HashMap<String, Option<Card>> = HashMap::new();
        for name in names {
            if found.contains_key(name) {
                continue;
            }
            let key = lotus::normalize_name(name);
            if !exact.contains_key(&key) {
                // Latest release wins when several catalog rows share a name.
                let card = select_cards!(
                    "WHERE normalized_name = ? ORDER BY released_at DESC LIMIT 1",
                    key
                )
                .fetch_optional(&mut *conn)
                .await?
                .map(Card::from);
                exact.insert(key.clone(), card);
            }
            let card = match exact.get(&key).cloned().flatten() {
                Some(card) => Some(card),
                None => find_by_face(&mut conn, &key).await?,
            };
            if let Some(card) = card {
                found.insert(name.clone(), card);
            }
        }
        Ok(found)
    }

    /// `search/2`: name search ranked exact, whole-word prefix, prefix, then substring.
    /// `commander` filters to (non-)commanders; `partner` allows any card that can share
    /// the command zone.
    pub async fn search(
        &self,
        query: &str,
        limit: Option<i64>,
        commander: Option<bool>,
        partner: bool,
    ) -> Result<Vec<Card>, sqlx::Error> {
        let normalized = strip_search_punctuation(&lotus::normalize_name(query.trim()));
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.unwrap_or(20).clamp(1, 50);
        let pattern = format!("%{}%", escape_like(&normalized));
        let prefix = format!("{}%", escape_like(&normalized));
        let whole_name_prefix = format!("{} %", escape_like(&normalized));
        let commander_filter = commander.map(i64::from);
        let partner_filter = i64::from(partner);
        Ok(select_cards!(
            r"WHERE replace(replace(replace(normalized_name, '''', ''), '’', ''), ',', '') LIKE ? ESCAPE '\'
                AND (? IS NULL OR can_be_commander = ?)
                AND (? = 0 OR can_be_commander OR commander_pairing IS NOT NULL)
              ORDER BY CASE
                  WHEN replace(replace(replace(normalized_name, '''', ''), '’', ''), ',', '') = ? THEN 0
                  WHEN replace(replace(replace(normalized_name, '''', ''), '’', ''), ',', '') LIKE ? ESCAPE '\' THEN 1
                  WHEN replace(replace(replace(normalized_name, '''', ''), '’', ''), ',', '') LIKE ? ESCAPE '\' THEN 2
                  ELSE 3 END,
                normalized_name, id
              LIMIT ?",
            pattern,
            commander_filter,
            commander_filter,
            partner_filter,
            normalized,
            whole_name_prefix,
            prefix,
            limit
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Card::from)
        .collect())
    }

    /// `card_summaries/1`: one lookup for many `(id, name)` references.
    pub async fn card_summaries(&self, refs: &[(Option<String>, Option<String>)]) -> Result<CardSummaries, sqlx::Error> {
        card_summaries_in(&mut *self.pool.acquire().await?, refs).await
    }

    /// `list_printings/3`: one page of printings of the card resolved from `id` or `name`.
    pub async fn list_printings(
        &self,
        scryfall: &scryfall::Scryfall,
        id: Option<&str>,
        name: Option<&str>,
        page: u32,
    ) -> Result<(Vec<Printing>, bool), printings::LookupError> {
        let card = self
            .resolve_card(id, name)
            .await
            .map_err(|error| {
                tracing::error!("resolving a card for printings: {error}");
                printings::LookupError::Database
            })?
            .ok_or(printings::LookupError::NotFound)?;
        printings::list(&self.pool, scryfall, &card, page, name).await
    }

    /// `sync_status/0`.
    pub async fn sync_status(&self) -> Result<sync::SyncState, sqlx::Error> {
        sync::status(&self.pool).await
    }

    /// `art_crop_urls/1`.
    pub async fn art_crop_urls(&self, refs: &[CardRef]) -> Result<ArtUrls, sqlx::Error> {
        art_crop_urls_in(&mut *self.pool.acquire().await?, refs).await
    }
}

/// [`Catalog::get_card`] on a connection (for callers inside a transaction).
pub async fn get_card_in(conn: &mut SqliteConnection, id: &str) -> Result<Option<Card>, sqlx::Error> {
    Ok(select_cards!("WHERE id = ?", id).fetch_optional(&mut *conn).await?.map(Card::from))
}

/// [`Catalog::resolve_card`] on a connection.
pub async fn resolve_card_in(
    conn: &mut SqliteConnection,
    id: Option<&str>,
    name: Option<&str>,
) -> Result<Option<Card>, sqlx::Error> {
    if let Some(id) = id
        && let Some(card) = get_card_in(conn, id).await?
    {
        return Ok(Some(card));
    }
    match name {
        Some(name) => find_card_by_name_in(conn, name).await,
        None => Ok(None),
    }
}

/// [`Catalog::get_printing`] on a connection.
pub async fn get_printing_in(conn: &mut SqliteConnection, id: &str) -> Result<Option<Printing>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id AS "id!", oracle_id, name, set_code, set_name, collector_number, lang, image_uris,
                  game_changer AS "game_changer: bool" FROM card_printings WHERE id = ?"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|row| Printing {
        id: row.id,
        oracle_id: row.oracle_id,
        name: row.name,
        set_code: row.set_code,
        set_name: row.set_name,
        collector_number: row.collector_number,
        lang: row.lang,
        image_uris: parse_image_uris(&row.image_uris),
        game_changer: row.game_changer,
    }))
}

/// [`Catalog::find_card_by_name`] on a connection.
pub async fn find_card_by_name_in(conn: &mut SqliteConnection, name: &str) -> Result<Option<Card>, sqlx::Error> {
    let normalized = lotus::normalize_name(name);
    if let Some(card) = select_cards!(
        "WHERE normalized_name = ? ORDER BY can_be_commander DESC, released_at DESC LIMIT 1",
        normalized
    )
    .fetch_optional(&mut *conn)
    .await?
    {
        return Ok(Some(card.into()));
    }
    find_by_face(conn, &normalized).await
}

/// [`Catalog::card_summaries`] on a connection.
pub async fn card_summaries_in(
    conn: &mut SqliteConnection,
    refs: &[(Option<String>, Option<String>)],
) -> Result<CardSummaries, sqlx::Error> {
    let mut ids: Vec<String> = refs.iter().filter_map(|(id, _)| id.clone()).collect();
    ids.sort();
    ids.dedup();
    let mut names: Vec<String> = refs.iter().filter_map(|(_, name)| name.as_deref().map(lotus::normalize_name)).collect();
    names.sort();
    names.dedup();
    let mut summaries = CardSummaries::default();
    if ids.is_empty() && names.is_empty() {
        return Ok(summaries);
    }
    let ids_json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into());
    let names_json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", name, normalized_name, image_uris, color_identity, game_changer AS "game_changer: bool"
           FROM cards
           WHERE id IN (SELECT value FROM json_each(?)) OR normalized_name IN (SELECT value FROM json_each(?))"#,
        ids_json,
        names_json
    )
    .fetch_all(&mut *conn)
    .await?;
    for row in rows {
        let images = parse_image_uris(&row.image_uris);
        let identity: Vec<String> = serde_json::from_str(&row.color_identity).unwrap_or_default();
        let summary = CardSummary {
            id: row.id.clone(),
            name: row.name,
            game_changer: row.game_changer,
            art_crop_url: images::url_opt(images.get("art_crop").map(String::as_str)),
            image_url: images::url_opt(images.get("normal").map(String::as_str)),
            color_identity: color_identity::canonical(&identity.concat()),
        };
        summaries.by_name.insert(row.normalized_name, summary.clone());
        summaries.by_id.insert(row.id, summary);
    }
    Ok(summaries)
}

/// [`Catalog::art_crop_urls`] on a connection.
pub async fn art_crop_urls_in(conn: &mut SqliteConnection, refs: &[CardRef]) -> Result<ArtUrls, sqlx::Error> {
    let mut identities = Vec::new();
    let mut printing_ids = Vec::new();
    for card_ref in refs {
        match card_ref {
            CardRef::Card(id, name) => identities.push((id.clone(), name.clone())),
            CardRef::Printing(Some(id)) => printing_ids.push(id.clone()),
            CardRef::Printing(None) => {}
        }
    }
    printing_ids.sort();
    printing_ids.dedup();
    let summaries = card_summaries_in(&mut *conn, &identities).await?;
    let mut printings = HashMap::new();
    if !printing_ids.is_empty() {
        let ids_json = serde_json::to_string(&printing_ids).unwrap_or_else(|_| "[]".into());
        let rows = sqlx::query!(
            r#"SELECT id AS "id!", image_uris FROM card_printings WHERE id IN (SELECT value FROM json_each(?))"#,
            ids_json
        )
        .fetch_all(&mut *conn)
        .await?;
        for row in rows {
            let images = parse_image_uris(&row.image_uris);
            printings.insert(
                row.id,
                (
                    images::url_opt(images.get("art_crop").map(String::as_str)),
                    images::url_opt(images.get("normal").map(String::as_str)),
                ),
            );
        }
    }
    Ok(ArtUrls { summaries, printings })
}

