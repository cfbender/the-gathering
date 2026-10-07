//! The `/log` web handoff (`WebGameDraft`, `SaveWebGame`): any server member gets a
//! private link to finish the game in The Gathering; reading it never creates game data,
//! and saving resolves the fixed roster, records the game, and consumes the staged game
//! atomically.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use crate::accounts::User;
use crate::db::{self, UtcDateTime};
use crate::games::{self, Game, GamesError, Resolution, ResolveError};
use crate::state::AppState;

use super::draft::{self, ResultDraft, WebDraftData};
use super::pending::{self, PendingGame};
use super::report::ReportPlayer;
use super::won_report::reference_external_id;
use super::{Actor, account_disabled, configured_guild};

/// Why a handoff link could not be made.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// No unfinished game.
    #[error("not found")]
    NotFound,
    /// Wrong server, disabled account, or no identity.
    #[error("forbidden")]
    Forbidden,
    /// The winner did not play.
    #[error("invalid winner")]
    InvalidWinner,
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// A link's preview (`DiscordResultDraftJSON.show/1`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preview {
    /// Draft UUID.
    pub id: String,
    /// `spellbot:SB12345`.
    pub external_id: String,
    /// Start time.
    pub played_at: UtcDateTime,
    /// Minutes since the start when the link was made.
    pub duration_minutes: Option<i64>,
    /// Winner chosen in Discord.
    pub winner_discord_id: Option<String>,
    /// How each roster identity would resolve.
    pub seats: Vec<PreviewSeat>,
}

/// A previewed seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreviewSeat {
    /// Discord id.
    pub discord_id: String,
    /// Existing player, if any.
    pub player_id: Option<i64>,
    /// Existing or new player name.
    pub player_name: String,
}

/// `WebGameDraft.open/3`.
pub async fn open(
    state: &AppState,
    reference: &str,
    winner: Option<&str>,
    actor: &Actor,
) -> Result<ResultDraft, OpenError> {
    let mut tx = db::begin(&state.pool).await?;
    let found = if reference.is_empty() {
        pending::latest_in_channel(&mut tx, &actor.channel_id).await?
    } else {
        pending::by_external_id(&mut tx, &reference_external_id(reference)).await?
    };
    let pending = found.ok_or(OpenError::NotFound)?;
    if actor.discord_id.is_empty()
        || actor.guild_id.is_empty()
        || actor.guild_id != pending.guild_id
        || configured_guild(state).is_some_and(|guild| guild != actor.guild_id)
        || account_disabled(&mut tx, &actor.discord_id).await? == Some(true)
    {
        return Err(OpenError::Forbidden);
    }
    if pending::recorded(&mut tx, &pending.external_id).await? {
        return Err(OpenError::NotFound);
    }
    if let Some(winner) = winner
        && !pending.has_player(winner)
    {
        return Err(OpenError::InvalidWinner);
    }
    let now = UtcDateTime::now();
    draft::delete_expired(&mut tx, now).await?;
    let data = WebDraftData {
        winner: winner.map(str::to_owned),
        duration: Some(
            (now.unix() - pending.played_at.unix())
                .div_euclid(60)
                .max(1),
        ),
    };
    let draft = ResultDraft {
        id: uuid::Uuid::new_v4().to_string(),
        pending_game_id: pending.id,
        discord_id: actor.discord_id.clone(),
        guild_id: actor.guild_id.clone(),
        channel_id: actor.channel_id.clone(),
        snapshot: pending.snapshot(),
        data: serde_json::to_string(&data).unwrap_or_default(),
        expires_at: now.plus(time::Duration::seconds(draft::LIFETIME_SECONDS)),
    };
    draft::insert(&mut tx, &draft).await?;
    tx.commit().await?;
    Ok(draft)
}

/// `WebGameDraft.load/2`: the draft and its staged game, if `user` may use it.
pub async fn load(
    conn: &mut SqliteConnection,
    id: &str,
    user: &User,
) -> Result<Option<(ResultDraft, PendingGame)>, sqlx::Error> {
    let Some(id) = draft::cast_uuid(id) else {
        return Ok(None);
    };
    let Some(draft) = draft::get(conn, &id).await? else {
        return Ok(None);
    };
    if user.disabled_at.is_some()
        || !(user.is_admin() || user.discord_id.as_deref() == Some(draft.discord_id.as_str()))
        || draft.expires_at <= UtcDateTime::now()
    {
        return Ok(None);
    }
    let Some(pending) = pending::get(conn, draft.pending_game_id).await? else {
        return Ok(None);
    };
    if draft.snapshot != pending.snapshot() || pending::recorded(conn, &pending.external_id).await?
    {
        return Ok(None);
    }
    Ok(Some((draft, pending)))
}

/// `WebGameDraft.preview/2`.
pub async fn preview(
    state: &AppState,
    id: &str,
    user: &User,
) -> Result<Option<Preview>, sqlx::Error> {
    let Some((draft, pending)) = load(&mut *state.pool.acquire().await?, id, user).await? else {
        return Ok(None);
    };
    let data: WebDraftData = serde_json::from_str(&draft.data).unwrap_or_default();
    let players = pending.players();
    let identities: Vec<(String, Option<String>)> = players
        .iter()
        .map(|player| (player.display_name.clone(), Some(player.discord_id.clone())))
        .collect();
    let resolutions = state.games.preview_player_resolutions(&identities).await?;
    let seats = players
        .iter()
        .zip(resolutions)
        .map(|(player, resolution)| match resolution {
            Resolution::Matched(found) => PreviewSeat {
                discord_id: player.discord_id.clone(),
                player_id: Some(found.id),
                player_name: found.name,
            },
            Resolution::Create(name) => PreviewSeat {
                discord_id: player.discord_id.clone(),
                player_id: None,
                player_name: name,
            },
        })
        .collect();
    Ok(Some(Preview {
        id: draft.id,
        external_id: pending.external_id,
        played_at: pending.played_at,
        duration_minutes: data.duration,
        winner_discord_id: data.winner,
        seats,
    }))
}

