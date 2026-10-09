//! `/newgame`: gathers players for a webcam table.

use crate::db::UtcDateTime;
use crate::state::AppState;

use super::api::{
    CommandDefinition, CommandOption, Component, DiscordApi, DiscordError, InteractionResponse,
    MessagePayload, Modal, OptionKind, ResponseKind, TextInput, TextInputStyle, private,
};
use super::interaction::{Interaction, InteractionData, OptionValue};
use super::scheduled::{
    self, NewQueue, QueueAction, QueueActor, QueueError, ScheduledGame, Status,
};
use super::scheduler::NewGameScheduler;
use super::start_time;

/// The `/newgame` definition.
pub fn definition() -> CommandDefinition {
    CommandDefinition {
        name: "newgame".into(),
        description: "Gather players for a webcam table".into(),
        dm_permission: false,
        options: vec![
            CommandOption {
                max_length: Some(100),
                ..CommandOption::optional(
                    OptionKind::String,
                    "start",
                    "8pm, 20:30, in 45m, tomorrow 7pm, or <t:unix>; omitted starts when filled",
                )
            },
            CommandOption {
                min_value: Some(2),
                max_value: Some(10),
                ..CommandOption::optional(
                    OptionKind::Integer,
                    "min_players",
                    "Minimum players (default 3)",
                )
            },
            CommandOption {
                max_length: Some(100),
                ..CommandOption::optional(OptionKind::String, "title", "Game title")
            },
            CommandOption {
                max_length: Some(100),
                ..CommandOption::optional(
                    OptionKind::String,
                    "format",
                    "Game format (default Commander)",
                )
            },
        ],
    }
}

/// A parsed `newgame:<id>:<action>` custom id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewGameCustomId {
    /// Queue id.
    pub id: i64,
    /// Button.
    pub action: ButtonAction,
}

/// A queue button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    /// Join.
    Join,
    /// Maybe.
    Maybe,
    /// Leave.
    Leave,
    /// Cancel.
    Cancel,
    /// Change time (opens, then submits, a modal).
    Time,
}

impl NewGameCustomId {
    /// Parses a custom id.
    pub fn parse(custom_id: &str) -> Option<Self> {
        let parts: Vec<&str> = custom_id.split(':').collect();
        let ["newgame", id, action] = parts.as_slice() else {
            return None;
        };
        let id: i64 = id.parse().ok().filter(|id| *id > 0)?;
        let action = match *action {
            "join" => ButtonAction::Join,
            "maybe" => ButtonAction::Maybe,
            "leave" => ButtonAction::Leave,
            "cancel" => ButtonAction::Cancel,
            "time" => ButtonAction::Time,
            _ => return None,
        };
        Some(Self { id, action })
    }
}

/// Why `/newgame` handling failed after it started.
#[derive(Debug, thiserror::Error)]
pub enum RespondError {
    /// The placeholder could not be published; the queue was cancelled.
    #[error("delivery_failed")]
    DeliveryFailed,
    /// A Discord call failed.
    #[error(transparent)]
    Discord(#[from] DiscordError),
}

fn error_message(error: &QueueError) -> &'static str {
    match error {
        QueueError::Full => "This game already has 10 players.",
        QueueError::MaybeFull => "This game's maybe list is full.",
        QueueError::Forbidden => {
            "Use this game's original server and channel. Only its host or a Discord Administrator can change its time or cancel it."
        }
        QueueError::Invalid => {
            "Use a minimum of 2–10 players and a title/format of at most 100 characters."
        }
        QueueError::Database(error) => {
            tracing::error!("Discord newgame database operation failed: {error}");
            "The game could not be saved because of a server error. Please try again later."
        }
    }
}

fn private_response(content: &str) -> InteractionResponse {
    InteractionResponse::message(ResponseKind::ChannelMessage, private(content))
}

fn actor(interaction: &Interaction) -> QueueActor {
    let user = interaction.user.as_ref();
    let display_name = interaction
        .member
        .as_ref()
        .and_then(|member| member.nick.clone())
        .or_else(|| user.and_then(|user| user.global_name.clone()))
        .or_else(|| user.and_then(|user| user.username.clone()))
        .unwrap_or_else(|| "Player".to_owned());
    QueueActor {
        discord_id: interaction.user_id(),
        guild_id: interaction.guild_id.clone().unwrap_or_default(),
        channel_id: interaction.channel_id.clone().unwrap_or_default(),
        message_id: interaction.message_id.clone().unwrap_or_default(),
        display_name,
        admin: false,
    }
}

/// Handles a `/newgame` command or one of its buttons.
pub async fn respond(
    state: &AppState,
    api: &dyn DiscordApi,
    scheduler: &NewGameScheduler,
    interaction: &Interaction,
    now: UtcDateTime,
) -> Result<(), RespondError> {
    if interaction.custom_id().is_some() {
        button(state, api, scheduler, interaction, now).await
    } else {
        create(state, api, scheduler, interaction, now).await
    }
}

fn zone(state: &AppState) -> &str {
    &state.config.discord_default_timezone
}

