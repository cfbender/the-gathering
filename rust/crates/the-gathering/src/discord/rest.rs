//! Discord's REST API over `reqwest`.
//!
//! Bot-authenticated routes (messages, commands) wait out a 429 and retry a few times;
//! message creation is idempotent through `nonce`/`enforce_nonce`. Interaction callbacks
//! and webhook edits authenticate with the interaction token instead of the bot token
//! and are never retried, redirected, or left waiting without a limit
//! (`SummaryUpload.edit_response/3`). Errors keep only numeric codes, so neither tokens
//! nor response bodies reach the logs.

use std::fmt::Write as _;
use std::time::Duration;

use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::api::{
    ApiFuture, CommandDefinition, DiscordApi, DiscordError, InteractionResponse, InteractionTarget,
    MessagePayload, RegisteredCommand, SentMessage,
};

/// Discord's API root.
pub const API_BASE: &str = "https://discord.com/api/v10";

/// How many times a bot route is retried after a 429.
const RATE_LIMIT_RETRIES: u32 = 3;

/// The REST client.
#[derive(Clone)]
pub struct RestApi {
    http: reqwest::Client,
    token: String,
    base: String,
}

impl std::fmt::Debug for RestApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the token.
        f.debug_struct("RestApi")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl RestApi {
    /// A client for `token` against [`API_BASE`].
    pub fn new(token: &str) -> Result<Self, reqwest::Error> {
        Self::with_base(token, API_BASE, Duration::from_secs(15))
    }

    /// A client against another API root (tests), with a request timeout.
    pub fn with_base(token: &str, base: &str, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .user_agent("DiscordBot (https://github.com/cfbender/the-gathering, 0.1.0)")
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3).min(timeout))
            .timeout(timeout)
            .build()?;
        Ok(Self {
            http,
            token: token.to_owned(),
            base: base.trim_end_matches('/').to_owned(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn bot(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, self.url(path))
            .header("authorization", format!("Bot {}", self.token))
    }

    /// Sends a bot-authenticated request, waiting out rate limits.
    async fn send_bot<B: Serialize + Sync>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Response, DiscordError> {
        let mut attempt = 0;
        loop {
            let mut request = self.bot(method.clone(), path);
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(transport)?;
            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < RATE_LIMIT_RETRIES {
                attempt += 1;
                let wait = retry_after(response).await;
                tokio::time::sleep(wait).await;
                continue;
            }
            return check(response).await;
        }
    }

    async fn bot_json<B: Serialize + Sync, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, DiscordError> {
        let response = self.send_bot(method, path, body).await?;
        response.json().await.map_err(transport)
    }

    fn commands_path(application_id: &str, guild_id: Option<&str>) -> String {
        match guild_id {
            Some(guild) => format!("/applications/{application_id}/guilds/{guild}/commands"),
            None => format!("/applications/{application_id}/commands"),
        }
    }
}

/// Percent-encodes everything but RFC 3986 unreserved characters
/// (`URI.encode(token, &URI.char_unreserved?/1)`).
fn encode_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[allow(clippy::needless_pass_by_value)] // Used as `map_err(transport)`.
fn transport(error: reqwest::Error) -> DiscordError {
    if error.is_timeout() {
        DiscordError::Timeout
    } else if error.is_connect() {
        DiscordError::Network
    } else {
        DiscordError::Transport
    }
}

async fn retry_after(response: Response) -> Duration {
    let seconds = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| body.get("retry_after").and_then(serde_json::Value::as_f64))
        .unwrap_or(1.0)
        .clamp(0.0, 30.0);
    Duration::from_secs_f64(seconds)
}

async fn check(response: Response) -> Result<Response, DiscordError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let code = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| body.get("code").and_then(serde_json::Value::as_i64));
    Err(DiscordError::Http {
        status: status.as_u16(),
        code,
    })
}

impl DiscordApi for RestApi {
    fn create_response<'a>(
        &'a self,
        interaction: &'a InteractionTarget,
        response: &'a InteractionResponse,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            let path = format!(
                "/interactions/{}/{}/callback",
                encode_segment(&interaction.id),
                encode_segment(&interaction.token)
            );
            let reply = self
                .http
                .post(self.url(&path))
                .json(response)
                .send()
                .await
                .map_err(transport)?;
            check(reply).await.map(|_| ())
        })
    }

    fn edit_response<'a>(
        &'a self,
        interaction: &'a InteractionTarget,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage> {
        Box::pin(async move {
            let path = format!(
                "/webhooks/{}/{}/messages/@original",
                encode_segment(&interaction.application_id),
                encode_segment(&interaction.token)
            );
            let request = self.http.patch(self.url(&path));
            let request = if message.files.is_empty() {
                request.json(message)
            } else {
                let payload =
                    serde_json::to_string(message).map_err(|_| DiscordError::Transport)?;
                let mut form = reqwest::multipart::Form::new().text("payload_json", payload);
                for (index, file) in message.files.iter().enumerate() {
                    let part = reqwest::multipart::Part::bytes(file.body.clone())
                        .file_name(file.name.clone())
                        .mime_str("image/png")
                        .map_err(|_| DiscordError::Transport)?;
                    form = form.part(format!("files[{index}]"), part);
                }
                request.multipart(form)
            };
            let reply = request.send().await.map_err(transport)?;
            check(reply).await?.json().await.map_err(transport)
        })
    }

    fn create_message<'a>(
        &'a self,
        channel_id: &'a str,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage> {
        Box::pin(async move {
            let path = format!("/channels/{}/messages", encode_segment(channel_id));
            self.bot_json(Method::POST, &path, Some(message)).await
        })
    }

    fn edit_message<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage> {
        Box::pin(async move {
            let path = format!(
                "/channels/{}/messages/{}",
                encode_segment(channel_id),
                encode_segment(message_id)
            );
            self.bot_json(Method::PATCH, &path, Some(message)).await
        })
    }

    fn create_command<'a>(
        &'a self,
        application_id: &'a str,
        guild_id: Option<&'a str>,
        command: &'a CommandDefinition,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            let path = Self::commands_path(application_id, guild_id);
            self.send_bot(Method::POST, &path, Some(command))
                .await
                .map(|_| ())
        })
    }

    fn list_commands<'a>(
        &'a self,
        application_id: &'a str,
        guild_id: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<RegisteredCommand>> {
        Box::pin(async move {
            let path = Self::commands_path(application_id, guild_id);
            self.bot_json::<(), _>(Method::GET, &path, None).await
        })
    }

    fn delete_command<'a>(
        &'a self,
        application_id: &'a str,
        guild_id: Option<&'a str>,
        command_id: &'a str,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            let path = format!(
                "{}/{}",
                Self::commands_path(application_id, guild_id),
                encode_segment(command_id)
            );
            self.send_bot::<()>(Method::DELETE, &path, None)
                .await
                .map(|_| ())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_reserved_token_characters() {
        assert_eq!(encode_segment("a.b-c_d~e"), "a.b-c_d~e");
        assert_eq!(encode_segment("a/b c"), "a%2Fb%20c");
    }
}
