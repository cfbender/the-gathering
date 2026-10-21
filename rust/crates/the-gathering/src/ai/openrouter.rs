//! OpenRouter's chat completions API: checking credentials and running a tool-calling
//! conversation to its final answer.
//!
//! Every round of a conversation resends the system prompt, the tools, and the earlier
//! rounds, so requests mark cache breakpoints after the system prompt (shared by every
//! question) and after the newest message (the prefix the next round resends), and carry
//! the conversation's `session_id` so OpenRouter keeps routing it to the provider holding
//! the cache. Providers that cache on their own (`OpenAI`, newer Gemini) ignore the marks.

use std::future::Future;
use std::time::Duration;

use serde_json::{Value, json};

use crate::validation::ValidationError;

/// One completion request may take this long (reasoning models are slow).
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(120);
/// Credential checks.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// Room for reasoning and the answer.
const MAX_TOKENS: u32 = 16_000;

/// An OpenRouter account and model.
#[derive(Clone, Copy, Debug)]
pub struct Client<'a> {
    /// Shared HTTP client.
    pub http: &'a reqwest::Client,
    /// API base, such as `https://openrouter.ai/api/v1`.
    pub base: &'a str,
    /// API key.
    pub api_key: &'a str,
    /// Model id.
    pub model: &'a str,
    /// Identifies the conversation, so its requests reach the provider holding its cache.
    pub session_id: &'a str,
}

/// Why a conversation failed, in words fit to show the person who asked.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ChatError(pub String);

/// Why saved credentials were refused.
#[derive(Debug)]
pub enum CheckError {
    /// OpenRouter refused the key or does not offer the model.
    Invalid(ValidationError),
    /// OpenRouter could not be asked.
    Unreachable(String),
}

fn request(
    http: &reqwest::Client,
    method: reqwest::Method,
    url: String,
    api_key: &str,
) -> reqwest::RequestBuilder {
    http.request(method, url)
        .bearer_auth(api_key)
        .header("accept", "application/json")
        .header("http-referer", "https://github.com/cfbender/the-gathering")
        .header("x-title", "The Gathering")
}

/// OpenRouter's error message in a failed response body.
fn error_message(body: &Value) -> Option<String> {
    let error = body.get("error")?;
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(str::to_owned)
}

async fn read(response: reqwest::Response) -> (reqwest::StatusCode, Value) {
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

/// Checks that OpenRouter accepts `api_key` and offers `model` with tool calling, which
/// rulings need.
pub async fn check(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
) -> Result<(), CheckError> {
    let unreachable = |error: reqwest::Error| {
        CheckError::Unreachable(format!("Could not reach OpenRouter: {error}"))
    };
    let response = request(http, reqwest::Method::GET, format!("{base}/key"), api_key)
        .timeout(CHECK_TIMEOUT)
        .send()
        .await
        .map_err(unreachable)?;
    let (status, body) = read(response).await;
    if matches!(status.as_u16(), 401 | 403) {
        return Err(CheckError::Invalid(ValidationError::single(
            "api_key",
            "was rejected by OpenRouter",
        )));
    }
    if !status.is_success() {
        return Err(CheckError::Unreachable(format!(
            "OpenRouter could not check the key ({status}){}",
            error_message(&body).map_or_else(String::new, |message| format!(": {message}"))
        )));
    }
    let response = request(
        http,
        reqwest::Method::GET,
        format!("{base}/models"),
        api_key,
    )
    .timeout(CHECK_TIMEOUT)
    .send()
    .await
    .map_err(unreachable)?;
    let (status, body) = read(response).await;
    if !status.is_success() {
        return Err(CheckError::Unreachable(format!(
            "OpenRouter could not list its models ({status})"
        )));
    }
    let listed = body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|models| {
            models
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(model))
        });
    let Some(listed) = listed else {
        return Err(CheckError::Invalid(ValidationError::single(
            "model",
            "is not a model OpenRouter offers",
        )));
    };
    // Models that list their parameters must list tools; others are given the benefit.
    let tools = listed
        .get("supported_parameters")
        .and_then(Value::as_array)
        .is_none_or(|parameters| parameters.iter().any(|value| value == "tools"));
    if !tools {
        return Err(CheckError::Invalid(ValidationError::single(
            "model",
            "does not support tool calling, which rulings need to look up rules and cards",
        )));
    }
    Ok(())
}

/// A tool call the model asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    /// Its id, echoed in the result.
    pub id: Value,
    /// The tool's name.
    pub name: String,
    /// Its arguments (an empty object when they were not valid JSON).
    pub arguments: Value,
}

