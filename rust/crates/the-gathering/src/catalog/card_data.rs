//! Turning a Scryfall record into a catalog row (`TheGathering.Catalog.CardData`).
//!
//! Parsing, the catalog policy, the printing ranking, and the commander rules come from
//! lotus; this module only maps them onto the `cards` / `catalog_cards_staging` columns.

use std::collections::BTreeMap;

use lotus::scryfall::{ScryfallCard, SelectionKey, describes_card};
use sqlx::SqliteConnection;

use crate::db::{IsoDate, UtcDateTime};

/// One catalog row, before it is stored.
#[derive(Clone, Debug, PartialEq)]
pub struct CardData {
    /// Scryfall id of the printing.
    pub id: String,
    /// Oracle id.
    pub oracle_id: String,
    /// Printed name.
    pub name: String,
    /// [`lotus::normalize_name`] of the name.
    pub normalized_name: String,
    /// The card's or front face's mana cost.
    pub mana_cost: Option<String>,
    /// Mana value.
    pub cmc: f64,
    /// Type line.
    pub type_line: String,
    /// Oracle text; multi-faced cards join every face's text.
    pub oracle_text: String,
    /// Top-level colors.
    pub colors: Vec<String>,
    /// Color identity.
    pub color_identity: Vec<String>,
    /// `small`, `normal`, and `art_crop` image URLs of the card or its front face.
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
    /// Pairing mechanic.
    pub commander_pairing: Option<String>,
    /// [`SelectionKey`] as stored in staging: the greatest key per oracle id wins.
    pub selection_key: String,
}

