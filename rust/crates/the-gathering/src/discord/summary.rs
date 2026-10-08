//! `/summary`: posts a rendered recap of a recorded game.

use std::time::Instant;

use crate::games::{self, Game, GamesError, RenderError, summary_card};
use crate::state::AppState;

use super::api::{
    AllowedMentions, ApiFuture, AttachmentMeta, CommandDefinition, CommandOption, DiscordApi,
    DiscordError, EPHEMERAL, FileUpload, InteractionResponse, MessagePayload, OptionKind,
    ResponseKind,
};
use super::configured_guild;
use super::interaction::{Interaction, OptionValue};

/// The `/summary` definition.
pub fn definition() -> CommandDefinition {
    CommandDefinition {
        name: "summary".into(),
        description: "Post a rendered recap of a recorded game (defaults to the latest game)"
            .into(),
        dm_permission: false,
        options: vec![CommandOption::optional(
            OptionKind::String,
            "game",
            "Gathering game ID (123) or SpellBot ID (SB12345)",
        )],
    }
}

/// Why a summary was not posted.
#[derive(Debug, thiserror::Error)]
pub enum Rejection {
    /// Not an active Discord-linked member in the bot's server.
    #[error("forbidden")]
    Forbidden,
    /// No such game.
    #[error("not_found")]
    NotFound,
    /// A malformed reference.
    #[error("bad_request")]
    BadRequest,
    /// Over the render budget.
    #[error("rate_limited")]
    RateLimited,
    /// Rendering or the database failed.
    #[error("{0}")]
    Failed(String),
}

/// The message shown for a rejected summary request.
pub fn message(reason: &Rejection) -> &'static str {
    match reason {
        Rejection::Forbidden => {
            "Sign in to The Gathering with Discord first. Summaries are only available to active members in the bot's server."
        }
        Rejection::NotFound => {
            "No recorded game found. SpellBot games must have a recorded result first (use `/log`)."
        }
        Rejection::BadRequest => {
            "Use a Gathering game ID such as `123`, or a SpellBot ID such as `SB12345`."
        }
        Rejection::RateLimited => "Too many summaries requested. Please try again in a minute.",
        Rejection::Failed(_) => {
            "I couldn't render that summary. Please try again or ask an administrator to check the renderer."
        }
    }
}

/// The game to summarize, for active members only.
pub async fn prepare(state: &AppState, interaction: &Interaction) -> Result<Game, Rejection> {
    let Some(guild) = &interaction.guild_id else {
        return Err(Rejection::Forbidden);
    };
    if configured_guild(state).is_some_and(|configured| configured != *guild) {
        return Err(Rejection::Forbidden);
    }
    let Some(user) = &interaction.user else {
        return Err(Rejection::Forbidden);
    };
    let account = state
        .accounts
        .get_user_by_discord_id(&user.id)
        .await
        .map_err(|error| Rejection::Failed(error.to_string()))?;
    if account.is_none_or(|account| account.disabled_at.is_some()) {
        return Err(Rejection::Forbidden);
    }
    let reference = interaction
        .option("game")
        .and_then(OptionValue::text)
        .unwrap_or_default();
    state
        .games
        .find_summary_game(&reference)
        .await
        .map_err(|error| match error {
            GamesError::NotFound => Rejection::NotFound,
            GamesError::BadRequest | GamesError::Invalid(_) => Rejection::BadRequest,
            GamesError::Database(error) => Rejection::Failed(error.to_string()),
        })
}

/// The PNG upload, or a plain-text failure.
pub async fn render_response(state: &AppState, game: &Game) -> MessagePayload {
    let started = Instant::now();
    tracing::info!("Discord /summary rendering game {}", game.id);
    match games::render_summary(state, game).await {
        Ok(png) => {
            tracing::info!(
                "Discord /summary rendered game {} in {} ms ({} bytes)",
                game.id,
                started.elapsed().as_millis(),
                png.len()
            );
            let name = format!("game-{}-summary.png", game.id);
            MessagePayload {
                content: Some(format!(
                    "Game #{} · <{}/games/{}>",
                    game.id,
                    state.config.public_url(),
                    game.id
                )),
                allowed_mentions: Some(AllowedMentions::none()),
                attachments: Some(vec![AttachmentMeta {
                    id: 0,
                    filename: name.clone(),
                    description: summary_card::description(game),
                }]),
                files: vec![FileUpload { name, body: png }],
                ..MessagePayload::default()
            }
        }
        Err(error) => {
            tracing::warn!(
                "Discord summary rendering failed for game {}: {error}",
                game.id
            );
            let reason = match error {
                RenderError::RateLimited => Rejection::RateLimited,
                other => Rejection::Failed(other.to_string()),
            };
            MessagePayload {
                content: Some(message(&reason).to_owned()),
                allowed_mentions: Some(AllowedMentions::none()),
                ..MessagePayload::default()
            }
        }
    }
}

/// Logs a stage's outcome with numeric codes only (never tokens or response bodies).
async fn logged<T>(stage: &str, call: ApiFuture<'_, T>) -> Result<T, DiscordError> {
    let result = call.await;
    match &result {
        Ok(_) => tracing::info!("Discord /summary {stage} completed"),
        Err(error) => tracing::error!("Discord /summary {stage} failed: {error}"),
    }
    result
}

/// Acknowledges publicly before rendering, then uploads.
/// A failed acknowledgement is never retried: Discord may already have accepted it.
pub async fn respond(
    state: &AppState,
    api: &dyn DiscordApi,
    interaction: &Interaction,
) -> Result<(), DiscordError> {
    tracing::info!("Discord /summary invoked");
    let target = interaction.target();
    match prepare(state, interaction).await {
        Ok(game) => {
            tracing::info!("Discord /summary selected game {}", game.id);
            let ack = InteractionResponse::deferred(false);
            logged("acknowledge", api.create_response(&target, &ack)).await?;
            let response = render_response(state, &game).await;
            tracing::info!("Discord /summary upload started for game {}", game.id);
            logged("upload", api.edit_response(&target, &response))
                .await
                .map(|_| ())
        }
        Err(reason) => {
            tracing::info!("Discord /summary rejected: {reason}");
            let rejection = InteractionResponse::message(
                ResponseKind::ChannelMessage,
                MessagePayload {
                    content: Some(message(&reason).to_owned()),
                    flags: Some(EPHEMERAL),
                    ..MessagePayload::default()
                },
            );
            logged("rejection", api.create_response(&target, &rejection)).await
        }
    }
}
