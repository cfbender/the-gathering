//! The gateway connection and event routing.
//!
//! [`Bot`] handles parsed events; [`start`] connects it to Discord's gateway with
//! `twilight-gateway`. Tests drive [`Bot`] directly with a fake [`DiscordApi`].

use std::sync::Arc;

use twilight_gateway::{
    CloseFrame, ConfigBuilder, Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _,
};
use twilight_model::gateway::CloseCode;
use twilight_model::gateway::payload::outgoing::update_presence::UpdatePresencePayload;
use twilight_model::gateway::presence::{ActivityType, MinimalActivity, Status};

use crate::db::UtcDateTime;
use crate::state::AppState;

use super::api::DiscordApi;
use super::interaction::Interaction;
use super::rest::RestApi;
use super::scheduler::{INTERVAL, NewGameScheduler};
use super::sink::{GamesSink, Sink};
use super::spellbot::{self, ParseError, SpellBotMessage};
use super::tracker::Tracker;
use super::{command, configured_guild, log_command, new_game, summary, won};

/// SpellBot's user id when `DISCORD_SPELLBOT_USER_ID` is unset.
pub const DEFAULT_SPELLBOT_USER_ID: &str = "725510263251402832";

/// The bot's handlers and long-lived workers.
pub struct Bot {
    /// Server state.
    pub state: AppState,
    /// Outbound Discord calls.
    pub api: Arc<dyn DiscordApi>,
    /// SpellBot game staging.
    pub tracker: Tracker,
    /// `/newgame` queue writer.
    pub scheduler: Arc<NewGameScheduler>,
}

impl std::fmt::Debug for Bot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bot").finish_non_exhaustive()
    }
}

impl Bot {
    /// A bot dispatching staged reports to `sink`.
    pub fn new(state: AppState, api: Arc<dyn DiscordApi>, sink: Arc<dyn Sink>) -> Self {
        let tracker = Tracker::new(state.pool.clone(), sink);
        let scheduler = Arc::new(NewGameScheduler::new(state.clone(), Arc::clone(&api)));
        Self {
            state,
            api,
            tracker,
            scheduler,
        }
    }

    fn spellbot_user_id(&self) -> &str {
        self.state
            .config
            .discord_bot
            .as_ref()
            .map_or(DEFAULT_SPELLBOT_USER_ID, |bot| {
                bot.spellbot_user_id.as_str()
            })
    }

    /// `READY`: logs the connection and registers commands.
    pub async fn ready(&self, application_id: &str, username: &str, guilds: usize) {
        tracing::info!("Discord bot connected as {username} in {guilds} guild(s)");
        let guild = configured_guild(&self.state);
        match command::register(self.api.as_ref(), application_id, guild.as_deref()).await {
            Ok(description) => tracing::info!("Discord {description}"),
            Err(error) => tracing::error!("Could not register Discord commands: {error}"),
        }
    }

    /// `MESSAGE_CREATE`/`MESSAGE_UPDATE`: stages SpellBot's ready games. Logs describe the
    /// game by SpellBot id and player count only; message text is never logged.
    pub async fn observe(&self, message: &SpellBotMessage, kind: &str) {
        match spellbot::parse(message, self.spellbot_user_id()) {
            Ok(report) => match self.tracker.observe(&report).await {
                Ok(()) => tracing::info!(
                    "Discord observed SpellBot game {} with {} player(s)",
                    report.external_id,
                    report.players.len()
                ),
                Err(error) => tracing::warn!(
                    "Could not record Discord game {}: {error}",
                    report.external_id
                ),
            },
            // Every other message on the server arrives here; only SpellBot's are worth a line.
            Err(ParseError::NotSpellbot) => {}
            // SpellBot defers its interactions, so its first message is an empty
            // placeholder, and its validation replies are plain text. The ready embed
            // arrives later as an edit of the waiting post.
            Err(reason) => tracing::debug!(
                "Discord ignored a {kind} SpellBot message (type {:?}): {reason}",
                message.kind
            ),
        }
    }

