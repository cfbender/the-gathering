//! Outbound Discord operations and the payloads they send.
//!
//! Every handler talks to Discord through [`DiscordApi`], so command and interaction
//! handling runs without a gateway: production uses [`super::rest::RestApi`], tests a
//! recording fake (the Elixir tests stubbed Nostrum's API modules the same way).
//! Payloads are typed and serialize to exactly the JSON the Elixir bot sent.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize, Serializer};

/// A boxed Discord call.
pub type ApiFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, DiscordError>> + Send + 'a>>;

/// Why a Discord call failed. `Display` never includes response bodies, tokens, or
/// message content, only numeric codes (`SummaryCommand.failure_details/1`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DiscordError {
    /// Discord answered with a non-success status.
    #[error("HTTP {status}, Discord code {}", code.map_or_else(|| "unknown".to_owned(), |code| code.to_string()))]
    Http {
        /// HTTP status.
        status: u16,
        /// Discord's JSON error code.
        code: Option<i64>,
    },
    /// The request timed out.
    #[error("timeout")]
    Timeout,
    /// The connection failed.
    #[error("network")]
    Network,
    /// Any other transport failure.
    #[error("transport error")]
    Transport,
}

/// What an interaction response is addressed with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionTarget {
    /// Interaction id.
    pub id: String,
    /// Application id.
    pub application_id: String,
    /// The short-lived interaction token.
    pub token: String,
}

/// A message Discord created or edited.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SentMessage {
    /// Message id.
    pub id: String,
}

/// A registered application command.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RegisteredCommand {
    /// Command id.
    pub id: String,
    /// Command name.
    pub name: String,
}

/// Outbound Discord operations (Nostrum's `Api.Interaction`, `Api.Message`, and
/// `Api.ApplicationCommand`).
pub trait DiscordApi: Send + Sync {
    /// Responds to an interaction. Never retried: Discord may already have accepted it.
    fn create_response<'a>(
        &'a self,
        interaction: &'a InteractionTarget,
        response: &'a InteractionResponse,
    ) -> ApiFuture<'a, ()>;

    /// Edits the original interaction response (multipart when it carries files).
    fn edit_response<'a>(
        &'a self,
        interaction: &'a InteractionTarget,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage>;

    /// Posts a message in a channel.
    fn create_message<'a>(
        &'a self,
        channel_id: &'a str,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage>;

    /// Edits a channel message.
    fn edit_message<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage>;

    /// Creates (or overwrites by name) a guild command, or a global one without a guild.
    fn create_command<'a>(
        &'a self,
        application_id: &'a str,
        guild_id: Option<&'a str>,
        command: &'a CommandDefinition,
    ) -> ApiFuture<'a, ()>;

    /// Lists guild or global commands.
    fn list_commands<'a>(
        &'a self,
        application_id: &'a str,
        guild_id: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<RegisteredCommand>>;

    /// Deletes a guild or global command.
    fn delete_command<'a>(
        &'a self,
        application_id: &'a str,
        guild_id: Option<&'a str>,
        command_id: &'a str,
    ) -> ApiFuture<'a, ()>;
}

/// Ephemeral message flag.
pub const EPHEMERAL: u64 = 64;

/// Interaction callback types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseKind {
    /// 4: reply with a message.
    ChannelMessage,
    /// 5: acknowledge now, edit the response later.
    DeferredChannelMessage,
    /// 7: update the message the component is attached to.
    UpdateMessage,
    /// 9: open a modal.
    Modal,
}

impl ResponseKind {
    /// Discord's numeric type.
    pub fn code(self) -> u8 {
        match self {
            Self::ChannelMessage => 4,
            Self::DeferredChannelMessage => 5,
            Self::UpdateMessage => 7,
            Self::Modal => 9,
        }
    }
}

impl Serialize for ResponseKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

/// An interaction response.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InteractionResponse {
    /// Callback type.
    #[serde(rename = "type")]
    pub kind: ResponseKind,
    /// Message or modal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
}

/// An interaction response's data.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// A message.
    Message(MessagePayload),
    /// A modal.
    Modal(Modal),
}

impl InteractionResponse {
    /// `%{type: 5}`, optionally private (`data: %{flags: 64}`).
    pub fn deferred(ephemeral: bool) -> Self {
        Self {
            kind: ResponseKind::DeferredChannelMessage,
            data: ephemeral.then(|| {
                ResponseData::Message(MessagePayload {
                    flags: Some(EPHEMERAL),
                    ..MessagePayload::default()
                })
            }),
        }
    }

    /// A message response of `kind` (4 or 7).
    pub fn message(kind: ResponseKind, message: MessagePayload) -> Self {
        Self {
            kind,
            data: Some(ResponseData::Message(message)),
        }
    }