async fn create(
    state: &AppState,
    api: &dyn DiscordApi,
    scheduler: &NewGameScheduler,
    interaction: &Interaction,
    now: UtcDateTime,
) -> Result<(), RespondError> {
    let target = interaction.target();
    let text = |name: &str| interaction.option(name).and_then(OptionValue::text);
    let start_at = match start_time::parse(text("start").as_deref(), now, zone(state)) {
        Ok(start_at) => start_at,
        Err(message) => {
            api.create_response(&target, &private_response(&message))
                .await?;
            return Ok(());
        }
    };
    let min_players = match interaction.option("min_players") {
        Some(OptionValue::Integer(value)) => Some(*value),
        Some(other) => other.text().and_then(|value| value.parse().ok()),
        None => None,
    };
    let queue = NewQueue {
        title: text("title"),
        format: text("format"),
        start_at,
        min_players,
    };
    let game = match scheduled::create(state, &queue, &actor(interaction)).await {
        Ok(game) => game,
        Err(error) => {
            api.create_response(&target, &private_response(error_message(&error)))
                .await?;
            return Ok(());
        }
    };
    // Persist the placeholder's id before exposing buttons. Once attached, even a failed
    // first queue edit is durable work the scheduler retries after boot.
    let placeholder = MessagePayload {
        content: Some("Preparing your game…".into()),
        allowed_mentions: Some(super::api::AllowedMentions::none()),
        ..MessagePayload::default()
    };
    let published = match api
        .create_response(&target, &InteractionResponse::deferred(false))
        .await
    {
        Ok(()) => api.edit_response(&target, &placeholder).await,
        Err(error) => Err(error),
    };
    match published {
        Ok(message) => {
            if let Err(error) = scheduler.attach(game.id, &message.id).await {
                tracing::error!("Discord newgame {} could not be attached: {error}", game.id);
            }
            Ok(())
        }
        Err(_) => {
            if let Err(error) = scheduled::cancel_unpublished(&state.pool, game.id).await {
                tracing::error!(
                    "Discord newgame {} could not be cancelled: {error}",
                    game.id
                );
            }
            tracing::warn!("Discord newgame {} initial response failed", game.id);
            Err(RespondError::DeliveryFailed)
        }
    }
}

async fn button(
    state: &AppState,
    api: &dyn DiscordApi,
    scheduler: &NewGameScheduler,
    interaction: &Interaction,
    now: UtcDateTime,
) -> Result<(), RespondError> {
    let target = interaction.target();
    let Some(custom_id) = interaction.custom_id().and_then(NewGameCustomId::parse) else {
        api.create_response(&target, &private_response("This game button is invalid."))
            .await?;
        return Ok(());
    };
    let mut actor = actor(interaction);
    // Discord computes the member's permissions in the interaction (the guild owner has
    // every permission), so no guild cache is needed.
    actor.admin = matches!(custom_id.action, ButtonAction::Cancel | ButtonAction::Time)
        && interaction.administrator();
    let id = custom_id.id;
    let submitted = matches!(interaction.data, InteractionData::Modal(_));
    let action = match custom_id.action {
        ButtonAction::Time if submitted => {
            let input = interaction
                .field("start")
                .map(str::trim)
                .unwrap_or_default();
            let input = (!input.is_empty()).then_some(input);
            match start_time::parse(input, now, zone(state)) {
                Ok(start_at) => QueueAction::Time(start_at),
                Err(message) => {
                    api.create_response(&target, &private_response(&message))
                        .await?;
                    return Ok(());
                }
            }
        }
        // Modals must be the first response, so check permission before opening one.
        ButtonAction::Time => {
            let allowed = scheduled::manageable(state, id, &actor)
                .await
                .unwrap_or(false);
            let response = if allowed {
                time_modal(id)
            } else {
                private_response(error_message(&QueueError::Forbidden))
            };
            api.create_response(&target, &response).await?;
            return Ok(());
        }
        ButtonAction::Join => QueueAction::Join,
        ButtonAction::Maybe => QueueAction::Maybe,
        ButtonAction::Leave => QueueAction::Leave,
        ButtonAction::Cancel => QueueAction::Cancel,
    };
    api.create_response(&target, &InteractionResponse::deferred(true))
        .await?;
    let result = scheduler.act(id, &action, &actor).await;
    api.edit_response(&target, &private(confirmation(&result, &action)))
        .await?;
    Ok(())
}

fn time_modal(id: i64) -> InteractionResponse {
    InteractionResponse::modal(Modal {
        custom_id: format!("newgame:{id}:time"),
        title: "Change start time".into(),
        components: vec![Component::row(vec![Component::TextInput(TextInput {
            custom_id: "start".into(),
            label: "Start time".into(),
            style: TextInputStyle::Short,
            required: false,
            max_length: 100,
            value: None,
            placeholder: "8pm, in 45m, tomorrow 7pm, <t:unix>; blank starts when filled".into(),
        })])],
    })
}

/// The private confirmation for a queue action.
pub fn confirmation(result: &Result<ScheduledGame, QueueError>, action: &QueueAction) -> String {
    let game = match result {
        Ok(game) => game,
        Err(error) => return error_message(error).to_owned(),
    };
    match (game.status, action) {
        (Status::Started, _) => "Your game is ready! See the lobby link in the channel.".into(),
        (Status::Expired, _) => "This game did not fill before its start time.".into(),
        (Status::Cancelled, _) => "This game was cancelled.".into(),
        (Status::Open, QueueAction::Join) => "You are on the roster.".into(),
        (Status::Open, QueueAction::Maybe) if game.start_at.is_none() => {
            "You are on the maybe list. Maybes don't count toward the minimum.".into()
        }
        (Status::Open, QueueAction::Maybe) if game.maybe_pinged_at.is_none() => {
            "You are on the maybe list. You'll be pinged at the start time if the game is short of players.".into()
        }
        (Status::Open, QueueAction::Maybe) => {
            "You are on the maybe list. Click Join if you can play.".into()
        }
        (Status::Open, QueueAction::Leave) => {
            "You are no longer on the roster or maybe list.".into()
        }
        (Status::Open, QueueAction::Time(_)) => match game.start_at {
            None => "The game now starts when filled.".into(),
            Some(start) => format!("The game now starts <t:{}:F>.", start.unix()),
        },
        // Only reachable if the cancel did not apply; report the status.
        (Status::Open, QueueAction::Cancel) => "This game is still open.".into(),
    }
}
