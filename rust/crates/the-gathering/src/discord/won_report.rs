//! Participant-scoped `/won` result drafts; only confirmation consumes a staged game
//! (`WonReport`).

use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::db::{self, UtcDateTime};
use crate::state::AppState;

use super::card_choice::{self, reportable_conditions};
use super::draft::{self, ResultDraft, Role, WonDraftData};
use super::pending::{self, PendingGame};
use super::report::ReportPlayer;
use super::sink::{GamesSink, Sink};
use super::{Actor, account_disabled, configured_guild};

/// The error shown for drafts that cannot be used.
pub const UNUSABLE: &str = "This draft expired, changed, or is not yours. Run /won again.";

/// A usable draft with its staged game.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The draft row.
    pub draft: ResultDraft,
    /// Its decoded data.
    pub data: WonDraftData,
    /// The staged game.
    pub pending: PendingGame,
}

impl Loaded {
    /// The players in seat order.
    pub fn players(&self) -> Vec<ReportPlayer> {
        self.pending.players()
    }
}

/// Form input: the selected value (components) or the submitted fields (modals).
pub type Values = BTreeMap<String, Option<String>>;

/// A form action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Commander modal submitted for a player.
    Commander(String),
    /// A commander or partner candidate picked for a player.
    ChooseCommander(String, Role),
    /// Details modal submitted.
    Details,
    /// Kills modal page submitted.
    Kills(usize),
    /// Winner selected.
    Winner,
    /// Win condition selected.
    Condition,
    /// MVP candidate selected.
    Mvp,
    /// Save pressed.
    Save,
    /// Cancel pressed.
    Cancel,
}

/// What an action did.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// The draft changed.
    Updated(Loaded),
    /// Saving was refused; show the review with this error.
    Invalid(Loaded, String),
    /// Recorded; the staged game's external id.
    Saved(String),
    /// The draft was discarded.
    Cancelled,
}

/// The players on each kills page (five per modal).
pub fn kills_page(players: &[ReportPlayer], page: usize) -> Vec<ReportPlayer> {
    players
        .chunks(5)
        .nth(page)
        .map(<[_]>::to_vec)
        .unwrap_or_default()
}

/// `SB12345`, `#sb12345`, or `12345` → `spellbot:SB12345`.
pub fn reference_external_id(reference: &str) -> String {
    let upper = reference.to_uppercase();
    let id = upper.trim_start_matches('#').trim_start_matches("SB");
    format!("spellbot:SB{id}")
}

async fn authorize(
    conn: &mut SqliteConnection,
    state: &AppState,
    pending: Option<&PendingGame>,
    actor: &Actor,
) -> Result<Result<(), String>, sqlx::Error> {
    let Some(pending) = pending else {
        return Ok(Err(
            "I haven't seen an unfinished SpellBot game here. Use /won game:SB12345 to choose one."
                .into(),
        ));
    };
    if actor.guild_id.is_empty() || actor.guild_id != pending.guild_id {
        return Ok(Err("Use /won in the game's server.".into()));
    }
    if configured_guild(state).is_some_and(|guild| guild != actor.guild_id) {
        return Ok(Err("Use /won in the bot's configured server.".into()));
    }
    if account_disabled(conn, &actor.discord_id).await? == Some(true) {
        return Ok(Err("Your account is disabled.".into()));
    }
    if !pending.has_player(&actor.discord_id) {
        return Ok(Err("Only a participant can report this game.".into()));
    }
    if pending::recorded(conn, &pending.external_id).await? {
        return Ok(Err(
            "This game has already been recorded. Edit it in The Gathering instead.".into(),
        ));
    }
    Ok(Ok(()))
}

/// Starts a draft for the referenced (or latest) staged game.
pub async fn open(
    state: &AppState,
    reference: &str,
    actor: &Actor,
) -> Result<Result<Loaded, String>, sqlx::Error> {
    let mut tx = db::begin(&state.pool).await?;
    let found = if reference.is_empty() {
        pending::latest_in_channel(&mut tx, &actor.channel_id).await?
    } else {
        pending::by_external_id(&mut tx, &reference_external_id(reference)).await?
    };
    if let Err(error) = authorize(&mut tx, state, found.as_ref(), actor).await? {
        return Ok(Err(error));
    }
    let Some(pending) = found else {
        return Ok(Err(UNUSABLE.into()));
    };
    let now = UtcDateTime::now();
    draft::delete_expired(&mut tx, now).await?;
    let minutes = (now.unix() - pending.played_at.unix())
        .div_euclid(60)
        .max(1);
    let data = WonDraftData {
        winner: Some(actor.discord_id.clone()),
        win_condition: Some("unknown".into()),
        duration: Some(minutes.to_string()),
        ..WonDraftData::default()
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
    Ok(Ok(Loaded {
        draft,
        data,
        pending,
    }))
}

/// Loads a `/won` draft for `actor` on a connection, or the message for an unusable one
/// (unknown, someone else's, expired, or its game changed since).
pub async fn load_in(
    conn: &mut SqliteConnection,
    state: &AppState,
    id: &str,
    actor: &Actor,
) -> Result<Result<Loaded, String>, sqlx::Error> {
    let unusable = || Ok(Err(UNUSABLE.to_owned()));
    let Some(id) = draft::cast_uuid(id) else {
        return unusable();
    };
    let Some(draft) = draft::get(conn, &id).await? else {
        return unusable();
    };
    if draft.discord_id != actor.discord_id
        || draft.guild_id != actor.guild_id
        || draft.channel_id != actor.channel_id
        || draft.expires_at <= UtcDateTime::now()
    {
        return unusable();
    }
    let Some(pending) = pending::get(conn, draft.pending_game_id).await? else {
        return unusable();
    };
    if draft.snapshot != pending.snapshot()
        || authorize(conn, state, Some(&pending), actor)
            .await?
            .is_err()
    {
        return unusable();
    }
    let Ok(data) = serde_json::from_str::<WonDraftData>(&draft.data) else {
        return unusable();
    };
    Ok(Ok(Loaded {
        draft,
        data,
        pending,
    }))
}

/// Loads a `/won` draft for `actor`; see [`load_in`].
pub async fn load(
    state: &AppState,
    id: &str,
    actor: &Actor,
) -> Result<Result<Loaded, String>, sqlx::Error> {
    load_in(&mut *state.pool.acquire().await?, state, id, actor).await
}

/// Applies an action atomically.
pub async fn act(
    state: &AppState,
    id: &str,
    action: &Action,
    values: &Values,
    actor: &Actor,
) -> Result<Result<Outcome, String>, sqlx::Error> {
    let mut tx = db::begin(&state.pool).await?;
    let loaded = match load_in(&mut tx, state, id, actor).await? {
        Ok(loaded) => loaded,
        Err(error) => return Ok(Err(error)),
    };
    match apply(&mut tx, loaded, action, values).await? {
        Ok(outcome) => {
            tx.commit().await?;
            Ok(Ok(outcome))
        }
        Err(error) => {
            tx.rollback().await?;
            Ok(Err(error))
        }
    }
}

fn value<'a>(values: &'a Values, key: &str) -> Option<&'a str> {
    values.get(key).and_then(Option::as_deref)
}

