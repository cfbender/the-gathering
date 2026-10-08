//! The `/won` result form (`WonCommand`): routes slash commands, buttons, selects, and
//! modal submissions with `won:<draft>:<action>` custom ids.

use crate::state::AppState;

use super::Actor;
use super::api::{DiscordApi, InteractionResponse, ResponseKind};
use super::draft::Role;
use super::interaction::{Interaction, InteractionData};
use super::won_form::{self, FormModal};
use super::won_report::{self, Action, Outcome, Values};

const INVALID_FORM: &str = "Invalid result form. Run /won again.";

/// A parsed `won:<draft>:<action>` custom id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WonCustomId {
    /// Draft UUID (validated when loaded).
    pub draft_id: String,
    /// The action segment.
    pub action: String,
}

impl WonCustomId {
    /// Parses a custom id; `None` unless it has exactly a draft and an action.
    pub fn parse(custom_id: &str) -> Option<Self> {
        let rest = custom_id.strip_prefix("won:")?;
        match rest.split(':').collect::<Vec<_>>().as_slice() {
            [draft_id, action] => Some(Self {
                draft_id: (*draft_id).to_owned(),
                action: (*action).to_owned(),
            }),
            _ => None,
        }
    }
}

/// What a component or modal asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Request {
    Commanders,
    Review,
    Player,
    OpenModal(FormModal),
    Update(String, Action),
    Act(Action),
    Invalid,
}

fn request(action: &str, modal: bool) -> Request {
    if !modal {
        match action {
            "commanders" => return Request::Commanders,
            "review" => return Request::Review,
            "player" => return Request::Player,
            "details" => return Request::OpenModal(FormModal::Details),
            "kills0" => return Request::OpenModal(FormModal::Kills(0)),
            "kills1" => return Request::OpenModal(FormModal::Kills(1)),
            _ => {}
        }
        if let Some(player) = action.strip_prefix("commander_choice_") {
            return Request::Update(
                player.to_owned(),
                Action::ChooseCommander(player.to_owned(), Role::Commander),
            );
        }
        if let Some(player) = action.strip_prefix("partner_choice_") {
            return Request::Update(
                player.to_owned(),
                Action::ChooseCommander(player.to_owned(), Role::Partner),
            );
        }
        return match action {
            "winner" => Request::Act(Action::Winner),
            "condition" => Request::Act(Action::Condition),
            "mvp" => Request::Act(Action::Mvp),
            "save" => Request::Act(Action::Save),
            "cancel" => Request::Act(Action::Cancel),
            _ => Request::Invalid,
        };
    }
    if let Some(player) = action.strip_prefix("commander_") {
        return Request::Update(player.to_owned(), Action::Commander(player.to_owned()));
    }
    match action {
        "details" => Request::Act(Action::Details),
        "kills0" => Request::Act(Action::Kills(0)),
        "kills1" => Request::Act(Action::Kills(1)),
        _ => Request::Invalid,
    }
}

fn actor(interaction: &Interaction) -> Actor {
    Actor {
        discord_id: interaction.user_id(),
        guild_id: interaction.guild_id.clone().unwrap_or_default(),
        channel_id: interaction.channel_id.clone().unwrap_or_default(),
    }
}

fn values(interaction: &Interaction) -> Values {
    match &interaction.data {
        InteractionData::Component(component) => {
            Values::from([("value".to_owned(), component.values.first().cloned())])
        }
        InteractionData::Modal(modal) => modal
            .fields
            .iter()
            .map(|(id, value)| (id.clone(), Some(value.trim().chars().take(4000).collect())))
            .collect(),
        _ => Values::new(),
    }
}

fn message(content: impl Into<String>) -> InteractionResponse {
    won_form::message(content, ResponseKind::ChannelMessage)
}

/// Handles a `/won` interaction and sends the response.
pub async fn respond(state: &AppState, api: &dyn DiscordApi, interaction: &Interaction) {
    let response = handle(state, interaction).await;
    if api
        .create_response(&interaction.target(), &response)
        .await
        .is_err()
    {
        tracing::error!(
            "Discord /won response failed (response type {})",
            response.kind.code()
        );
    }
}

