//! `/log`: a private link to finish a SpellBot game in The Gathering (`LogCommand`).

use crate::state::AppState;

use super::Actor;
use super::api::{
    AllowedMentions, Button, ButtonStyle, Component, DiscordApi, EPHEMERAL, InteractionResponse,
    MessagePayload, ResponseKind,
};
use super::interaction::{Interaction, OptionValue};
use super::web_draft::{self, OpenError};

fn response(content: &str, components: Vec<Component>) -> InteractionResponse {
    InteractionResponse::message(
        ResponseKind::ChannelMessage,
        MessagePayload {
            content: Some(content.to_owned()),
            components: Some(components),
            flags: Some(EPHEMERAL),
            allowed_mentions: Some(AllowedMentions::none()),
            ..MessagePayload::default()
        },
    )
}

/// `LogCommand.respond/2`.
pub async fn respond(state: &AppState, api: &dyn DiscordApi, interaction: &Interaction) {
    let response = handle(state, interaction).await;
    if api
        .create_response(&interaction.target(), &response)
        .await
        .is_err()
    {
        tracing::error!("Discord /log response failed");
    }
}

/// `LogCommand.handle/1`.
pub async fn handle(state: &AppState, interaction: &Interaction) -> InteractionResponse {
    let actor = Actor {
        discord_id: interaction.user_id(),
        guild_id: interaction.guild_id.clone().unwrap_or_default(),
        channel_id: interaction.channel_id.clone().unwrap_or_default(),
    };
    let reference = interaction
        .option("game")
        .and_then(OptionValue::text)
        .unwrap_or_default();
    let winner = interaction.option("winner").and_then(OptionValue::text);
    match web_draft::open(state, reference.trim(), winner.as_deref(), &actor).await {
        Ok(draft) => response(
            "Finish logging this game in The Gathering. Sign in with the same Discord account. Nothing is saved until you submit; this link expires in one hour.",
            vec![Component::row(vec![Component::Button(Button {
                custom_id: None,
                label: "Open game log".into(),
                style: ButtonStyle::Link,
                url: Some(format!(
                    "{}/games/new?discord={}",
                    state.config.public_url(),
                    draft.id
                )),
                disabled: None,
            })])],
        ),
        Err(OpenError::InvalidWinner) => response(
            "Choose a winner from this SpellBot game's players.",
            Vec::new(),
        ),
        Err(OpenError::NotFound) => response(
            "No unfinished game found. Try /log game:SB12345.",
            Vec::new(),
        ),
        Err(error) => {
            if let OpenError::Database(error) = &error {
                tracing::error!("Discord /log failed: {error}");
            }
            response(
                "Use /log in the game's server. Disabled accounts cannot log games.",
                Vec::new(),
            )
        }
    }
}