fn tool_calls(message: &Value) -> Vec<ToolCall> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let function = call.get("function")?;
                    let arguments = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .and_then(|text| serde_json::from_str(text).ok())
                        .unwrap_or_else(|| json!({}));
                    Some(ToolCall {
                        id: call.get("id").cloned().unwrap_or(Value::Null),
                        name: function.get("name")?.as_str()?.to_owned(),
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `messages` with cache breakpoints after the first message (the system prompt) and the
/// last one. Marked messages carry their text as one content part, the form breakpoints
/// attach to.
fn with_cache_breakpoints(messages: &[Value]) -> Vec<Value> {
    let last = messages.len().saturating_sub(1);
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let mut message = message.clone();
            if (index == 0 || index == last)
                && let Some(text) = message.get("content").and_then(Value::as_str)
            {
                let part = json!([{
                    "type": "text",
                    "text": text,
                    "cache_control": { "type": "ephemeral" },
                }]);
                if let Some(object) = message.as_object_mut() {
                    object.insert("content".into(), part);
                }
            }
            message
        })
        .collect()
}

/// What a conversation cost, summed over its requests from OpenRouter's `usage` blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    /// Charged to the account, in USD (OpenRouter credits).
    pub cost_usd: f64,
    /// Prompt tokens.
    pub prompt_tokens: i64,
    /// Completion tokens, reasoning included.
    pub completion_tokens: i64,
    /// Prompt tokens read from the provider's cache (part of `prompt_tokens`).
    pub cached_tokens: i64,
    /// Prompt tokens written to the provider's cache (part of `prompt_tokens`).
    pub cache_write_tokens: i64,
}

impl Usage {
    fn add(&mut self, response: &Value) {
        let Some(usage) = response.get("usage") else {
            return;
        };
        let number = |key: &str| usage.get(key).and_then(Value::as_i64).unwrap_or(0);
        self.cost_usd += usage
            .get("cost")
            .and_then(Value::as_f64)
            .filter(|cost| cost.is_finite() && *cost > 0.0)
            .unwrap_or(0.0);
        self.prompt_tokens = self.prompt_tokens.saturating_add(number("prompt_tokens"));
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(number("completion_tokens"));
        let details = |key: &str| {
            usage
                .get("prompt_tokens_details")
                .and_then(|details| details.get(key))
                .and_then(Value::as_i64)
                .unwrap_or(0)
        };
        self.cached_tokens = self.cached_tokens.saturating_add(details("cached_tokens"));
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(details("cache_write_tokens"));
    }
}

/// The final answer and the model that wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The answer text.
    pub content: String,
    /// The model OpenRouter used.
    pub model: String,
}

impl Client<'_> {
    async fn post(&self, body: &Value, usage: &mut Usage) -> Result<Value, ChatError> {
        let response = request(
            self.http,
            reqwest::Method::POST,
            format!("{}/chat/completions", self.base),
            self.api_key,
        )
        .timeout(COMPLETION_TIMEOUT)
        .json(body)
        .send()
        .await
        .map_err(|error| {
            ChatError(if error.is_timeout() {
                "OpenRouter took too long to answer. Try again.".to_owned()
            } else {
                format!("Could not reach OpenRouter: {error}")
            })
        })?;
        let (status, body) = read(response).await;
        usage.add(&body);
        // OpenRouter reports some failures (such as a provider error) in a 200 body.
        if let Some(message) = error_message(&body) {
            return Err(ChatError(format!("OpenRouter could not answer: {message}")));
        }
        if !status.is_success() {
            return Err(ChatError(format!(
                "OpenRouter could not answer ({status})."
            )));
        }
        Ok(body)
    }

    /// Runs the conversation: each round sends `messages`, runs the tools the model calls
    /// with `call`, and appends the results, until the model answers. After `max_rounds`
    /// rounds of tool calls the model must answer without tools. Adds what every request
    /// cost to `usage`, including requests before a failure.
    pub async fn converse<F, Fut>(
        &self,
        mut messages: Vec<Value>,
        tools: &Value,
        max_rounds: usize,
        usage: &mut Usage,
        mut call: F,
    ) -> Result<Completion, ChatError>
    where
        F: FnMut(ToolCall) -> Fut,
        Fut: Future<Output = Value>,
    {
        for round in 0..=max_rounds {
            let mut body = json!({
                "model": self.model,
                "session_id": self.session_id,
                "messages": with_cache_breakpoints(&messages),
                "tools": tools,
                "max_tokens": MAX_TOKENS,
            });
            if round == max_rounds
                && let Some(body) = body.as_object_mut()
            {
                body.insert("tool_choice".into(), json!("none"));
            }
            let response = self.post(&body, usage).await?;
            let message = response
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("message"))
                .cloned()
                .ok_or_else(|| ChatError("OpenRouter sent no answer.".to_owned()))?;
            let calls = tool_calls(&message);
            if calls.is_empty() || round == max_rounds {
                let content = message
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|content| !content.is_empty())
                    .ok_or_else(|| ChatError("The model returned an empty answer.".to_owned()))?;
                let model = response
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or(self.model);
                return Ok(Completion {
                    content: content.to_owned(),
                    model: model.to_owned(),
                });
            }
            // The assistant turn goes back as sent (with any reasoning), then each result.
            messages.push(message);
            for tool_call in calls {
                let id = tool_call.id.clone();
                let name = tool_call.name.clone();
                let result = call(tool_call).await;
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "name": name,
                    "content": result.to_string(),
                }));
            }
        }
        Err(ChatError("The model did not finish its answer.".to_owned()))
    }
}
