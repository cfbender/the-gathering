//! `TheGathering.CloudflareTurn`: mints short-lived Cloudflare Realtime TURN credentials for
//! webcam tables.
//!
//! A Cloudflare TURN key (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`) is a
//! long-term secret that stays on the server. Each webcam-table config request exchanges it
//! for ICE servers whose username/credential expire after `ttl_seconds`.

use std::sync::LazyLock;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::config::CloudflareTurnConfig;
use crate::regex::{Regex, compile};

/// Cloudflare returns six TURN URLs; only UDP on 3478 and TLS on 443 are passed on (a browser
/// opens one relay allocation per URL, and Firefox warns that five or more slow discovery).
static PREFERRED_TURN_URLS: LazyLock<[Regex; 2]> = LazyLock::new(|| {
    [compile(r"^turn:[^?]*:3478\?transport=udp$"), compile(r"^turns:[^?]*:443\?transport=tcp$")]
});

fn present(value: Option<&String>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// True when a TURN key id and API token are configured.
pub fn configured(config: &CloudflareTurnConfig) -> bool {
    present(config.key_id.as_ref()) && present(config.api_token.as_ref())
}

/// Why minting failed.
#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    /// Cloudflare answered with a non-2xx status or an unexpected body.
    #[error("Cloudflare TURN credential request failed with {0}")]
    Status(u16),
    /// The request failed.
    #[error("Cloudflare TURN credential request failed: {0}")]
    Request(#[from] reqwest::Error),
}

/// Requests ICE servers carrying fresh TURN credentials: maps with `urls` and, for the TURN
/// entry, `username` and `credential`.
pub async fn ice_servers(http: &reqwest::Client, config: &CloudflareTurnConfig) -> Result<Vec<Value>, TurnError> {
    let url = format!(
        "{}/v1/turn/keys/{}/credentials/generate-ice-servers",
        config.api_base.trim_end_matches('/'),
        config.key_id.as_deref().unwrap_or_default()
    );
    let response = http
        .post(url)
        .bearer_auth(config.api_token.as_deref().unwrap_or_default())
        .json(&json!({ "ttl": config.ttl_seconds }))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .inspect_err(|error| tracing::warn!("Cloudflare TURN credential request failed: {error}"))?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    match body.get("iceServers").and_then(Value::as_array) {
        Some(servers) if status.is_success() => Ok(servers.iter().map(|server| prefer_urls(take(server))).collect()),
        _ => {
            tracing::warn!("Cloudflare TURN credential request failed with {status}: {body}");
            Err(TurnError::Status(status.as_u16()))
        }
    }
}

fn take(server: &Value) -> Value {
    let mut taken = Map::new();
    for key in ["urls", "username", "credential"] {
        if let Some(value) = server.get(key) {
            taken.insert(key.into(), value.clone());
        }
    }
    Value::Object(taken)
}

/// Keeps the preferred TURN URLs when the response has them; an unfamiliar URL set (or a
/// STUN-only entry) passes through untouched rather than losing the relay.
fn prefer_urls(mut server: Value) -> Value {
    let Some(urls) = server.get("urls").and_then(Value::as_array) else {
        return server;
    };
    let preferred: Vec<Value> = urls
        .iter()
        .filter(|url| url.as_str().is_some_and(|url| PREFERRED_TURN_URLS.iter().any(|regex| regex.is_match(url))))
        .cloned()
        .collect();
    if !preferred.is_empty()
        && let Some(object) = server.as_object_mut()
    {
        object.insert("urls".into(), Value::Array(preferred));
    }
    server
}

/// Relay servers for the SFU's relay-only mode (`Sfu.relay_servers/0`): TURN entries with
/// credentials, or none when Cloudflare is unavailable.
pub async fn relay_servers(http: &reqwest::Client, config: &CloudflareTurnConfig) -> Vec<the_gathering_sfu::IceServer> {
    let Ok(servers) = ice_servers(http, config).await else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|server| {
            let username = server.get("username").and_then(Value::as_str)?;
            let urls = match server.get("urls")? {
                Value::String(url) => vec![url.clone()],
                Value::Array(urls) => urls.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
                _ => return None,
            };
            Some(the_gathering_sfu::IceServer {
                urls,
                username: Some(username.to_owned()),
                credential: server.get("credential").and_then(Value::as_str).map(str::to_owned),
            })
        })
        .collect()
}
