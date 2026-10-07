//! Scryfall API calls the app makes on demand: printing
//! searches, single printings, rulings, and the bulk-data download. Requests go through
//! lotus's [`ScryfallClient`]; the shared per-server limit lives here.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lotus::scryfall::{ScryfallClient, ScryfallError};
use serde_json::Value;
use tokio::time::Instant;

use crate::config::WindowLimit;
use crate::db::UtcDateTime;
use crate::rate_limit::{Decision, RateLimiter};

/// Every seat at a webcam table looks up a newly identified card at the same moment, so
/// card lookups queue for the shared limit briefly instead of failing all but the first.
const CARD_QUEUE: Duration = Duration::from_secs(3);

/// Why a Scryfall request failed, as the controllers report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Scryfall has no such resource (404).
    NotFound,
    /// Rate limited, unreachable, or an unexpected answer (502).
    BadGateway,
}

/// The Scryfall client with the shared request limit.
#[derive(Debug)]
pub struct Scryfall {
    client: ScryfallClient,
    limiter: RateLimiter,
    limit: u64,
}

fn failure(error: &ScryfallError) -> Failure {
    match error {
        ScryfallError::NotFound => Failure::NotFound,
        _ => Failure::BadGateway,
    }
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

impl Scryfall {
    /// A client for `api_base` allowing `limit` requests per window.
    pub fn new(api_base: &str, limit: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(crate::state::user_agent())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client: ScryfallClient::with_client(http, api_base.trim_end_matches('/')),
            limiter: RateLimiter::new(),
            limit,
        })
    }

    /// The shared limiter (tests take a slot to simulate another seat's request).
    pub fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }

    fn window(&self, millis: u64) -> WindowLimit {
        WindowLimit {
            limit: self.limit,
            scale: Duration::from_millis(millis),
        }
    }

    fn allow(&self, key: &str, millis: u64) -> Result<(), Failure> {
        match self.limiter.hit(key, self.window(millis)) {
            Decision::Allow(_) => Ok(()),
            Decision::Deny(_) => Err(Failure::BadGateway),
        }
    }

    async fn await_slot(&self, key: &str, millis: u64) -> Result<(), Failure> {
        let deadline = Instant::now() + CARD_QUEUE;
        loop {
            match self.limiter.hit(key, self.window(millis)) {
                Decision::Allow(_) => return Ok(()),
                Decision::Deny(retry_ms) => {
                    let retry = Duration::from_millis(retry_ms.max(1));
                    if Instant::now() + retry > deadline {
                        return Err(Failure::BadGateway);
                    }
                    tokio::time::sleep(retry).await;
                }
            }
        }
    }

    /// One page of every English paper printing of an oracle id, oldest first: the raw
    /// card objects and whether another page follows. No match is an empty page.
    pub async fn printings(
        &self,
        oracle_id: &str,
        page: u32,
    ) -> Result<(Vec<Value>, bool), Failure> {
        self.allow("scryfall_search", 500)?;
        let query = format!("oracleid:{oracle_id} game:paper lang:en");
        let url = format!(
            "{}/cards/search?q={}&unique=prints&order=released&include_variations=true&page={page}",
            self.client.api_base(),
            encode(&query)
        );
        match self.client.get_json::<Value>(&url).await {
            Ok(mut body) => match (body.get_mut("data").map(Value::take), body.get("has_more")) {
                (Some(Value::Array(cards)), Some(Value::Bool(has_more))) => Ok((cards, *has_more)),
                _ => Err(Failure::BadGateway),
            },
            Err(ScryfallError::NotFound) => Ok((Vec::new(), false)),
            Err(_) => Err(Failure::BadGateway),
        }
    }

    /// One printing by Scryfall id, as Scryfall serves it.
    pub async fn card(&self, id: &str) -> Result<Value, Failure> {
        self.await_slot("scryfall_card", 100).await?;
        let url = format!("{}/cards/{}", self.client.api_base(), encode(id));
        match self.client.get_json::<Value>(&url).await {
            Ok(card) if card.get("id").is_some() => Ok(card),
            Ok(_) => Err(Failure::BadGateway),
            Err(error) => Err(failure(&error)),
        }
    }

    /// A printing's rulings with only their public fields (`source`, `published_at`,
    /// `comment`).
    pub async fn rulings(&self, id: &str) -> Result<Vec<Value>, Failure> {
        self.allow("scryfall_rulings", 100)?;
        let url = format!("{}/cards/{}/rulings", self.client.api_base(), encode(id));
        let body = self
            .client
            .get_json::<Value>(&url)
            .await
            .map_err(|error| failure(&error))?;
        let Some(Value::Array(rulings)) = body.get("data") else {
            return Err(Failure::BadGateway);
        };
        Ok(rulings
            .iter()
            .filter_map(Value::as_object)
            .map(|ruling| {
                let kept: serde_json::Map<String, Value> = ruling
                    .iter()
                    .filter(|(key, _)| {
                        matches!(key.as_str(), "source" | "published_at" | "comment")
                    })
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                Value::Object(kept)
            })
            .collect())
    }

    /// `fetch/0`: downloads the `default_cards` bulk file into `dir`, returning its path
    /// and Scryfall's generation time.
    pub async fn download_bulk(
        &self,
        dir: &Path,
    ) -> anyhow::Result<(PathBuf, Option<UtcDateTime>)> {
        let list = self.client.bulk_data_list().await?;
        let metadata = list.find("default_cards").ok_or_else(|| {
            anyhow::anyhow!("Scryfall did not return default_cards bulk metadata")
        })?;
        let uri = metadata
            .download_uri()
            .map_err(|_| anyhow::anyhow!("Scryfall default_cards has no JSONL URI"))?;
        let path = dir.join(format!(
            "the-gathering-scryfall-{}.jsonl.gz",
            uuid::Uuid::new_v4()
        ));
        match self.client.download_to_file(uri, &path).await {
            Ok(_) => Ok((path, metadata.updated_at.map(UtcDateTime::from_offset))),
            Err(ScryfallError::Status(status)) => {
                anyhow::bail!("Scryfall bulk download returned HTTP {}", status.as_u16())
            }
            Err(error) => Err(error.into()),
        }
    }
}
