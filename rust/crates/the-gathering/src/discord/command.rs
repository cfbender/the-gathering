//! Slash command definitions and registration (`Discord.Command`).

use super::api::{CommandDefinition, CommandOption, DiscordApi, DiscordError, OptionKind};
use super::{new_game, summary};

/// The `/log` definition.
pub fn definition() -> CommandDefinition {
    CommandDefinition {
        name: "log".into(),
        description: "Open a prefilled game log for a SpellBot game".into(),
        dm_permission: false,
        options: vec![
            CommandOption::optional(
                OptionKind::User,
                "winner",
                "Optional winner; otherwise choose in the game log",
            ),
            CommandOption::optional(
                OptionKind::String,
                "game",
                "SpellBot game ID (e.g. SB12345). Defaults to the latest game in this channel",
            ),
        ],
    }
}

/// `Command.register/2`: registers `/log`, `/summary`, and `/newgame` (in the configured
/// guild, else globally) and removes only the replaced `/won` command. Returns a
/// description for the log.
pub async fn register(
    api: &dyn DiscordApi,
    application_id: &str,
    guild_id: Option<&str>,
) -> Result<String, DiscordError> {
    for command in [definition(), summary::definition(), new_game::definition()] {
        api.create_command(application_id, guild_id, &command)
            .await?;
    }
    let commands = api.list_commands(application_id, guild_id).await?;
    if let Some(legacy) = commands.iter().find(|command| command.name == "won") {
        api.delete_command(application_id, guild_id, &legacy.id)
            .await?;
    }
    Ok(match guild_id {
        Some(guild) => format!("registered /log, /summary, and /newgame in guild {guild}"),
        None => "registered /log, /summary, and /newgame globally; new commands can take up to an hour to appear".into(),
    })
}
