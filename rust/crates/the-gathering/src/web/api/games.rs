//! Players, decks, games, the deck chooser, player administration, and the API-key game
//! history (`PlayerController`, `DeckController`, `GameController`,
//! `DeckChooserController`, `AdminPlayerController`, `AdminUserController.link_player`,
//! `V1.GameController`) with their JSON views.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

use crate::accounts::User;
use crate::catalog::{ArtUrls, CardRef};
use crate::changeset::cast_integer;
use crate::error::{ApiError, ApiResult};
use crate::games::{
    self, Deck, DeckPick, Game, Outcome, Player, PlayerDetail, RenderError, Seat, SeatGame,
    can_manage_player,
};
use crate::local_time::{Zone, parse_date};
use crate::state::AppState;
use crate::web::auth::AuthUser;
use crate::web::params::Params;

use super::{data, parse_id};

// JSON views

/// `PlayerJSON.summary/1`.
pub fn player_summary(player: &Player) -> Value {
    json!({
        "id": player.id,
        "name": player.name,
        "avatar_url": player.avatar_url,
        "user_id": player.user_id,
        "archived_at": player.archived_at,
    })
}

/// `DeckJSON.card_refs/1` for one deck.
pub fn deck_card_refs(deck: &Deck) -> Vec<CardRef> {
    vec![
        CardRef::Card(
            deck.commander_card_id.clone(),
            Some(deck.commander_name.clone()),
        ),
        CardRef::Card(deck.partner_card_id.clone(), deck.partner_name.clone()),
        CardRef::Printing(deck.commander_printing_id.clone()),
        CardRef::Printing(deck.partner_printing_id.clone()),
    ]
}

/// `DeckJSON.summary/2`; `player` is the preloaded owner, when loaded.
pub fn deck_summary(deck: &Deck, player: Option<&Player>, art: &ArtUrls) -> Value {
    let commander = (
        deck.commander_card_id.as_deref(),
        Some(deck.commander_name.as_str()),
    );
    let partner = (
        deck.partner_card_id.as_deref(),
        deck.partner_name.as_deref(),
    );
    json!({
        "id": deck.id,
        "player_id": deck.player_id,
        "name": deck.name,
        "commander_card_id": deck.commander_card_id,
        "commander_name": deck.commander_name,
        "commander_game_changer": art.game_changer(commander.0, commander.1),
        "commander_printing_id": deck.commander_printing_id,
        "commander_image_url": art.card_image_url(commander.0, commander.1, deck.commander_printing_id.as_deref()),
        "commander_art_crop_url": art.art_crop_url(commander.0, commander.1, deck.commander_printing_id.as_deref()),
        "partner_card_id": deck.partner_card_id,
        "partner_name": deck.partner_name,
        "partner_game_changer": art.game_changer(partner.0, partner.1),
        "partner_printing_id": deck.partner_printing_id,
        "partner_image_url": art.card_image_url(partner.0, partner.1, deck.partner_printing_id.as_deref()),
        "partner_art_crop_url": art.art_crop_url(partner.0, partner.1, deck.partner_printing_id.as_deref()),
        "color_identity": deck.color_identity,
        "decklist_url": deck.decklist_url,
        "decklist_source": deck.decklist_source,
        "archived_at": deck.archived_at,
        "skip_count": deck.skip_count,
        "included_for_play": deck.included_for_play,
        "player": player.map_or(Value::Null, player_summary),
    })
}

/// `GameJSON.seat_game/2`.
pub fn seat_game(seat: &SeatGame, art: &ArtUrls) -> Value {
    json!({
        "id": seat.game_id,
        "played_at": seat.played_at,
        "format": seat.format,
        "result": seat.result,
        "deck": seat.deck.as_ref().map_or(Value::Null, |deck| deck_summary(deck, None, art)),
    })
}

