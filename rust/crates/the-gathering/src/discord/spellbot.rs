//! Parses SpellBot's edited `Your game is ready!` embed (`SpellBotParser`).
//!
//! [`SpellBotMessage`] is a lenient view of a Discord message: every field is optional, so
//! partial updates and unrelated messages are rejected with a reason instead of an error.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::db::UtcDateTime;
use crate::regex::compile;

use super::report::{GameReport, ReportPlayer};

/// SpellBot's "started" embed color.
const STARTED_COLOR: u32 = 0x00F8_AE4A;

/// A message as far as the parser reads it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SpellBotMessage {
    /// Message id.
    #[serde(deserialize_with = "snowflake")]
    pub id: Option<String>,
    /// Guild id.
    #[serde(deserialize_with = "snowflake")]
    pub guild_id: Option<String>,
    /// Channel id.
    #[serde(deserialize_with = "snowflake")]
    pub channel_id: Option<String>,
    /// Author.
    pub author: Option<Author>,
    /// Message type.
    #[serde(rename = "type")]
    pub kind: Option<u8>,
    /// Embeds.
    pub embeds: Option<Vec<MessageEmbed>>,
}

/// A message author.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Author {
    /// Snowflake.
    #[serde(deserialize_with = "snowflake")]
    pub id: Option<String>,
    /// Whether the author is a bot.
    pub bot: Option<bool>,
}

/// An embed.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MessageEmbed {
    /// Title.
    pub title: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Color.
    pub color: Option<u32>,
    /// Footer.
    pub footer: Option<Footer>,
    /// Fields.
    pub fields: Vec<Field>,
}

/// An embed footer.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Footer {
    /// Text.
    pub text: Option<String>,
}

/// An embed field.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Field {
    /// Name.
    pub name: Option<String>,
    /// Value.
    pub value: Option<String>,
}

fn snowflake<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::String(value)) => Some(value),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    })
}

impl From<&twilight_model::channel::Message> for SpellBotMessage {
    fn from(message: &twilight_model::channel::Message) -> Self {
        Self {
            id: Some(message.id.to_string()),
            guild_id: message.guild_id.map(|id| id.to_string()),
            channel_id: Some(message.channel_id.to_string()),
            author: Some(Author {
                id: Some(message.author.id.to_string()),
                bot: Some(message.author.bot),
            }),
            kind: Some(u8::from(message.kind)),
            embeds: Some(
                message
                    .embeds
                    .iter()
                    .map(|embed| MessageEmbed {
                        title: embed.title.clone(),
                        description: embed.description.clone(),
                        color: embed.color,
                        footer: embed.footer.as_ref().map(|footer| Footer {
                            text: Some(footer.text.clone()),
                        }),
                        fields: embed
                            .fields
                            .iter()
                            .map(|field| Field {
                                name: Some(field.name.clone()),
                                value: Some(field.value.clone()),
                            })
                            .collect(),
                    })
                    .collect(),
            ),
        }
    }
}

/// Why a message is not a started SpellBot game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Another author.
    #[error("not_spellbot")]
    NotSpellbot,
    /// No embeds (placeholders and text replies).
    #[error("no_embeds")]
    NoEmbeds,
    /// Embeds, but no started-game embed.
    #[error("not_started_game")]
    NotStartedGame,
    /// No valid `Started at` field.
    #[error("invalid_started_at")]
    InvalidStartedAt,
    /// A required field is missing.
    #[error("missing_field")]
    MissingField,
    /// No player lines.
    #[error("missing_players")]
    MissingPlayers,
}

/// Parses a SpellBot game message into a report.
pub fn parse(message: &SpellBotMessage, spellbot_user_id: &str) -> Result<GameReport, ParseError> {
    let author = message.author.as_ref();
    let is_spellbot = author.is_some_and(|author| {
        author.bot == Some(true) && author.id.as_deref().unwrap_or_default() == spellbot_user_id
    });
    if !is_spellbot {
        return Err(ParseError::NotSpellbot);
    }
    let (embed, game_id) = find_started_embed(message.embeds.as_deref().unwrap_or_default())?;
    let played_at = parse_played_at(embed)?;
    let players = parse_players(embed)?;
    if players.is_empty() {
        return Err(ParseError::MissingPlayers);
    }
    Ok(GameReport {
        external_id: format!("spellbot:{game_id}"),
        source: "discord".into(),
        played_at,
        guild_id: message.guild_id.clone().unwrap_or_default(),
        channel_id: message.channel_id.clone().unwrap_or_default(),
        players,
        winner_discord_ids: Vec::new(),
        raw: raw(message, embed),
        details: None,
    })
}

fn find_started_embed(embeds: &[MessageEmbed]) -> Result<(&MessageEmbed, String), ParseError> {
    if embeds.is_empty() {
        return Err(ParseError::NoEmbeds);
    }
    let footer_pattern = compile(r"^SpellBot Game ID: #(SB\d+) — Service: .+$");
    embeds
        .iter()
        .find_map(|embed| {
            if embed.title.as_deref() != Some("**Your game is ready!**")
                || embed.color != Some(STARTED_COLOR)
            {
                return None;
            }
            let footer = embed
                .footer
                .as_ref()
                .and_then(|footer| footer.text.as_deref())
                .unwrap_or_default();
            footer_pattern
                .captures(footer)
                .and_then(|captures| captures.get(1))
                .map(|id| (embed, id.as_str().to_owned()))
        })
        .ok_or(ParseError::NotStartedGame)
}

fn field_value<'a>(embed: &'a MessageEmbed, name: &str) -> Result<&'a str, ParseError> {
    embed
        .fields
        .iter()
        .find(|field| field.name.as_deref() == Some(name))
        .map(|field| field.value.as_deref().unwrap_or_default())
        .ok_or(ParseError::MissingField)
}

fn parse_played_at(embed: &MessageEmbed) -> Result<UtcDateTime, ParseError> {
    let value = field_value(embed, "Started at").map_err(|_| ParseError::InvalidStartedAt)?;
    compile(r"^<t:(\d+)>$")
        .captures(value)
        .and_then(|captures| captures.get(1))
        .and_then(|unix| unix.as_str().parse::<i64>().ok())
        .and_then(UtcDateTime::from_unix)
        .ok_or(ParseError::InvalidStartedAt)
}

fn parse_players(embed: &MessageEmbed) -> Result<Vec<ReportPlayer>, ParseError> {
    let value = field_value(embed, "Players")?;
    let pattern = compile(r"<@!?(\d+)>\s+\((.*)\)\s*$");
    Ok(value
        .split('\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let captures = pattern.captures(line)?;
            Some(ReportPlayer {
                discord_id: captures.get(1)?.as_str().to_owned(),
                display_name: captures.get(2)?.as_str().to_owned(),
                commander_name: None,
            })
        })
        .collect())
}

fn raw(message: &SpellBotMessage, embed: &MessageEmbed) -> Map<String, Value> {
    let fields: Vec<Value> = embed
        .fields
        .iter()
        .map(|field| json!({ "name": field.name, "value": field.value }))
        .collect();
    let value = json!({
        "message_id": message.id.clone().unwrap_or_default(),
        "author_id": message.author.as_ref().and_then(|author| author.id.clone()).unwrap_or_default(),
        "embed": {
            "title": embed.title,
            "description": embed.description,
            "color": embed.color,
            "footer": embed.footer.as_ref().and_then(|footer| footer.text.clone()),
            "fields": fields,
        }
    });
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