const GAME_FIELDS: [&str; 5] = [
    "played_at",
    "turns",
    "duration_minutes",
    "win_condition",
    "notes",
];
const SEAT_FIELDS: [&str; 4] = ["result", "kills", "mvp_card_id", "mvp_card_name"];
const DECK_FIELDS: [&str; 7] = [
    "name",
    "commander_card_id",
    "commander_name",
    "partner_card_id",
    "partner_name",
    "color_identity",
    "decklist_url",
];

fn take(attrs: &Map<String, Value>, keys: &[&str]) -> Map<String, Value> {
    keys.iter()
        .filter_map(|key| {
            attrs
                .get(*key)
                .map(|value| ((*key).to_owned(), value.clone()))
        })
        .collect()
}

/// `SaveWebGame.run/3`: the recorded game.
pub async fn save(
    state: &AppState,
    id: &str,
    user: &User,
    attrs: &Map<String, Value>,
) -> Result<Game, GamesError> {
    let mut tx = db::begin(&state.pool).await?;
    let result = save_in(&mut tx, id, user, attrs).await;
    match result {
        Ok(game) => {
            tx.commit().await?;
            Ok(game)
        }
        Err(error) => {
            tx.rollback().await?;
            Err(error)
        }
    }
}

async fn save_in(
    conn: &mut SqliteConnection,
    id: &str,
    user: &User,
    attrs: &Map<String, Value>,
) -> Result<Game, GamesError> {
    let (_draft, pending) = load(conn, id, user).await?.ok_or(GamesError::NotFound)?;
    let seats = seats(conn, &pending.players(), attrs.get("seats")).await?;
    let mut game_attrs = take(attrs, &GAME_FIELDS);
    game_attrs.insert("source".into(), json!("discord"));
    game_attrs.insert("external_id".into(), json!(pending.external_id));
    game_attrs.insert("seats".into(), Value::Array(seats));
    let game = games::record_game::create(conn, &Value::Object(game_attrs), Some(user.id)).await?;
    pending::delete(conn, pending.id).await?;
    Ok(game)
}

async fn seats(
    conn: &mut SqliteConnection,
    roster: &[ReportPlayer],
    seats: Option<&Value>,
) -> Result<Vec<Value>, GamesError> {
    let seats = seats
        .and_then(Value::as_array)
        .ok_or(GamesError::BadRequest)?;
    let by_id: HashMap<&str, &ReportPlayer> = roster
        .iter()
        .map(|player| (player.discord_id.as_str(), player))
        .collect();
    let mut given: Vec<Option<&str>> = seats
        .iter()
        .map(|seat| seat.get("discord_id").and_then(Value::as_str))
        .collect();
    let mut expected: Vec<Option<&str>> = by_id.keys().copied().map(Some).collect();
    given.sort_unstable();
    expected.sort_unstable();
    if given != expected {
        return Err(GamesError::BadRequest);
    }
    let mut resolved = Vec::with_capacity(seats.len());
    for (index, seat) in seats.iter().enumerate() {
        let seat = seat.as_object().ok_or(GamesError::BadRequest)?;
        let identity = seat
            .get("discord_id")
            .and_then(Value::as_str)
            .and_then(|id| by_id.get(id))
            .ok_or(GamesError::BadRequest)?;
        let player = games::resolve_player::run(
            conn,
            &identity.display_name,
            Some(&identity.discord_id),
            None,
        )
        .await
        .map_err(|error| match error {
            ResolveError::Invalid(errors) => GamesError::Invalid(errors),
            ResolveError::Database(error) => GamesError::Database(error),
            ResolveError::DiscordIdentityConflict => GamesError::BadRequest,
        })?;
        let deck_id = deck(conn, player.id, seat).await?;
        let mut attrs = take(seat, &SEAT_FIELDS);
        attrs.insert("player_id".into(), json!(player.id));
        attrs.insert("deck_id".into(), json!(deck_id));
        attrs.insert("seat".into(), json!(index + 1));
        resolved.push(Value::Object(attrs));
    }
    Ok(resolved)
}

/// `Ecto.Type.cast(:id, value)`.
fn cast_id(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

async fn deck(
    conn: &mut SqliteConnection,
    player_id: i64,
    seat: &Map<String, Value>,
) -> Result<Option<i64>, GamesError> {
    if let Some(id) = seat.get("deck_id").filter(|value| !value.is_null()) {
        let id = cast_id(id).ok_or(GamesError::BadRequest)?;
        return match games::get_deck(conn, id).await? {
            Some(deck) if deck.player_id == player_id => Ok(Some(id)),
            _ => Err(GamesError::BadRequest),
        };
    }
    match seat.get("deck") {
        Some(Value::Object(attrs)) if attrs.get("name").is_some_and(Value::is_string) => {
            let name = attrs
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let fields: Map<String, Value> = DECK_FIELDS
                .iter()
                .map(|key| {
                    (
                        (*key).to_owned(),
                        attrs.get(*key).cloned().unwrap_or(Value::Null),
                    )
                })
                .collect();
            let deck =
                games::deck::find_or_create_deck(conn, player_id, name, &Value::Object(fields))
                    .await?;
            Ok(Some(deck.id))
        }
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(_) => Err(GamesError::BadRequest),
    }
}