/// `GameJSON.card_refs/1`.
pub fn game_card_refs<'a>(games: impl IntoIterator<Item = &'a Game>) -> Vec<CardRef> {
    games
        .into_iter()
        .flat_map(|game| &game.seats)
        .flat_map(|seat| {
            let mut refs = vec![CardRef::Card(
                seat.mvp_card_id.clone(),
                seat.mvp_card_name.clone(),
            )];
            if let Some(deck) = &seat.deck {
                refs.extend(deck_card_refs(deck));
            }
            refs
        })
        .collect()
}

/// `GameJSON.seat/2`.
pub fn seat_json(seat: &Seat, art: &ArtUrls) -> Value {
    let mvp = (seat.mvp_card_id.as_deref(), seat.mvp_card_name.as_deref());
    json!({
        "id": seat.id,
        "player_id": seat.player_id,
        "deck_id": seat.deck_id,
        "seat": seat.seat,
        "result": seat.result,
        "kills": seat.kills,
        "eliminated_turn": seat.eliminated_turn,
        "eliminated_by_player_id": seat.eliminated_by_player_id,
        "mvp_card_id": seat.mvp_card_id,
        "mvp_card_name": seat.mvp_card_name,
        "mvp_game_changer": art.game_changer(mvp.0, mvp.1),
        "mvp_image_url": art.card_image_url(mvp.0, mvp.1, None),
        "mvp_art_crop_url": art.art_crop_url(mvp.0, mvp.1, None),
        "notes": seat.notes,
        "player": player_summary(&seat.player),
        "deck": seat.deck.as_ref().map_or(Value::Null, |deck| deck_summary(deck, None, art)),
    })
}

/// `GameJSON.game/2`.
pub fn game_json(game: &Game, art: &ArtUrls) -> Value {
    let mut seats: Vec<&Seat> = game.seats.iter().collect();
    seats.sort_by_key(|seat| seat.seat);
    json!({
        "id": game.id,
        "played_at": game.played_at,
        "duration_minutes": game.duration_minutes,
        "turns": game.turns,
        "win_condition": game.win_condition,
        "notes": game.notes,
        "source": game.source,
        "format": game.format,
        "external_id": game.external_id,
        "created_by_user_id": game.created_by_user_id,
        "seats": seats.into_iter().map(|seat| seat_json(seat, art)).collect::<Vec<_>>(),
    })
}

async fn art_urls(state: &AppState, refs: &[CardRef]) -> ApiResult<ArtUrls> {
    Ok(crate::catalog::art_crop_urls_in(&mut *state.pool.acquire().await?, refs).await?)
}

/// `PlayerJSON.card_refs/1`.
fn player_card_refs(detail: &PlayerDetail) -> Vec<CardRef> {
    detail
        .decks
        .iter()
        .flat_map(deck_card_refs)
        .chain(
            detail
                .seats
                .iter()
                .filter_map(|seat| seat.deck.as_ref())
                .flat_map(deck_card_refs),
        )
        .collect()
}

