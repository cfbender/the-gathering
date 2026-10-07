//! The parts of a Discord interaction the bot reads, converted from twilight's model.
//!
//! Snowflakes are strings, as the Elixir code compared them (`to_string/1`). The invoking
//! user comes from `user` in DMs and `member.user` in guilds.

use twilight_model::application::interaction::application_command::CommandOptionValue;
use twilight_model::application::interaction::modal::ModalInteractionComponent;
use twilight_model::application::interaction::{
    Interaction as TwilightInteraction, InteractionData as TwilightData,
};
use twilight_model::guild::Permissions;

use super::api::InteractionTarget;

/// An interaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interaction {
    /// Interaction id.
    pub id: String,
    /// Application id.
    pub application_id: String,
    /// Response token.
    pub token: String,
    /// Guild, absent in DMs.
    pub guild_id: Option<String>,
    /// Channel.
    pub channel_id: Option<String>,
    /// The invoking user.
    pub user: Option<InteractionUser>,
    /// The invoking member, in guilds.
    pub member: Option<InteractionMember>,
    /// The message a component is attached to (or a modal was opened from).
    pub message_id: Option<String>,
    /// Command, component, or modal data.
    pub data: InteractionData,
}

/// The invoking user.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionUser {
    /// Snowflake.
    pub id: String,
    /// Username.
    pub username: Option<String>,
    /// Display name.
    pub global_name: Option<String>,
}

/// The invoking guild member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionMember {
    /// Server nickname.
    pub nick: Option<String>,
    /// Computed permissions in the channel (Discord grants the guild owner all of them).
    pub permissions: Option<u64>,
}

/// What was invoked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InteractionData {
    /// A slash command (type 2).
    Command(CommandData),
    /// A button or select (type 3).
    Component(ComponentData),
    /// A modal submission (type 5).
    Modal(ModalData),
    /// Anything else.
    #[default]
    Other,
}

/// Slash command data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandData {
    /// Command name.
    pub name: String,
    /// Options given.
    pub options: Vec<CommandOptionData>,
}

/// One given option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOptionData {
    /// Option name.
    pub name: String,
    /// Value.
    pub value: OptionValue,
}

/// An option's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionValue {
    /// String option.
    String(String),
    /// Integer option.
    Integer(i64),
    /// User option (a snowflake).
    User(String),
    /// Any other type.
    Other,
}

impl OptionValue {
    /// The value as text (`to_string/1`): strings, integers, and user ids.
    pub fn text(&self) -> Option<String> {
        match self {
            Self::String(value) | Self::User(value) => Some(value.clone()),
            Self::Integer(value) => Some(value.to_string()),
            Self::Other => None,
        }
    }
}

/// Component data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentData {
    /// Custom id.
    pub custom_id: String,
    /// Selected values.
    pub values: Vec<String>,
}

/// Modal submission data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModalData {
    /// Custom id.
    pub custom_id: String,
    /// Text inputs as `(custom_id, value)`, in order.
    pub fields: Vec<(String, String)>,
}

impl Interaction {
    /// Where responses go.
    pub fn target(&self) -> InteractionTarget {
        InteractionTarget {
            id: self.id.clone(),
            application_id: self.application_id.clone(),
            token: self.token.clone(),
        }
    }

    /// The invoking user's id, or `""`.
    pub fn user_id(&self) -> String {
        self.user
            .as_ref()
            .map(|user| user.id.clone())
            .unwrap_or_default()
    }

    /// The command name, for slash commands.
    pub fn command_name(&self) -> Option<&str> {
        match &self.data {
            InteractionData::Command(command) => Some(&command.name),
            _ => None,
        }
    }

    /// The custom id of a component or modal.
    pub fn custom_id(&self) -> Option<&str> {
        match &self.data {
            InteractionData::Component(data) => Some(&data.custom_id),
            InteractionData::Modal(data) => Some(&data.custom_id),
            _ => None,
        }
    }

    /// A slash command option.
    pub fn option(&self, name: &str) -> Option<&OptionValue> {
        match &self.data {
            InteractionData::Command(command) => command
                .options
                .iter()
                .find(|option| option.name == name)
                .map(|option| &option.value),
            _ => None,
        }
    }

    /// A submitted modal field.
    pub fn field(&self, name: &str) -> Option<&str> {
        match &self.data {
            InteractionData::Modal(modal) => modal
                .fields
                .iter()
                .find(|(id, _)| id == name)
                .map(|(_, value)| value.as_str()),
            _ => None,
        }
    }

    /// Whether the member has the Administrator permission.
    pub fn administrator(&self) -> bool {
        self.member
            .as_ref()
            .and_then(|member| member.permissions)
            .is_some_and(|bits| bits & Permissions::ADMINISTRATOR.bits() != 0)
    }
}

fn text_inputs(components: &[ModalInteractionComponent], fields: &mut Vec<(String, String)>) {
    for component in components {
        match component {
            ModalInteractionComponent::ActionRow(row) => text_inputs(&row.components, fields),
            ModalInteractionComponent::Label(label) => {
                text_inputs(std::slice::from_ref(&label.component), fields);
            }
            ModalInteractionComponent::TextInput(input) => {
                fields.push((input.custom_id.clone(), input.value.clone()));
            }
            _ => {}
        }
    }
}

impl From<TwilightInteraction> for Interaction {
    fn from(interaction: TwilightInteraction) -> Self {
        let member_user = interaction
            .member
            .as_ref()
            .and_then(|member| member.user.clone());
        let user = interaction
            .user
            .clone()
            .or(member_user)
            .map(|user| InteractionUser {
                id: user.id.to_string(),
                username: Some(user.name),
                global_name: user.global_name,
            });
        let member = interaction.member.as_ref().map(|member| InteractionMember {
            nick: member.nick.clone(),
            permissions: member.permissions.map(|permissions| permissions.bits()),
        });
        let data = match interaction.data {
            Some(TwilightData::ApplicationCommand(command)) => {
                InteractionData::Command(CommandData {
                    name: command.name,
                    options: command
                        .options
                        .into_iter()
                        .map(|option| CommandOptionData {
                            name: option.name,
                            value: match option.value {
                                CommandOptionValue::String(value) => OptionValue::String(value),
                                CommandOptionValue::Integer(value) => OptionValue::Integer(value),
                                CommandOptionValue::User(id) => OptionValue::User(id.to_string()),
                                _ => OptionValue::Other,
                            },
                        })
                        .collect(),
                })
            }
            Some(TwilightData::MessageComponent(component)) => {
                InteractionData::Component(ComponentData {
                    custom_id: component.custom_id,
                    values: component.values,
                })
            }
            Some(TwilightData::ModalSubmit(modal)) => {
                let mut fields = Vec::new();
                text_inputs(&modal.components, &mut fields);
                InteractionData::Modal(ModalData {
                    custom_id: modal.custom_id,
                    fields,
                })
            }
            _ => InteractionData::Other,
        };
        Self {
            id: interaction.id.to_string(),
            application_id: interaction.application_id.to_string(),
            token: interaction.token,
            guild_id: interaction.guild_id.map(|id| id.to_string()),
            channel_id: interaction
                .channel
                .as_ref()
                .map(|channel| channel.id.to_string()),
            user,
            member,
            message_id: interaction
                .message
                .as_ref()
                .map(|message| message.id.to_string()),
            data,
        }
    }
}
