//! The Phoenix Channels V2 JSON wire format: `[join_ref, ref, topic, event, payload]`.

use axum::extract::ws::Utf8Bytes;
use serde_json::{Value, json};

/// A frame queued for the socket's writer.
#[derive(Clone, Debug)]
pub enum Outbound {
    /// A text frame.
    Text(Utf8Bytes),
    /// Close the connection.
    Close,
}

/// A decoded client frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The channel join this frame belongs to.
    pub join_ref: Option<String>,
    /// The client's message ref, echoed in the reply.
    pub ref_: Option<String>,
    /// Topic.
    pub topic: String,
    /// Event name.
    pub event: String,
    /// Payload.
    pub payload: Value,
}

impl Frame {
    /// Decodes a text frame.
    pub fn decode(text: &str) -> Option<Self> {
        let (join_ref, ref_, topic, event, payload): (Option<Value>, Option<Value>, String, String, Value) =
            serde_json::from_str(text).ok()?;
        Some(Self { join_ref: join_ref.and_then(ref_string), ref_: ref_.and_then(ref_string), topic, event, payload })
    }
}

/// Refs are strings in phoenix.js; accept numbers too.
fn ref_string(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Encodes a server frame.
pub fn encode(join_ref: Option<&str>, ref_: Option<&str>, topic: &str, event: &str, payload: &Value) -> String {
    json!([join_ref, ref_, topic, event, payload]).to_string()
}

/// How a request was answered (`{:reply, :ok | {:ok, map} | {:error, map}, socket}`).
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// `status: "ok"`.
    Ok(Value),
    /// `status: "error"`.
    Error(Value),
}

impl Reply {
    /// `:ok`: an empty response.
    pub fn ok() -> Self {
        Self::Ok(json!({}))
    }

    /// `{:error, %{reason: reason}}`.
    pub fn reason(reason: impl Into<String>) -> Self {
        Self::Error(json!({ "reason": reason.into() }))
    }

    /// The `phx_reply` payload.
    pub fn payload(&self) -> Value {
        match self {
            Self::Ok(response) => json!({ "status": "ok", "response": response }),
            Self::Error(response) => json!({ "status": "error", "response": response }),
        }
    }
}

impl From<Result<(), String>> for Reply {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(reason) => Self::reason(reason),
        }
    }
}