/// `PlayerJSON.show/1` for a player id.
async fn render_player(state: &AppState, id: i64) -> ApiResult<Value> {
    let detail = state
        .games
        .get_player_detail(id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let art = art_urls(state, &player_card_refs(&detail)).await?;
    let mut body = player_summary(&detail.player);
    if let Value::Object(object) = &mut body {
        object.insert("discord_id".into(), json!(detail.player.discord_id));
        object.insert("games_played".into(), json!(detail.seats.len()));
        let wins = detail
            .seats
            .iter()
            .filter(|seat| seat.result == games::GameResult::Win)
            .count();
        object.insert("wins".into(), json!(wins));
        object.insert(
            "decks".into(),
            Value::Array(
                detail
                    .decks
                    .iter()
                    .map(|deck| deck_summary(deck, None, &art))
                    .collect(),
            ),
        );
        object.insert(
            "recent_games".into(),
            Value::Array(
                detail
                    .seats
                    .iter()
                    .take(10)
                    .map(|seat| seat_game(seat, &art))
                    .collect(),
            ),
        );
    }
    Ok(json!({ "data": body }))
}

/// `DeckJSON.show/1` for a deck id.
async fn render_deck(state: &AppState, id: i64) -> ApiResult<Value> {
    let deck = state.games.get_deck(id).await?.ok_or(ApiError::NotFound)?;
    let mut conn = state.pool.acquire().await?;
    let player = games::get_player(&mut conn, deck.player_id).await?;
    let seats = games::deck::deck_seat_games(&mut conn, deck.id).await?;
    drop(conn);
    let art = art_urls(state, &deck_card_refs(&deck)).await?;
    let mut body = deck_summary(&deck, player.as_ref(), &art);
    if let Value::Object(object) = &mut body {
        object.insert("games_played".into(), json!(seats.len()));
        let wins = seats
            .iter()
            .filter(|seat| seat.result == games::GameResult::Win)
            .count();
        object.insert("wins".into(), json!(wins));
        object.insert(
            "recent_games".into(),
            Value::Array(
                seats
                    .iter()
                    .take(10)
                    .map(|seat| seat_game(seat, &art))
                    .collect(),
            ),
        );
    }
    Ok(json!({ "data": body }))
}

/// `GameJSON.show/1` (also rendered by the Discord result draft controller).
pub async fn render_game(state: &AppState, game: &Game) -> ApiResult<Value> {
    let art = art_urls(state, &game_card_refs([game])).await?;
    Ok(json!({ "data": game_json(game, &art) }))
}

async fn render_games(state: &AppState, params: &Value) -> ApiResult<Json<Value>> {
    let (games, pagination) = state.games.list_games(params).await?;
    let art = art_urls(state, &game_card_refs(&games)).await?;
    Ok(Json(json!({
        "data": games.iter().map(|game| game_json(game, &art)).collect::<Vec<_>>(),
        "pagination": pagination,
    })))
}

fn take(attrs: &Value, keys: &[&str]) -> Value {
    let object = attrs.as_object();
    Value::Object(
        keys.iter()
            .filter_map(|key| {
                object
                    .and_then(|object| object.get(*key))
                    .map(|value| ((*key).to_owned(), value.clone()))
            })
            .collect::<Map<String, Value>>(),
    )
}

/// Ecto's `:id` cast of a path or body value: integers or numeric strings, else a 400.
fn cast_id(value: &Value) -> ApiResult<i64> {
    cast_integer(value).ok_or(ApiError::BadRequest)
}

fn include_archived(params: &Params) -> bool {
    // `maybe_active/2` only skips the filter for a literal `true`, never a query string.
    params.get("include_archived") == Some(&Value::Bool(true))
}

// PlayerController

/// Account and Discord identity are linked by trusted OAuth/import/admin paths only.
const PLAYER_MEMBER_ATTRS: &[&str] = &["name", "archived_at"];

/// `GET /api/players`.
pub async fn players_index(
    State(state): State<AppState>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let players = state.games.list_players(include_archived(&params)).await?;
    Ok(data(players.iter().map(player_summary).collect::<Vec<_>>()))
}

/// `POST /api/players`.
pub async fn players_create(State(state): State<AppState>, params: Params) -> ApiResult<Response> {
    let attrs = params.object("player").ok_or(ApiError::BadRequest)?;
    let player = state
        .games
        .create_player(&take(attrs, PLAYER_MEMBER_ATTRS), None)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(render_player(&state, player.id).await?),
    )
        .into_response())
}

/// `GET /api/players/:id`.
pub async fn players_show(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(render_player(&state, parse_id(&id)?).await?))
}