async fn apply(
    conn: &mut SqliteConnection,
    mut loaded: Loaded,
    action: &Action,
    values: &Values,
) -> Result<Result<Outcome, String>, sqlx::Error> {
    let players = loaded.players();
    let player = |id: &str| players.iter().find(|player| player.discord_id == id);
    match action {
        Action::Commander(player_id) => {
            if player(player_id).is_none() {
                return Ok(Err("Select a player from this game.".into()));
            }
            card_choice::put_commanders(
                conn,
                &mut loaded.data,
                player_id,
                value(values, "commander"),
                value(values, "partner"),
            )
            .await?;
        }
        Action::ChooseCommander(player_id, role) => {
            if player(player_id).is_none() {
                return Ok(Err("Select a player from this game.".into()));
            }
            let id = value(values, "value").unwrap_or_default();
            if let Err(error) =
                card_choice::choose_commander(&mut loaded.data, player_id, *role, id)
            {
                return Ok(Err(error));
            }
        }
        Action::Details => {
            for (key, slot) in [
                ("turns", &mut loaded.data.turns),
                ("duration", &mut loaded.data.duration),
                ("mvp", &mut loaded.data.mvp),
                ("notes", &mut loaded.data.notes),
            ] {
                if let Some(submitted) = values.get(key) {
                    slot.clone_from(submitted);
                }
            }
            loaded.data.details_done = true;
            card_choice::with_mvp(conn, &mut loaded.data).await?;
        }
        Action::Kills(page) => {
            for player in kills_page(&players, *page) {
                if let Some(Some(submitted)) = values.get(&format!("kills_{}", player.discord_id)) {
                    loaded
                        .data
                        .kills
                        .insert(player.discord_id.clone(), submitted.clone());
                }
            }
        }
        Action::Winner => match value(values, "value").and_then(player) {
            Some(winner) => loaded.data.winner = Some(winner.discord_id.clone()),
            None => return Ok(Err("Select a winner from this game's players.".into())),
        },
        Action::Condition => {
            let key = value(values, "value").unwrap_or_default();
            if !reportable_conditions().any(|condition| condition.as_str() == key) {
                return Ok(Err("Select a valid win condition.".into()));
            }
            loaded.data.win_condition = Some(key.to_owned());
        }
        Action::Mvp => {
            let id = value(values, "value").unwrap_or_default();
            let Some(card) = loaded
                .data
                .mvp_candidates
                .iter()
                .find(|candidate| candidate.id == id)
                .cloned()
            else {
                return Ok(Err("Select one of the matching MVP cards.".into()));
            };
            loaded.data.mvp = Some(card.name);
            loaded.data.mvp_id = Some(card.id);
            loaded.data.mvp_error = None;
        }
        Action::Save => {
            return match card_choice::validate(conn, &loaded.data, &players).await? {
                Ok(details) => save(conn, loaded, details).await,
                Err(error) => Ok(Ok(Outcome::Invalid(loaded, error))),
            };
        }
        Action::Cancel => {
            draft::delete(conn, &loaded.draft.id).await?;
            return Ok(Ok(Outcome::Cancelled));
        }
    }
    let data = serde_json::to_string(&loaded.data).unwrap_or_default();
    draft::update_data(conn, &loaded.draft.id, &data).await?;
    loaded.draft.data = data;
    Ok(Ok(Outcome::Updated(loaded)))
}

async fn save(
    conn: &mut SqliteConnection,
    loaded: Loaded,
    details: super::report::ReportDetails,
) -> Result<Result<Outcome, String>, sqlx::Error> {
    let mut report = loaded.pending.report();
    report.winner_discord_ids = loaded.data.winner.iter().cloned().collect();
    report.details = Some(details);
    report.raw.insert(
        "winner_reported_by".into(),
        Value::String(loaded.draft.discord_id.clone()),
    );
    match GamesSink.handle_report(conn, &report).await {
        Ok(()) => {
            pending::delete(conn, loaded.pending.id).await?;
            Ok(Ok(Outcome::Saved(loaded.pending.external_id)))
        }
        Err(_) => Ok(Err(
            "Could not save the result. Your draft is still available; try again.".into(),
        )),
    }
}
