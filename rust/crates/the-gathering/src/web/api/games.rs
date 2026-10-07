//! Placeholder handlers, replaced as the area is ported.

use crate::error::ApiError;

use super::not_implemented;

/// Not ported yet.
pub async fn admin_link_player() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn admin_players_index() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn admin_players_unlink() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn deck_chooser_outcome() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn deck_chooser_show() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn decks_create() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn decks_delete() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn decks_index() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn decks_show() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn decks_update() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn games_create() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn games_delete() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn games_index() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn games_show() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn games_summary() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn games_update() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn players_create() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn players_delete() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn players_index() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn players_merge() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn players_show() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn players_update() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn remote_decks_sync() -> ApiError {
    not_implemented()
}

/// Not ported yet.
pub async fn v1_games_index() -> ApiError {
    not_implemented()
}

// Deck lists (`DecklistController`, `DecklistJSON`, and `RemoteDeckController.index`
// with `RemoteDeckJSON`), kept in their own module so this file's imports stay untouched.
pub use self::decklist_handlers::{decklist_resolve, decklist_show, remote_decks_index};

mod decklist_handlers {
    use axum::Json;
    use axum::extract::{Path, State};
    use serde_json::{Value, json};

    use crate::catalog::{Card, Catalog, images};
    use crate::decklists::remote_decks::RemoteDeckList;
    use crate::decklists::{DeckCard, DecklistError};
    use crate::error::{ApiError, ApiResult, Errors};
    use crate::state::AppState;
    use crate::web::api::{data, parse_id};
    use crate::web::auth::AuthUser;
    use crate::web::params::Params;

    fn invalid_url() -> ApiError {
        ApiError::Validation(Errors::single("url", "is not a supported deck-list URL"))
    }

    fn card_json(entry: &DeckCard, card: Option<&Card>) -> Value {
        let (mut details, catalog_images) = match card {
            Some(card) => {
                let images: std::collections::BTreeMap<String, String> = card
                    .image_uris
                    .iter()
                    .filter(|(variant, _)| matches!(variant.as_str(), "small" | "normal"))
                    .map(|(variant, url)| (variant.clone(), url.clone()))
                    .collect();
                (
                    json!({
                        "card_id": card.id,
                        "type_line": card.type_line,
                        "mana_cost": card.mana_cost,
                        "cmc": card.cmc,
                        "game_changer": card.game_changer,
                    }),
                    images::urls(&images),
                )
            }
            None => (
                json!({"card_id": null, "type_line": null, "mana_cost": null, "cmc": null, "game_changer": false}),
                std::collections::BTreeMap::new(),
            ),
        };
        // The list's own printing, so the dialog shows the player's art; the catalog's
        // preferred printing otherwise.
        let image_uris = entry
            .printing_id
            .as_deref()
            .and_then(images::printing_urls)
            .unwrap_or(catalog_images);
        if let Some(object) = details.as_object_mut() {
            object.insert("name".into(), json!(entry.name));
            object.insert("quantity".into(), json!(entry.quantity));
            object.insert("zone".into(), json!(entry.zone.as_str()));
            object.insert("printing_id".into(), json!(entry.printing_id));
            object.insert("image_uris".into(), json!(image_uris));
        }
        details
    }

    /// `GET /api/decks/:deck_id/decklist`: the playable list behind a deck's linked
    /// Moxfield, Archidekt, or ManaVault page, with catalog type, cost, and cached images
    /// for each card. 404 when the deck has no supported link or the list is missing or
    /// private upstream.
    pub async fn decklist_show(
        State(state): State<AppState>,
        Path(deck_id): Path<String>,
    ) -> ApiResult<Json<Value>> {
        let deck_id = parse_id(&deck_id)?;
        let url = sqlx::query_scalar!("SELECT decklist_url FROM decks WHERE id = ?", deck_id)
            .fetch_optional(&state.pool)
            .await?
            .flatten()
            .ok_or(ApiError::NotFound)?;
        let decklist = match state.decklists.resolve(&url).await {
            Ok(decklist) => decklist,
            Err(DecklistError::UpstreamError) => return Err(ApiError::BadGateway),
            Err(_) => return Err(ApiError::NotFound),
        };
        let names: Vec<String> = decklist
            .cards
            .iter()
            .map(|card| card.name.clone())
            .collect();
        let catalog = Catalog {
            pool: state.pool.clone(),
        }
        .cards_by_name(&names)
        .await?;
        let cards: Vec<Value> = decklist
            .cards
            .iter()
            .map(|entry| card_json(entry, catalog.get(&entry.name)))
            .collect();
        Ok(data(json!({
            "source": decklist.source.as_str(),
            "url": decklist.url,
            "name": decklist.name,
            "fetched_at": decklist.fetched_at_iso(),
            "cards": cards,
        })))
    }

    /// `POST /api/decklists/resolve`: a deck list's public metadata.
    pub async fn decklist_resolve(
        State(state): State<AppState>,
        params: Params,
    ) -> ApiResult<Json<Value>> {
        let url = params.str("url").ok_or_else(invalid_url)?;
        match state.decklists.resolve(url).await {
            Ok(decklist) => Ok(data(decklist.to_json())),
            Err(DecklistError::InvalidUrl | DecklistError::UnsupportedUrl) => Err(invalid_url()),
            Err(DecklistError::NotFound | DecklistError::Private) => Err(ApiError::NotFound),
            Err(DecklistError::UpstreamError) => Err(ApiError::BadGateway),
        }
    }

    /// `RemoteDeckJSON.index/1`.
    pub fn remote_decks_json(result: &RemoteDeckList) -> Value {
        json!({
            "decks": result.decks.iter().map(|deck| json!({
                "name": deck.name,
                "commanders": deck.commanders,
                "color_identity": deck.color_identity,
                "url": deck.url,
                "source": deck.source.as_str(),
                "updated_at": deck.updated_at,
            })).collect::<Vec<_>>(),
            "sources": result.sources.iter().map(|source| json!({
                "source": source.source.as_str(),
                "configured": source.configured,
                "error": source.error,
            })).collect::<Vec<_>>(),
        })
    }

    /// `GET /api/session/remote-decks`: the member's decks on their configured hosts.
    pub async fn remote_decks_index(
        State(state): State<AppState>,
        AuthUser(user): AuthUser,
    ) -> Json<Value> {
        let result = state.decklists.remote.list(&user).await;
        data(remote_decks_json(&result))
    }
}