async fn manageable_player(state: &AppState, user: &User, id: &str) -> ApiResult<Player> {
    let player = state
        .games
        .get_player(parse_id(id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    if can_manage_player(user, &player) {
        Ok(player)
    } else {
        Err(ApiError::Forbidden)
    }
}

/// `PATCH /api/players/:id`.
pub async fn players_update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let attrs = params.object("player").ok_or(ApiError::BadRequest)?;
    let player = manageable_player(&state, &user, &id).await?;
    let player = state
        .games
        .update_player(&player, &take(attrs, PLAYER_MEMBER_ATTRS))
        .await?;
    Ok(Json(render_player(&state, player.id).await?))
}

/// `POST /api/players/:id/merge`: folds the player into `target_id`.
pub async fn players_merge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let target_id = params.get("target_id").ok_or(ApiError::BadRequest)?;
    let source = state
        .games
        .get_player(parse_id(&id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    let target = state
        .games
        .get_player(cast_id(target_id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    let merged = state.games.merge_players(&source, &target).await?;
    Ok(Json(render_player(&state, merged.id).await?))
}

/// `DELETE /api/players/:id`.
pub async fn players_delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let player = manageable_player(&state, &user, &id).await?;
    state.games.delete_player(&player).await?;
    Ok(StatusCode::NO_CONTENT)
}

// DeckController

/// `GET /api/decks` (`player_id` filters by owner).
pub async fn decks_index(State(state): State<AppState>, params: Params) -> ApiResult<Json<Value>> {
    let player_id = match params.get("player_id") {
        None | Some(Value::Null) => None,
        Some(value) => Some(cast_id(value)?),
    };
    let decks = state
        .games
        .list_decks(include_archived(&params), player_id)
        .await?;
    let refs: Vec<CardRef> = decks
        .iter()
        .flat_map(|(deck, _)| deck_card_refs(deck))
        .collect();
    let art = art_urls(&state, &refs).await?;
    Ok(data(
        decks
            .iter()
            .map(|(deck, player)| deck_summary(deck, Some(player), &art))
            .collect::<Vec<_>>(),
    ))
}

/// Creating a deck for another member's player is refused; an unknown player id falls
/// through to the changeset's error.
async fn authorize_owner(state: &AppState, user: &User, attrs: &Value) -> ApiResult<()> {
    let Some(player_id) = attrs
        .get("player_id")
        .filter(|value| !value.is_null())
        .and_then(cast_integer)
    else {
        return Ok(());
    };
    match state.games.get_player(player_id).await? {
        Some(player) if !can_manage_player(user, &player) => Err(ApiError::Forbidden),
        _ => Ok(()),
    }
}

/// `POST /api/decks`.
pub async fn decks_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Response> {
    let attrs = params.object("deck").ok_or(ApiError::BadRequest)?;
    authorize_owner(&state, &user, attrs).await?;
    let deck = state.games.create_deck(attrs).await?;
    Ok((
        StatusCode::CREATED,
        Json(render_deck(&state, deck.id).await?),
    )
        .into_response())
}

/// `GET /api/decks/:id`.
pub async fn decks_show(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(render_deck(&state, parse_id(&id)?).await?))
}

async fn manageable_deck(state: &AppState, user: &User, id: &str) -> ApiResult<Deck> {
    let deck = state
        .games
        .get_deck(parse_id(id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    if state.games.can_manage_deck(user, &deck).await? {
        Ok(deck)
    } else {
        Err(ApiError::Forbidden)
    }
}

/// `PATCH /api/decks/:id`.
pub async fn decks_update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let attrs = params.object("deck").ok_or(ApiError::BadRequest)?;
    let deck = manageable_deck(&state, &user, &id).await?;
    let deck = state.games.update_deck(&deck, attrs).await?;
    Ok(Json(render_deck(&state, deck.id).await?))
}

/// `DELETE /api/decks/:id`: `replacement_deck_id` moves the deck's games to another of the
/// player's decks; without it those seats keep no deck.
pub async fn decks_delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<StatusCode> {
    let deck = manageable_deck(&state, &user, &id).await?;
    let replacement = match params.get("replacement_deck_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.is_empty() => None,
        Some(value) => Some(
            state
                .games
                .get_deck(cast_id(value)?)
                .await?
                .ok_or(ApiError::BadRequest)?,
        ),
    };
    state.games.delete_deck(&deck, replacement.as_ref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

// GameController

const GAME_MEMBER_ATTRS: &[&str] = &[
    "played_at",
    "duration_minutes",
    "turns",
    "win_condition",
    "notes",
    "seats",
    "format",
];

/// `GET /api/games`: filtered and paginated (`ListGames`).
pub async fn games_index(State(state): State<AppState>, params: Params) -> ApiResult<Json<Value>> {
    render_games(&state, &params.0).await
}

/// `POST /api/games`: the creator comes from the session; provenance cannot be set.
pub async fn games_create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Response> {
    let attrs = params.object("game").ok_or(ApiError::BadRequest)?;
    let game = state
        .games
        .create_game(&take(attrs, GAME_MEMBER_ATTRS), Some(user.id))
        .await?;
    Ok((StatusCode::CREATED, Json(render_game(&state, &game).await?)).into_response())
}

/// `GET /api/games/:id`.
pub async fn games_show(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let game = state
        .games
        .get_game(parse_id(&id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(render_game(&state, &game).await?))
}

/// `GET /api/games/:id/summary`: the summary card PNG (by id, `SB…` SpellBot id).
pub async fn games_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let game = state.games.find_summary_game(&id).await?;
    let png = match games::render_summary(&state, &game).await {
        Ok(png) => png,
        Err(RenderError::Database(error)) => return Err(error.into()),
        Err(error) => {
            tracing::warn!("summary render failed: {error}");
            return Err(ApiError::BadGateway);
        }
    };
    let mut response = (StatusCode::OK, png).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

async fn manageable_game(state: &AppState, user: &User, id: &str) -> ApiResult<Game> {
    let game = state
        .games
        .get_game(parse_id(id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    if state.games.can_manage_game(user, &game).await? {
        Ok(game)
    } else {
        Err(ApiError::Forbidden)
    }
}

/// `PATCH /api/games/:id`.
pub async fn games_update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let attrs = params.object("game").ok_or(ApiError::BadRequest)?;
    let game = manageable_game(&state, &user, &id).await?;
    let game = state
        .games
        .update_game(&game, &take(attrs, GAME_MEMBER_ATTRS))
        .await?;
    Ok(Json(render_game(&state, &game).await?))
}

/// `DELETE /api/games/:id`.
pub async fn games_delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let game = manageable_game(&state, &user, &id).await?;
    state.games.delete_game(&game).await?;
    Ok(StatusCode::NO_CONTENT)
}

// DeckChooserController

/// `GET /api/deck-chooser` (`exclude_id` skips the previous suggestion).
pub async fn deck_chooser_show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Json<Value>> {
    let random: f64 = rand::random();
    let pick = state
        .games
        .pick_deck(&user, params.get("exclude_id"), random)
        .await?;
    Ok(match pick {
        DeckPick::PlayerNotLinked => data(json!({ "deck": null, "reason": "player_not_linked" })),
        DeckPick::NoEligibleDecks => data(json!({ "deck": null, "reason": "no_eligible_decks" })),
        DeckPick::Picked(candidate) => {
            let art = art_urls(&state, &deck_card_refs(&candidate.deck)).await?;
            data(json!({
                "deck": deck_summary(&candidate.deck, None, &art),
                "play_count": candidate.play_count,
                "skip_count": candidate.deck.skip_count,
                "last_played_at": candidate.last_played_at,
                "reason": null,
            }))
        }
    })
}

/// `POST /api/deck-chooser/:id/outcomes` (`outcome`: `played` or `skipped`).
pub async fn deck_chooser_outcome(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let outcome = params
        .str("outcome")
        .and_then(Outcome::parse)
        .ok_or(ApiError::BadRequest)?;
    let deck = state
        .games
        .record_deck_outcome(&user, parse_id(&id)?, outcome)
        .await?;
    Ok(data(
        json!({ "deck_id": deck.id, "outcome": outcome.as_str(), "skip_count": deck.skip_count }),
    ))
}

// AdminPlayerController

/// `GET /api/admin/players` (`page`, `per_page`, `search`).
pub async fn admin_players_index(
    State(state): State<AppState>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let (players, meta) = state.games.list_player_identities(&params.0).await?;
    let rows: Vec<Value> = players
        .iter()
        .map(|row| {
            json!({
                "id": row.player.id,
                "name": row.player.name,
                "discord_id": row.player.discord_id,
                "archived_at": row.player.archived_at,
                "user": row.user.as_ref().map(|(id, username)| json!({ "id": id, "username": username })),
            })
        })
        .collect();
    Ok(Json(json!({ "data": rows, "meta": meta })))
}

/// `DELETE /api/admin/players/:id/identity`: detaches the account and Discord identity.
pub async fn admin_players_unlink(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let player = state
        .games
        .get_player(parse_id(&id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    state.games.unlink_player_identity(&player).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/admin/users/:id/player` (`AdminUserController.link_player`): makes `player_id`
/// the account's player.
pub async fn admin_link_player(
    State(state): State<AppState>,
    Path(id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let player_id = params.get("player_id").ok_or(ApiError::BadRequest)?;
    let user = state
        .accounts
        .get_user(parse_id(&id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    let player = state
        .games
        .get_player(cast_id(player_id)?)
        .await?
        .ok_or(ApiError::NotFound)?;
    let linked = state.games.link_player_to_user(&player, &user).await?;
    Ok(Json(render_player(&state, linked.id).await?))
}

// V1.GameController

const V1_FILTERS: &[&str] = &[
    "player_id",
    "date_from",
    "date_to",
    "tz",
    "page",
    "per_page",
];

fn valid_positive(params: &Value, key: &str) -> ApiResult<()> {
    match params.get(key) {
        None => Ok(()),
        Some(Value::String(text)) if text.parse::<i64>().is_ok_and(|value| value > 0) => Ok(()),
        Some(_) => Err(ApiError::BadRequest),
    }
}

fn valid_date(params: &Value, key: &str) -> ApiResult<()> {
    match params.get(key) {
        None => Ok(()),
        Some(Value::String(text)) if parse_date(text).is_some() => Ok(()),
        Some(_) => Err(ApiError::BadRequest),
    }
}

/// `GET /api/v1/games`: game history for API-key clients, newest first. Accepts
/// `player_id` (an id, or `me` for the key owner's player), inclusive `date_from`/`date_to`
/// read in `tz`, and `page`/`per_page`. Invalid filters are rejected rather than ignored so
/// a typo never silently widens the result.
pub async fn v1_games_index(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    params: Params,
) -> ApiResult<Json<Value>> {
    let mut filters = take(&params.0, V1_FILTERS);
    if filters.get("player_id").and_then(Value::as_str) == Some("me") {
        let player = state
            .games
            .get_player_for_user(user.id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if let Value::Object(object) = &mut filters {
            object.insert("player_id".into(), Value::String(player.id.to_string()));
        }
    }
    for key in ["player_id", "page", "per_page"] {
        valid_positive(&filters, key)?;
    }
    valid_date(&filters, "date_from")?;
    valid_date(&filters, "date_to")?;
    // `LocalTime.zone/1` falls back to UTC, which would quietly shift the date window.
    if let Some(zone) = filters.get("tz") {
        let valid = zone
            .as_str()
            .is_some_and(|zone| Zone::parse(Some(zone)).name() == zone);
        if !valid {
            return Err(ApiError::BadRequest);
        }
    }
    render_games(&state, &filters).await
}

// Owned by other areas; kept at the bottom to ease merging.

/// `POST /api/session/remote-decks/sync` (`RemoteDeckController.sync/2` with
/// `RemoteDeckJSON.sync/1`): folds the member's hosted decks into their player's decks.
pub async fn remote_decks_sync(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let result = games::sync_remote_decks::run(&state, &user).await?;
    Ok(data(json!({
        "created": result.created,
        "updated": result.updated,
        "errors": result.errors.iter().map(|failure| json!({
            "source": failure.source.as_str(),
            "error": failure.error,
        })).collect::<Vec<_>>(),
    })))
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