/// The response for an interaction.
pub async fn handle(state: &AppState, interaction: &Interaction) -> InteractionResponse {
    match handle_inner(state, interaction).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!("Discord /won failed: {error}");
            message(won_report::UNUSABLE)
        }
    }
}

async fn handle_inner(
    state: &AppState,
    interaction: &Interaction,
) -> Result<InteractionResponse, sqlx::Error> {
    let actor = actor(interaction);
    let (custom_id, modal) = match &interaction.data {
        InteractionData::Command(command) if command.name == "won" => {
            let reference = interaction
                .option("game")
                .and_then(super::interaction::OptionValue::text)
                .unwrap_or_default();
            return Ok(
                match won_report::open(state, reference.trim(), &actor).await? {
                    Ok(loaded) => won_form::modal(&loaded, FormModal::Details),
                    Err(error) => message(error),
                },
            );
        }
        InteractionData::Component(component) => (component.custom_id.as_str(), false),
        InteractionData::Modal(data) => (data.custom_id.as_str(), true),
        _ => return Ok(message(INVALID_FORM)),
    };
    let Some(custom_id) = WonCustomId::parse(custom_id) else {
        return Ok(message(INVALID_FORM));
    };
    let id = custom_id.draft_id.as_str();
    let values = values(interaction);
    match request(&custom_id.action, modal) {
        Request::Commanders | Request::Review => {
            let review = custom_id.action == "review";
            Ok(match won_report::load(state, id, &actor).await? {
                Ok(loaded) if review => {
                    won_form::review(&loaded, ResponseKind::UpdateMessage, None)
                }
                Ok(loaded) => won_form::commanders(&loaded, ResponseKind::UpdateMessage, None),
                Err(error) => message(error),
            })
        }
        Request::Player => Ok(match won_report::load(state, id, &actor).await? {
            Ok(loaded) => {
                let selected = values.get("value").cloned().flatten().unwrap_or_default();
                match loaded
                    .players()
                    .iter()
                    .find(|player| player.discord_id == selected)
                {
                    Some(player) => won_form::commander_modal(&loaded, player),
                    None => message("Select a player from this game."),
                }
            }
            Err(error) => message(error),
        }),
        Request::OpenModal(which) => Ok(match won_report::load(state, id, &actor).await? {
            Ok(loaded) => {
                if which == FormModal::Kills(1) && loaded.players().len() <= 5 {
                    message("This game does not need another kills page.")
                } else {
                    won_form::modal(&loaded, which)
                }
            }
            Err(error) => message(error),
        }),
        Request::Update(player_id, action) => Ok(
            match won_report::act(state, id, &action, &values, &actor).await? {
                Ok(Outcome::Updated(loaded)) => {
                    won_form::commanders(&loaded, ResponseKind::UpdateMessage, Some(&player_id))
                }
                Ok(_) => message(INVALID_FORM),
                Err(error) => message(error),
            },
        ),
        Request::Act(action) => {
            let kind = if !modal || interaction.message_id.is_some() {
                ResponseKind::UpdateMessage
            } else {
                ResponseKind::ChannelMessage
            };
            Ok(
                match won_report::act(state, id, &action, &values, &actor).await? {
                    Ok(Outcome::Updated(loaded)) => won_form::review(&loaded, kind, None),
                    Ok(Outcome::Invalid(loaded, error)) => {
                        won_form::review(&loaded, kind, Some(&error))
                    }
                    Ok(Outcome::Saved(external_id)) => won_form::message(
                        format!(
                            "Recorded {}. Use /summary to share the recap.",
                            external_id
                                .strip_prefix("spellbot:")
                                .unwrap_or(&external_id)
                        ),
                        kind,
                    ),
                    Ok(Outcome::Cancelled) => {
                        won_form::message("Draft cancelled. The game is still pending.", kind)
                    }
                    Err(error) => message(error),
                },
            )
        }
        Request::Invalid => Ok(message("Invalid result action.")),
    }
}