    /// `INTERACTION_CREATE`: routes commands and components.
    pub async fn handle_interaction(&self, interaction: &Interaction) {
        let api = self.api.as_ref();
        let state = &self.state;
        if let Some(name) = interaction.command_name() {
            match name {
                "summary" => {
                    let _ = summary::respond(state, api, interaction).await;
                }
                "newgame" => {
                    let _ = new_game::respond(
                        state,
                        api,
                        &self.scheduler,
                        interaction,
                        UtcDateTime::now(),
                    )
                    .await;
                }
                "log" | "won" => log_command::respond(state, api, interaction).await,
                _ => {}
            }
        } else if let Some(custom_id) = interaction.custom_id() {
            if custom_id.starts_with("newgame:") {
                let _ =
                    new_game::respond(state, api, &self.scheduler, interaction, UtcDateTime::now())
                        .await;
            } else if custom_id.starts_with("won:") {
                won::respond(state, api, interaction).await;
            }
        }
    }

    /// Handles one gateway event.
    pub async fn handle_event(&self, event: Event) {
        match event {
            Event::Ready(ready) => {
                self.ready(
                    &ready.application.id.to_string(),
                    &ready.user.name,
                    ready.guilds.len(),
                )
                .await;
            }
            Event::MessageCreate(message) => {
                self.observe(&SpellBotMessage::from(&message.0), "new")
                    .await;
            }
            Event::MessageUpdate(message) => {
                self.observe(&SpellBotMessage::from(&message.0), "edited")
                    .await;
            }
            Event::InteractionCreate(interaction) => {
                self.handle_interaction(&Interaction::from(interaction.0))
                    .await;
            }
            _ => {}
        }
    }
}

/// Starts the bot when `DISCORD_BOT_TOKEN` is set. The connection runs in the background:
/// a bad token is logged and never takes the web server down. The token is only handed
/// to the gateway and REST clients and never logged.
pub fn start(state: &AppState) {
    let Some(config) = state.config.discord_bot.clone() else {
        tracing::info!("Discord bot disabled: DISCORD_BOT_TOKEN is not set");
        return;
    };
    tracing::info!(
        "Discord bot enabled; connecting to the gateway (the application needs the Message Content intent)"
    );
    let api: Arc<dyn DiscordApi> = match RestApi::new(&config.token) {
        Ok(api) => Arc::new(api),
        Err(error) => {
            tracing::error!(
                "Discord bot could not start; game tracking from Discord is off until the server restarts with a valid DISCORD_BOT_TOKEN: {error}"
            );
            return;
        }
    };
    let bot = Arc::new(Bot::new(state.clone(), api, Arc::new(GamesSink)));
    let scheduler = bot.scheduler.spawn(INTERVAL);
    tokio::spawn(async move {
        run_gateway(bot, config.token).await;
        // Nothing keeps running without a gateway session.
        scheduler.abort();
    });
}

fn fatal(frame: &CloseFrame<'_>) -> bool {
    CloseCode::try_from(frame.code).is_ok_and(|code| !code.can_reconnect())
}

async fn run_gateway(bot: Arc<Bot>, token: String) {
    let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;
    let mut builder = ConfigBuilder::new(token, intents);
    // Announce a presence on identify so the bot shows as online, "Watching the battlefield".
    let activity = MinimalActivity {
        kind: ActivityType::Watching,
        name: "the battlefield".into(),
        url: None,
    };
    match UpdatePresencePayload::new(vec![activity.into()], false, None, Status::Online) {
        Ok(presence) => builder = builder.presence(presence),
        Err(error) => tracing::warn!("Discord presence is invalid: {error}"),
    }
    let mut shard = Shard::with_config(ShardId::ONE, builder.build());
    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        match item {
            Ok(Event::GatewayClose(Some(frame))) if fatal(&frame) => {
                // Discord names the cause, e.g. 4004 Authentication failed or 4014
                // Disallowed intents (enable Message Content in the developer portal).
                tracing::error!(
                    "Discord bot could not start; game tracking from Discord is off until the server restarts with a valid DISCORD_BOT_TOKEN: close code {} {}",
                    frame.code,
                    frame.reason
                );
                return;
            }
            Ok(event) => {
                let bot = Arc::clone(&bot);
                tokio::spawn(async move { bot.handle_event(event).await });
            }
            Err(error) => tracing::warn!("Discord gateway error: {error}"),
        }
    }
    tracing::warn!("Discord gateway connection ended");
}
