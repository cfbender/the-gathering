//! The `/log` web handoff (`WebGameDraft`, `SaveWebGame`): any server member gets a
//! private link to finish the game in The Gathering; reading it never creates game data,
//! and saving resolves the fixed roster, records the game, and consumes the staged game
//! atomically.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::accounts::User;
use crate::db::{self, UtcDateTime};
use crate::games::{
    self, DeckInput, Game, GameInput, GamesError, Resolution, ResolveError, SeatInput,
};
use crate::patch::Patch;
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

/// The finished result a member submits for a staged game.
#[derive(Clone, Debug, Deserialize)]
pub struct DraftResult {
    /// When it was played.
    #[serde(default)]
    pub played_at: Patch<UtcDateTime>,
    /// Number of turns.
    #[serde(default)]
    pub turns: Patch<i64>,
    /// Length in minutes.
    #[serde(default)]
    pub duration_minutes: Patch<i64>,
    /// How it was won.
    #[serde(default)]
    pub win_condition: Patch<String>,
    /// Notes.
    #[serde(default)]
    pub notes: Patch<String>,
    /// One seat per staged player, in turn order.
    pub seats: Vec<DraftSeat>,
}

/// A staged player's seat.
#[derive(Clone, Debug, Deserialize)]
pub struct DraftSeat {
    /// The staged player.
    pub discord_id: String,
    /// One of their existing decks.
    #[serde(default)]
    pub deck_id: Option<i64>,
    /// A deck to find or create for them, when `deck_id` is not given.
    #[serde(default)]
    pub deck: Option<DraftDeck>,
    /// `win`, `loss`, or `draw`.
    #[serde(default)]
    pub result: Patch<String>,
    /// Kills.
    #[serde(default)]
    pub kills: Patch<i64>,
    /// Most valuable card.
    #[serde(default)]
    pub mvp_card_id: Patch<String>,
    /// Its name.
    #[serde(default)]
    pub mvp_card_name: Patch<String>,
}

/// A deck named in a draft seat.
#[derive(Clone, Debug, Deserialize)]
pub struct DraftDeck {
    /// Deck name.
    pub name: String,
    /// Commander card id.
    #[serde(default)]
    pub commander_card_id: Option<String>,
    /// Commander name.
    #[serde(default)]
    pub commander_name: Option<String>,
    /// Partner card id.
    #[serde(default)]
    pub partner_card_id: Option<String>,
    /// Partner name.
    #[serde(default)]
    pub partner_name: Option<String>,
    /// Color identity.
    #[serde(default)]
    pub color_identity: Option<String>,
    /// Deck-list link.
    #[serde(default)]
    pub decklist_url: Option<String>,
}

/// `SaveWebGame.run/3`: the recorded game.
pub async fn save(
    state: &AppState,
    id: &str,
    user: &User,
    result: &DraftResult,
) -> Result<Game, GamesError> {
    let mut tx = db::begin(&state.pool).await?;
    let result = save_in(&mut tx, state.games.deck_links(), id, user, result).await;
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
    links: &games::DeckLinks,
    id: &str,
    user: &User,
    result: &DraftResult,
) -> Result<Game, GamesError> {
    let (_draft, pending) = load(conn, id, user).await?.ok_or(GamesError::NotFound)?;
    let seats = seats(conn, links, &pending.players(), &result.seats).await?;
    let input = GameInput {
        played_at: result.played_at.clone(),
        turns: result.turns.clone(),
        duration_minutes: result.duration_minutes.clone(),
        win_condition: result.win_condition.clone(),
        notes: result.notes.clone(),
        seats: Some(seats).into(),
        source: Some("discord".to_owned()),
        external_id: Some(pending.external_id.clone()),
        ..GameInput::default()
    };
    let game = games::record_game::create(conn, &input, Some(user.id)).await?;
    pending::delete(conn, pending.id).await?;
    Ok(game)
}

async fn seats(
    conn: &mut SqliteConnection,
    links: &games::DeckLinks,
    roster: &[ReportPlayer],
    seats: &[DraftSeat],
) -> Result<Vec<SeatInput>, GamesError> {
    let by_id: HashMap<&str, &ReportPlayer> = roster
        .iter()
        .map(|player| (player.discord_id.as_str(), player))
        .collect();
    let mut given: Vec<&str> = seats.iter().map(|seat| seat.discord_id.as_str()).collect();
    let mut expected: Vec<&str> = by_id.keys().copied().collect();
    given.sort_unstable();
    expected.sort_unstable();
    if given != expected {
        return Err(GamesError::BadRequest);
    }
    let mut resolved = Vec::with_capacity(seats.len());
    for (index, seat) in seats.iter().enumerate() {
        let identity = by_id
            .get(seat.discord_id.as_str())
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
        let deck_id = deck(conn, links, player.id, seat).await?;
        resolved.push(SeatInput {
            player_id: Some(player.id).into(),
            deck_id: deck_id.into(),
            seat: i64::try_from(index + 1).ok().into(),
            result: seat.result.clone(),
            kills: seat.kills.clone(),
            mvp_card_id: seat.mvp_card_id.clone(),
            mvp_card_name: seat.mvp_card_name.clone(),
            ..SeatInput::default()
        });
    }
    Ok(resolved)
}

async fn deck(
    conn: &mut SqliteConnection,
    links: &games::DeckLinks,
    player_id: i64,
    seat: &DraftSeat,
) -> Result<Option<i64>, GamesError> {
    if let Some(id) = seat.deck_id {
        return match games::get_deck(conn, id).await? {
            Some(deck) if deck.player_id == player_id => Ok(Some(id)),
            _ => Err(GamesError::BadRequest),
        };
    }
    let Some(deck) = &seat.deck else {
        return Ok(None);
    };
    let input = DeckInput {
        name: Some(deck.name.clone()).into(),
        commander_card_id: deck.commander_card_id.clone().into(),
        commander_name: deck.commander_name.clone().into(),
        partner_card_id: deck.partner_card_id.clone().into(),
        partner_name: deck.partner_name.clone().into(),
        color_identity: deck.color_identity.clone().into(),
        decklist_url: deck.decklist_url.clone().into(),
        ..DeckInput::default()
    };
    let deck = games::deck::find_or_create_deck(conn, links, player_id, &deck.name, &input).await?;
    Ok(Some(deck.id))
}
