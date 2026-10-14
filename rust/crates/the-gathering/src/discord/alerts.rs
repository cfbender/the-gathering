//! `/alerts`: toggles the invoking member's `DISCORD_NEWGAME_ROLE_ID` role, which the bot
//! mentions in each `/newgame` thread.

use crate::state::AppState;

use super::api::{
    CommandDefinition, DiscordApi, DiscordError, InteractionResponse, ResponseKind, private,
};
use super::configured_guild;
use super::interaction::Interaction;

/// The `/alerts` definition.
pub fn definition() -> CommandDefinition {
    CommandDefinition {
        name: "alerts".into(),
        description: "Turn new game alerts on or off".into(),
        dm_permission: false,
        options: Vec::new(),
    }
}

/// Gives the member the alerts role, or takes it away when they already have it.
pub async fn respond(
    state: &AppState,
    api: &dyn DiscordApi,
    interaction: &Interaction,
) -> Result<(), DiscordError> {
    let target = interaction.target();
    let guild = interaction
        .guild_id
        .clone()
        .filter(|guild| configured_guild(state).is_none_or(|configured| configured == *guild));
    let (Some(guild), Some(role), Some(member)) = (
        guild,
        state.config.discord_newgame_role_id.as_deref(),
        interaction.member.as_ref(),
    ) else {
        let reply = private("Game alerts are not set up on this server.");
        return api
            .create_response(
                &target,
                &InteractionResponse::message(ResponseKind::ChannelMessage, reply),
            )
            .await;
    };
    let on = !member.roles.iter().any(|id| id == role);
    api.create_response(&target, &InteractionResponse::deferred(true))
        .await?;
    let reply = match api
        .set_member_role(&guild, &interaction.user_id(), role, on)
        .await
    {
        Ok(()) if on => {
            "Game alerts are on: you'll be added to each new /newgame thread. Run /alerts again to turn them off."
        }
        Ok(()) => "Game alerts are off. Run /alerts again to turn them back on.",
        Err(error) => {
            tracing::warn!("Discord alerts role could not be changed: {error}");
            "Your game alerts could not be changed. The bot needs the Manage Roles permission and a role above the alerts role."
        }
    };
    api.edit_response(&target, &private(reply)).await?;
    Ok(())
}