    /// A modal.
    pub fn modal(modal: Modal) -> Self {
        Self {
            kind: ResponseKind::Modal,
            data: Some(ResponseData::Modal(modal)),
        }
    }

    /// The message data, if any.
    pub fn message_data(&self) -> Option<&MessagePayload> {
        match &self.data {
            Some(ResponseData::Message(message)) => Some(message),
            _ => None,
        }
    }

    /// The modal, if any.
    pub fn modal_data(&self) -> Option<&Modal> {
        match &self.data {
            Some(ResponseData::Modal(modal)) => Some(modal),
            _ => None,
        }
    }

    /// The message content (empty when there is none).
    pub fn content(&self) -> &str {
        self.message_data()
            .and_then(|message| message.content.as_deref())
            .unwrap_or_default()
    }
}

/// A modal.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Modal {
    /// Custom id echoed back on submit.
    pub custom_id: String,
    /// Title.
    pub title: String,
    /// Action rows of text inputs.
    pub components: Vec<Component>,
}

/// Which mentions may ping.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AllowedMentions {
    /// Mention types parsed from content; always empty (`:none`).
    pub parse: Vec<String>,
    /// Users who may be pinged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<String>>,
}

impl AllowedMentions {
    /// No pings (`%{parse: []}`).
    pub fn none() -> Self {
        Self::default()
    }

    /// Only these users (`%{parse: [], users: ids}`).
    pub fn users(ids: Vec<String>) -> Self {
        Self {
            parse: Vec::new(),
            users: Some(ids),
        }
    }
}

/// Message content for responses, edits, and new messages.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MessagePayload {
    /// Text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Embeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<Embed>>,
    /// Action rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<Component>>,
    /// Mention suppression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
    /// Message flags (64 = ephemeral).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Attachment metadata for `files`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<AttachmentMeta>>,
    /// Idempotency nonce (at most 25 characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// Reject duplicates of `nonce`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforce_nonce: Option<bool>,
    /// Uploaded files (sent as multipart parts, not JSON).
    #[serde(skip)]
    pub files: Vec<FileUpload>,
}

impl MessagePayload {
    /// The action rows (empty when there are none).
    pub fn rows(&self) -> &[Component] {
        self.components.as_deref().unwrap_or_default()
    }

    /// Every component inside the action rows.
    pub fn row_components(&self) -> Vec<&Component> {
        self.rows()
            .iter()
            .flat_map(|row| match row {
                Component::ActionRow(row) => row.components.iter().collect(),
                other => vec![other],
            })
            .collect()
    }
}

/// Attachment metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AttachmentMeta {
    /// Index of the file part.
    pub id: u32,
    /// File name.
    pub filename: String,
    /// Alt text.
    pub description: String,
}

/// An uploaded file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileUpload {
    /// File name.
    pub name: String,
    /// Bytes.
    pub body: Vec<u8>,
}

/// An embed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Embed {
    /// Title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Fields.
    pub fields: Vec<EmbedField>,
}

/// An embed field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EmbedField {
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
    /// Shown side by side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline: Option<bool>,
}

impl EmbedField {
    /// A block field.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            inline: None,
        }
    }

    /// An inline field.
    pub fn inline(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            inline: Some(true),
            ..Self::new(name, value)
        }
    }
}

/// A message or modal component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Component {
    /// Type 1.
    ActionRow(ActionRow),
    /// Type 2.
    Button(Button),
    /// Type 3.
    SelectMenu(SelectMenu),
    /// Type 4.
    TextInput(TextInput),
}

impl Component {
    /// Discord's numeric type.
    pub fn code(&self) -> u8 {
        match self {
            Self::ActionRow(_) => 1,
            Self::Button(_) => 2,
            Self::SelectMenu(_) => 3,
            Self::TextInput(_) => 4,
        }
    }

    /// A row of components.
    pub fn row(components: Vec<Component>) -> Self {
        Self::ActionRow(ActionRow { components })
    }

    /// The row's components (empty for other components).
    pub fn children(&self) -> &[Component] {
        match self {
            Self::ActionRow(row) => &row.components,
            _ => &[],
        }
    }

    /// The custom id, when the component has one.
    pub fn custom_id(&self) -> Option<&str> {
        match self {
            Self::ActionRow(_) => None,
            Self::Button(button) => button.custom_id.as_deref(),
            Self::SelectMenu(select) => Some(&select.custom_id),
            Self::TextInput(input) => Some(&input.custom_id),
        }
    }
}

#[derive(Serialize)]
struct Tagged<'a, T> {
    #[serde(rename = "type")]
    kind: u8,
    #[serde(flatten)]
    inner: &'a T,
}

impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let kind = self.code();
        match self {
            Self::ActionRow(inner) => Tagged { kind, inner }.serialize(serializer),
            Self::Button(inner) => Tagged { kind, inner }.serialize(serializer),
            Self::SelectMenu(inner) => Tagged { kind, inner }.serialize(serializer),
            Self::TextInput(inner) => Tagged { kind, inner }.serialize(serializer),
        }
    }
}

/// An action row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActionRow {
    /// Children.
    pub components: Vec<Component>,
}

/// Button styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonStyle {
    /// 1 (blurple).
    Primary,
    /// 2 (grey).
    Secondary,
    /// 3 (green).
    Success,
    /// 4 (red).
    Danger,
    /// 5 (opens a URL).
    Link,
}

impl ButtonStyle {
    /// Discord's numeric style.
    pub fn code(self) -> u8 {
        match self {
            Self::Primary => 1,
            Self::Secondary => 2,
            Self::Success => 3,
            Self::Danger => 4,
            Self::Link => 5,
        }
    }
}

impl Serialize for ButtonStyle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

/// A button.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Button {
    /// Custom id (absent for link buttons).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Label.
    pub label: String,
    /// Style.
    pub style: ButtonStyle,
    /// Link target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Greyed out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// A string select menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SelectMenu {
    /// Custom id.
    pub custom_id: String,
    /// Placeholder.
    pub placeholder: String,
    /// Always 1.
    pub min_values: u8,
    /// Always 1.
    pub max_values: u8,
    /// Options.
    pub options: Vec<SelectOption>,
}

/// A select option.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SelectOption {
    /// Label.
    pub label: String,
    /// Value.
    pub value: String,
    /// Preselected.
    pub default: bool,
}

/// Text input styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputStyle {
    /// 1: one line.
    Short,
    /// 2: multi-line.
    Paragraph,
}

impl Serialize for TextInputStyle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(match self {
            Self::Short => 1,
            Self::Paragraph => 2,
        })
    }
}

/// A modal text input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TextInput {
    /// Custom id (the field name on submit).
    pub custom_id: String,
    /// Label (at most 45 characters).
    pub label: String,
    /// Style.
    pub style: TextInputStyle,
    /// Required.
    pub required: bool,
    /// Maximum length.
    pub max_length: u16,
    /// Prefilled value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Placeholder.
    pub placeholder: String,
}

/// Application command option types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    /// 3.
    String,
    /// 4.
    Integer,
    /// 6.
    User,
}

impl Serialize for OptionKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(match self {
            Self::String => 3,
            Self::Integer => 4,
            Self::User => 6,
        })
    }
}

/// A slash command definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandDefinition {
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Usable in DMs.
    pub dm_permission: bool,
    /// Options.
    pub options: Vec<CommandOption>,
}

/// A slash command option.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandOption {
    /// Type.
    #[serde(rename = "type")]
    pub kind: OptionKind,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Required.
    pub required: bool,
    /// Maximum string length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u16>,
    /// Minimum integer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<i64>,
    /// Maximum integer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<i64>,
}

impl CommandOption {
    /// An optional option.
    pub fn optional(kind: OptionKind, name: &str, description: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            description: description.to_owned(),
            required: false,
            max_length: None,
            min_value: None,
            max_value: None,
        }
    }
}

/// A private, mention-free message (`%{content: c, flags: 64, allowed_mentions: %{parse: []}}`).
pub fn private(content: impl Into<String>) -> MessagePayload {
    MessagePayload {
        content: Some(content.into()),
        flags: Some(EPHEMERAL),
        allowed_mentions: Some(AllowedMentions::none()),
        ..MessagePayload::default()
    }
}

/// Truncates to `limit` characters.
pub fn slice(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_components_with_numeric_types() {
        let row = Component::row(vec![Component::Button(Button {
            custom_id: Some("won:1:save".into()),
            label: "Save game".into(),
            style: ButtonStyle::Success,
            url: None,
            disabled: None,
        })]);
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            json!({"type": 1, "components": [{"type": 2, "custom_id": "won:1:save", "label": "Save game", "style": 3}]})
        );
        assert_eq!(
            serde_json::to_value(InteractionResponse::deferred(false)).unwrap(),
            json!({"type": 5})
        );
        assert_eq!(
            serde_json::to_value(InteractionResponse::deferred(true)).unwrap(),
            json!({"type": 5, "data": {"flags": 64}})
        );
        assert_eq!(
            DiscordError::Http {
                status: 403,
                code: Some(50_013)
            }
            .to_string(),
            "HTTP 403, Discord code 50013"
        );
    }
}