/// `from_scryfall/1`: the row for a record that describes a card (not a token or
/// memorabilia set, and with an oracle id), else `None`.
///
/// Unlike the Elixir code, which stored only the top-level `oracle_text` (empty for
/// multi-faced cards), the stored text joins every face's text with lotus's
/// [`ScryfallCard::full_oracle_text`], and commander eligibility and pairing are derived
/// from it, so modal double-faced commanders and partners are recognized.
pub fn from_scryfall(card: &ScryfallCard) -> Option<CardData> {
    if !describes_card(card) {
        return None;
    }
    let oracle_id = card.oracle_id.as_ref()?.as_str().to_owned();
    let type_line = card.type_line.clone().unwrap_or_default();
    let oracle_text = card.full_oracle_text().unwrap_or_default();
    let mut image_uris = BTreeMap::new();
    if let Some(images) = card.front_image_uris() {
        for (variant, url) in [
            ("small", &images.small),
            ("normal", &images.normal),
            ("art_crop", &images.art_crop),
        ] {
            if let Some(url) = url {
                image_uris.insert(variant.to_owned(), url.clone());
            }
        }
    }
    let codes = |colors: &[lotus::Color]| {
        colors
            .iter()
            .map(|color| color.code().to_owned())
            .collect::<Vec<_>>()
    };
    Some(CardData {
        id: card.id.as_str().to_owned(),
        oracle_id,
        normalized_name: lotus::normalize_name(&card.name),
        name: card.name.clone(),
        mana_cost: card.front_mana_cost().map(str::to_owned),
        cmc: card.cmc.unwrap_or(0.0),
        can_be_commander: lotus::can_be_commander(&type_line, &oracle_text),
        commander_pairing: lotus::commander_pairing(&type_line, &oracle_text)
            .map(|pairing| pairing.as_str().to_owned()),
        type_line,
        oracle_text,
        colors: codes(card.colors.as_deref().unwrap_or_default()),
        color_identity: codes(&card.color_identity),
        image_uris,
        set_code: card.set.clone(),
        collector_number: card.collector_number.clone(),
        released_at: card.released_at.map(IsoDate),
        layout: card.layout.clone().unwrap_or_else(|| "normal".to_owned()),
        rarity: card
            .rarity
            .map_or("common", lotus::Rarity::as_str)
            .to_owned(),
        game_changer: card.game_changer,
        commander_legal: card.commander_legal(),
        selection_key: SelectionKey::of(card).to_string(),
    })
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

/// Inserts (or replaces) a row in `cards`.
pub async fn insert_card(conn: &mut SqliteConnection, card: &CardData) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    let colors = json(&card.colors);
    let identity = json(&card.color_identity);
    let images = json(&card.image_uris);
    sqlx::query!(
        "INSERT OR REPLACE INTO cards (id, oracle_id, name, normalized_name, mana_cost, cmc, type_line, oracle_text,
            colors, color_identity, image_uris, set_code, collector_number, released_at, layout, rarity,
            game_changer, commander_legal, can_be_commander, commander_pairing, inserted_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        card.id,
        card.oracle_id,
        card.name,
        card.normalized_name,
        card.mana_cost,
        card.cmc,
        card.type_line,
        card.oracle_text,
        colors,
        identity,
        images,
        card.set_code,
        card.collector_number,
        card.released_at,
        card.layout,
        card.rarity,
        card.game_changer,
        card.commander_legal,
        card.can_be_commander,
        card.commander_pairing,
        now,
        now
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Stages a row, replacing the staged printing of the same card
/// (`on_conflict: {:replace_all_except, [:oracle_id, :inserted_at]}`).
pub async fn stage_card(conn: &mut SqliteConnection, card: &CardData) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    let colors = json(&card.colors);
    let identity = json(&card.color_identity);
    let images = json(&card.image_uris);
    sqlx::query!(
        "INSERT INTO catalog_cards_staging (id, oracle_id, name, normalized_name, mana_cost, cmc, type_line,
            oracle_text, colors, color_identity, image_uris, set_code, collector_number, released_at, layout,
            rarity, game_changer, commander_legal, can_be_commander, commander_pairing, selection_key,
            inserted_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (oracle_id) DO UPDATE SET id = excluded.id, name = excluded.name,
            normalized_name = excluded.normalized_name, mana_cost = excluded.mana_cost, cmc = excluded.cmc,
            type_line = excluded.type_line, oracle_text = excluded.oracle_text, colors = excluded.colors,
            color_identity = excluded.color_identity, image_uris = excluded.image_uris,
            set_code = excluded.set_code, collector_number = excluded.collector_number,
            released_at = excluded.released_at, layout = excluded.layout, rarity = excluded.rarity,
            game_changer = excluded.game_changer, commander_legal = excluded.commander_legal,
            can_be_commander = excluded.can_be_commander, commander_pairing = excluded.commander_pairing,
            selection_key = excluded.selection_key, updated_at = excluded.updated_at",
        card.id,
        card.oracle_id,
        card.name,
        card.normalized_name,
        card.mana_cost,
        card.cmc,
        card.type_line,
        card.oracle_text,
        colors,
        identity,
        images,
        card.set_code,
        card.collector_number,
        card.released_at,
        card.layout,
        card.rarity,
        card.game_changer,
        card.commander_legal,
        card.can_be_commander,
        card.commander_pairing,
        card.selection_key,
        now,
        now
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(value: serde_json::Value) -> ScryfallCard {
        serde_json::from_value(value).unwrap()
    }

    // Ported from `test/the_gathering/catalog/card_data_test.exs`.
    #[test]
    fn copies_the_game_changer_flag_defaulting_to_false() {
        let base = json!({"id": "rhystic", "oracle_id": "oracle-rhystic", "name": "Rhystic Study"});
        let mut flagged = base.clone();
        flagged["game_changer"] = json!(true);
        assert!(from_scryfall(&card(flagged)).unwrap().game_changer);
        let mut unflagged = base.clone();
        unflagged["game_changer"] = json!(false);
        assert!(!from_scryfall(&card(unflagged)).unwrap().game_changer);
        assert!(!from_scryfall(&card(base)).unwrap().game_changer);
    }

    #[test]
    fn skips_tokens_memorabilia_and_records_without_an_oracle_id() {
        let base = json!({"id": "x", "oracle_id": "o", "name": "X", "set_type": "memorabilia"});
        assert!(from_scryfall(&card(base)).is_none());
        assert!(
            from_scryfall(&card(
                json!({"id": "x", "oracle_id": "o", "name": "X", "set_type": "token"})
            ))
            .is_none()
        );
        assert!(from_scryfall(&card(json!({"id": "x", "name": "X"}))).is_none());
    }

    #[test]
    fn maps_defaults_like_the_elixir_card_data() {
        let row = from_scryfall(&card(
            json!({"id": "x", "oracle_id": "o", "name": "Jötun’s Grunt"}),
        ))
        .unwrap();
        assert_eq!(row.normalized_name, "jotuns grunt");
        assert_eq!(row.layout, "normal");
        assert_eq!(row.rarity, "common");
        assert_eq!(row.oracle_text, "");
        assert_eq!(row.type_line, "");
        assert!((row.cmc - 0.0).abs() < f64::EPSILON);
        assert!(row.image_uris.is_empty());
        assert_eq!(row.selection_key, "0|0|1|1|0000-00-00|||x");
    }

    #[test]
    fn detects_modal_double_faced_commanders_and_partners_from_face_text() {
        let row = from_scryfall(&card(json!({
            "id": "mdfc", "oracle_id": "oracle-mdfc", "name": "Front // Back",
            "type_line": "Legendary Creature — Elf // Legendary Creature — Elf",
            "layout": "modal_dfc",
            "card_faces": [
                {"name": "Front", "oracle_text": "Partner", "image_uris": {"normal": "https://img.example/front.jpg", "png": "x"}},
                {"name": "Back", "oracle_text": "Flying"}
            ]
        })))
        .unwrap();
        assert_eq!(row.oracle_text, "Partner\n---\nFlying");
        assert!(row.can_be_commander);
        assert_eq!(row.commander_pairing.as_deref(), Some("partner"));
        assert_eq!(
            row.image_uris.get("normal").map(String::as_str),
            Some("https://img.example/front.jpg")
        );
        assert!(!row.image_uris.contains_key("png"));
    }
}
