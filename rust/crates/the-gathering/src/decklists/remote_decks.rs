//! Lists and normalizes the public decks on a member's configured deck hosts: Moxfield and Archidekt by username, and the
//! member's own ManaVault instance with their API key.
//!
//! Each source is fetched within a budget (pages, decks, response bytes, and wall time);
//! hitting one keeps what was listed so far and reports why the listing is incomplete.
//! Results are cached per member for five minutes, keyed by a hash of their settings so a
//! changed setting refetches and no API key is stored in the cache.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream;
use lotus::decklist::{Allowlist, Origin, Source};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::time::Instant;

use super::SharedResolver;
use super::cache::{self, TtlCache};
use super::destination;
use crate::accounts::User;
use crate::config::Config;

const COLORS: [&str; 5] = ["W", "U", "B", "R", "G"];
const PUBLIC_ARCHIDEKT: &str = "https://archidekt.com";
const PUBLIC_MOXFIELD: &str = "https://moxfield.com";
const API_KEY_MISSING: &str = "Add a ManaVault API key in Settings to list your decks. Public share links still work individually.";

/// Per-source fetch budget (`@default_limits`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Pages per source.
    pub max_pages: u32,
    /// Decks per source.
    pub max_decks: usize,
    /// Response bytes per source.
    pub max_bytes: u64,
    /// Wall time per source.
    pub duration: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pages: 20,
            max_decks: 500,
            max_bytes: 2_000_000,
            duration: Duration::from_secs(15),
        }
    }
}

/// One listed deck.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RemoteDeck {
    /// Deck name.
    pub name: Option<String>,
    /// Commander names.
    pub commanders: Vec<String>,
    /// Identity letters in WUBRG order.
    pub color_identity: Vec<String>,
    /// The deck's public (or, for private ManaVault decks, owner-only) URL.
    pub url: String,
    /// Which host listed it.
    pub source: Source,
    /// The host's last-updated timestamp, as it sent it.
    pub updated_at: Option<String>,
}

/// How one source fared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceStatus {
    /// The host.
    pub source: Source,
    /// Whether the member configured it.
    pub configured: bool,
    /// Why the listing failed or is incomplete.
    pub error: Option<String>,
}

/// `RemoteDecks.list/1`'s result: every deck, newest first, and each source's status
/// (Moxfield, Archidekt, ManaVault, in that order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RemoteDeckList {
    /// Decks from every source.
    pub decks: Vec<RemoteDeck>,
    /// Per-source status.
    pub sources: Vec<SourceStatus>,
}

enum Listing {
    Done(Vec<RemoteDeck>),
    Truncated(Vec<RemoteDeck>, String),
    Failed(String),
}

#[derive(Debug)]
enum HttpError {
    ByteLimit,
    DurationLimit,
    Blocked,
    Failed,
}

struct Response {
    status: u16,
    body: Option<Value>,
}

struct Budget {
    bytes_left: Mutex<u64>,
    deadline: Instant,
}

impl Budget {
    fn new(limits: Limits) -> Self {
        Self {
            bytes_left: Mutex::new(limits.max_bytes),
            deadline: Instant::now() + limits.duration,
        }
    }

    fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    fn consume(&self, bytes: usize) -> bool {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        let Ok(mut left) = self.bytes_left.lock() else {
            return false;
        };
        if bytes <= *left {
            *left -= bytes;
            true
        } else {
            false
        }
    }
}

fn budget_message(error: &HttpError) -> &'static str {
    match error {
        HttpError::ByteLimit => {
            "Remote listing was truncated after reaching the 2 MB response budget."
        }
        _ => "Remote listing was truncated after reaching the 15 second time budget.",
    }
}

fn budget_truncated(decks: Vec<RemoteDeck>, error: &HttpError) -> Listing {
    Listing::Truncated(decks, budget_message(error).to_owned())
}

fn truncated(decks: Vec<RemoteDeck>, limit: &str) -> Listing {
    Listing::Truncated(
        decks,
        format!("Remote listing was truncated at the {limit} limit."),
    )
}

fn order_colors<'a>(colors: impl IntoIterator<Item = &'a Value>) -> Vec<String> {
    let present: HashSet<&str> = colors.into_iter().filter_map(Value::as_str).collect();
    COLORS
        .iter()
        .filter(|color| present.contains(*color))
        .map(|color| (*color).to_owned())
        .collect()
}

fn array(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

/// `commander_name/1`.
fn commander_name(value: &Value) -> Option<String> {
    if let Some(name) = value.as_str() {
        return Some(name.to_owned());
    }
    let object = value.as_object()?;
    if let Some(name) = object.get("name") {
        return string(Some(name));
    }
    let card = object.get("card")?.as_object()?;
    if let Some(name) = card.get("name") {
        return string(Some(name));
    }
    string(card.get("oracleCard").and_then(|oracle| oracle.get("name")))
}

fn commanders(row: &Value) -> Vec<String> {
    array(row.get("commanders"))
        .iter()
        .filter_map(commander_name)
        .collect()
}

fn id_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn moxfield_deck(row: &Value) -> RemoteDeck {
    let public_id = row
        .get("publicId")
        .filter(|id| !id.is_null())
        .or_else(|| row.get("id"));
    let colors = row
        .get("colorIdentity")
        .filter(|colors| !colors.is_null())
        .or_else(|| row.get("colors"));
    RemoteDeck {
        name: string(row.get("name")),
        commanders: commanders(row),
        color_identity: order_colors(array(colors)),
        url: string(row.get("publicUrl"))
            .unwrap_or_else(|| format!("{PUBLIC_MOXFIELD}/decks/{}", id_text(public_id))),
        source: Source::Moxfield,
        updated_at: string(row.get("lastUpdatedAtUtc")),
    }
}

fn manavault_deck(row: &Value, origin: &str) -> RemoteDeck {
    RemoteDeck {
        name: string(row.get("name")),
        commanders: commanders(row),
        color_identity: order_colors(array(row.get("commanderColorIdentity"))),
        url: string(row.get("public_share_url"))
            .unwrap_or_else(|| format!("{origin}/decks/{}", id_text(row.get("id")))),
        source: Source::ManaVault,
        updated_at: string(row.get("updated_at")),
    }
}

fn archidekt_color(name: &str) -> &str {
    match name {
        "White" => "W",
        "Blue" => "U",
        "Black" => "B",
        "Red" => "R",
        "Green" => "G",
        other => other,
    }
}

/// Members' deck listings.
pub struct RemoteDecks {
    client: reqwest::Client,
    resolver: SharedResolver,
    allowlist: Allowlist,
    moxfield_base: String,
    archidekt_base: String,
    cache: TtlCache<i64, ([u8; 32], RemoteDeckList)>,
    locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    limits: Mutex<Limits>,
}

impl std::fmt::Debug for RemoteDecks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteDecks")
            .field("moxfield_base", &self.moxfield_base)
            .finish_non_exhaustive()
    }
}

fn user_agent() -> String {
    format!(
        "TheGathering/{} deck metadata resolver (+https://github.com/cfbender/the-gathering)",
        env!("CARGO_PKG_VERSION")
    )
}

fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(user_agent())
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .no_proxy()
}

/// `settings_fingerprint/1`: changes whenever a deck-host setting changes.
fn fingerprint(user: &User) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for value in [
        &user.moxfield_username,
        &user.archidekt_username,
        &user.manavault_url,
        &user.manavault_api_key,
    ] {
        match value {
            Some(text) => {
                hasher.update([1]);
                hasher.update(u64::try_from(text.len()).unwrap_or(u64::MAX).to_be_bytes());
                hasher.update(text.as_bytes());
            }
            None => hasher.update([0]),
        }
    }
    hasher.finalize().into()
}

fn blank(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|value| !value.is_empty())
}

impl RemoteDecks {
    /// Built from the configuration; ManaVault hosts resolve through `resolver`.
    pub fn new(config: &Config, resolver: SharedResolver) -> anyhow::Result<Self> {
        Ok(Self {
            client: client_builder().build()?,
            resolver,
            allowlist: Allowlist::parse(config.manavault_allowed_hosts.iter().map(String::as_str)),
            moxfield_base: config.moxfield_api_base.trim_end_matches('/').to_owned(),
            archidekt_base: config.archidekt_api_base.trim_end_matches('/').to_owned(),
            cache: TtlCache::new(cache::TTL, cache::MAX_ENTRIES),
            locks: Mutex::new(HashMap::new()),
            limits: Mutex::new(Limits::default()),
        })
    }

    /// Replaces the per-source budget (tests shrink it).
    pub fn set_limits(&self, limits: Limits) {
        if let Ok(mut current) = self.limits.lock() {
            *current = limits;
        }
    }

    fn limits(&self) -> Limits {
        self.limits
            .lock()
            .map_or_else(|_| Limits::default(), |limits| *limits)
    }

    /// Forgets cached listings.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Drops expired cached listings.
    pub fn sweep(&self) {
        self.cache.sweep();
    }

    /// The cache (tests check that no API key is stored).
    pub fn cache(&self) -> &TtlCache<i64, ([u8; 32], RemoteDeckList)> {
        &self.cache
    }

    /// `list/1`: the member's decks on every configured host. Never fails: each source
    /// reports its own error. Concurrent misses for one member share a single fetch.
    pub async fn list(&self, user: &User) -> RemoteDeckList {
        let print = fingerprint(user);
        if let Some((cached, result)) = self.cache.fetch(&user.id)
            && cached == print
        {
            return result;
        }
        let lock = self
            .locks
            .lock()
            .map(|mut locks| locks.entry(user.id).or_default().clone())
            .unwrap_or_default();
        let _guard = lock.lock().await;
        if let Some((cached, result)) = self.cache.fetch(&user.id)
            && cached == print
        {
            return result;
        }
        let result = self.fetch_all(user).await;
        self.cache.put(user.id, (print, result.clone()));
        result
    }

    async fn fetch_all(&self, user: &User) -> RemoteDeckList {
        let limits = self.limits();
        let moxfield = match blank(user.moxfield_username.as_ref()) {
            None => None,
            Some(username) => Some(
                self.fetch_moxfield(username, &Budget::new(limits), limits)
                    .await,
            ),
        };
        let archidekt = match blank(user.archidekt_username.as_ref()) {
            None => None,
            Some(username) => Some(
                self.fetch_archidekt(username, &Budget::new(limits), limits)
                    .await,
            ),
        };
        let manavault = match (
            blank(user.manavault_url.as_ref()),
            blank(user.manavault_api_key.as_ref()),
        ) {
            (None, _) => None,
            (Some(_), None) => Some(Listing::Failed(API_KEY_MISSING.to_owned())),
            (Some(origin), Some(key)) => Some(
                self.fetch_manavault(origin, key, &Budget::new(limits), limits)
                    .await,
            ),
        };
        let mut decks = Vec::new();
        let mut sources = Vec::new();
        for (source, listing) in [
            (Source::Moxfield, moxfield),
            (Source::Archidekt, archidekt),
            (Source::ManaVault, manavault),
        ] {
            let (configured, error, listed) = match listing {
                None => (false, None, Vec::new()),
                Some(Listing::Done(listed)) => (true, None, listed),
                Some(Listing::Truncated(listed, message)) => (true, Some(message), listed),
                Some(Listing::Failed(message)) => (true, Some(message), Vec::new()),
            };
            decks.extend(listed);
            sources.push(SourceStatus {
                source,
                configured,
                error,
            });
        }
        decks.sort_by(|a, b| {
            let key = |deck: &RemoteDeck| {
                (
                    deck.updated_at.clone().unwrap_or_default(),
                    deck.name.clone().unwrap_or_default(),
                )
            };
            key(b).cmp(&key(a))
        });
        RemoteDeckList { decks, sources }
    }

    /// `HTTP.get_limited/3` (and the request half of `get_remote/4`): a GET whose body
    /// counts against the budget and whose time is bounded by it.
    async fn get_limited(
        &self,
        client: &reqwest::Client,
        url: &str,
        headers: &[(&str, String)],
        budget: &Budget,
    ) -> Result<Response, HttpError> {
        let remaining = budget.remaining();
        if remaining.is_zero() {
            return Err(HttpError::DurationLimit);
        }
        let mut request = client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(remaining.min(Duration::from_secs(8)));
        for (name, value) in headers {
            request = request.header(*name, value);
        }
        let timed_out = |error: &reqwest::Error| {
            if error.is_timeout() && budget.remaining().is_zero() {
                HttpError::DurationLimit
            } else {
                HttpError::Failed
            }
        };
        let response = request.send().await.map_err(|error| timed_out(&error))?;
        let status = response.status().as_u16();
        let mut body = Vec::new();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|error| timed_out(&error))?;
            if !budget.consume(chunk.len()) {
                return Err(HttpError::ByteLimit);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Response {
            status,
            body: serde_json::from_slice(&body).ok(),
        })
    }

    /// `HTTP.get_remote/4`: resolves the member's origin within the budget, applies the
    /// destination policy, and pins the connection to the checked address (keeping the
    /// hostname for TLS and `Host`). Redirects are never followed.
    async fn get_remote(
        &self,
        origin: &str,
        path: &str,
        headers: &[(&str, String)],
        budget: &Budget,
    ) -> Result<Response, HttpError> {
        let remaining = budget.remaining();
        if remaining.is_zero() {
            return Err(HttpError::DurationLimit);
        }
        let resolved = tokio::time::timeout(
            remaining,
            destination::resolve(origin, &self.allowlist, &self.resolver),
        )
        .await
        .map_err(|_| HttpError::DurationLimit)?;
        let (origin, address): (Origin, _) = resolved.map_err(|_| HttpError::Blocked)?;
        let mut builder = client_builder();
        if origin.ip_literal().is_none() {
            builder = builder.resolve(
                &origin.host,
                SocketAddr::new(address, origin.port_or_default()),
            );
        }
        let client = builder.build().map_err(|_| HttpError::Failed)?;
        self.get_limited(&client, &origin.join(path), headers, budget)
            .await
    }

    fn add_rows(
        decks: &mut Vec<RemoteDeck>,
        rows: &[Value],
        limits: Limits,
        mapper: impl Fn(&Value) -> RemoteDeck,
    ) -> bool {
        let room = limits.max_decks.saturating_sub(decks.len());
        decks.extend(rows.iter().take(room).map(mapper));
        rows.len() > room
    }

    fn continue_pages(
        decks: &[RemoteDeck],
        deck_limit: bool,
        page: u32,
        more: bool,
        limits: Limits,
    ) -> Option<Listing> {
        if !more {
            Some(Listing::Done(decks.to_vec()))
        } else if deck_limit || decks.len() >= limits.max_decks {
            Some(truncated(decks.to_vec(), "deck"))
        } else if page >= limits.max_pages {
            Some(truncated(decks.to_vec(), "page"))
        } else {
            None
        }
    }

    async fn fetch_moxfield(&self, username: &str, budget: &Budget, limits: Limits) -> Listing {
        let mut decks = Vec::new();
        let mut page: u32 = 1;
        loop {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("authorUserNames", username)
                .append_pair("includePinned", "true")
                .append_pair("pageNumber", &page.to_string())
                .append_pair("pageSize", "100")
                .append_pair("showIllegal", "true")
                .append_pair("sortDirection", "Descending")
                .append_pair("sortType", "Updated")
                .finish();
            let url = format!("{}/v2/decks/search-sfw?{query}", self.moxfield_base);
            match self.get_limited(&self.client, &url, &[], budget).await {
                Ok(Response {
                    status: 200,
                    body: Some(body),
                }) if body.get("data").is_some_and(Value::is_array) => {
                    let rows = array(body.get("data"));
                    let deck_limit = Self::add_rows(&mut decks, rows, limits, moxfield_deck);
                    let total_pages = body
                        .get("totalPages")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::from(page));
                    let more = u64::from(page) < total_pages && !rows.is_empty();
                    if let Some(listing) =
                        Self::continue_pages(&decks, deck_limit, page, more, limits)
                    {
                        return listing;
                    }
                    page += 1;
                }
                Ok(Response { status: 404, .. }) => {
                    return Listing::Failed("Moxfield user was not found.".to_owned());
                }
                Ok(Response {
                    status: 401 | 403 | 429,
                    ..
                }) => {
                    return Listing::Failed(
                        "Moxfield blocked the request. Try again later or open decks on Moxfield."
                            .to_owned(),
                    );
                }
                Err(error @ (HttpError::ByteLimit | HttpError::DurationLimit)) => {
                    return budget_truncated(decks, &error);
                }
                _ => {
                    return Listing::Failed(
                        "Moxfield could not be reached. Try again shortly.".to_owned(),
                    );
                }
            }
        }
    }

    /// `archidekt_next_url/1`: only Archidekt pages (relative or on archidekt.com) are followed.
    fn archidekt_next_url(&self, next: Option<&Value>) -> Option<String> {
        let next = next?.as_str()?;
        if next.starts_with('/') {
            return Some(format!("{}{next}", self.archidekt_base));
        }
        if next.starts_with(&format!("{}/", self.archidekt_base)) {
            return Some(next.to_owned());
        }
        let host = url::Url::parse(next).ok()?.host_str()?.to_lowercase();
        matches!(host.as_str(), "archidekt.com" | "www.archidekt.com").then(|| next.to_owned())
    }

    async fn fetch_archidekt(&self, username: &str, budget: &Budget, limits: Limits) -> Listing {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("orderBy", "-updatedAt")
            .append_pair("ownerUsername", username)
            .append_pair("page", "1")
            .finish();
        let mut url = format!("{}/api/decks/v3/?{query}", self.archidekt_base);
        let mut rows: Vec<Value> = Vec::new();
        let mut seen = HashSet::new();
        let mut page: u32 = 1;
        let (complete, message) = loop {
            if !seen.insert(url.clone()) {
                return Listing::Failed("Archidekt returned invalid pagination.".to_owned());
            }
            match self.get_limited(&self.client, &url, &[], budget).await {
                Ok(Response {
                    status: 200,
                    body: Some(body),
                }) if body.get("results").is_some_and(Value::is_array) => {
                    let results = array(body.get("results"));
                    let room = limits.max_decks.saturating_sub(rows.len());
                    rows.extend(results.iter().take(room).cloned());
                    let deck_limit = results.len() > room;
                    match self.archidekt_next_url(body.get("next")) {
                        None => break (true, None),
                        Some(_) if deck_limit || rows.len() >= limits.max_decks => {
                            break (
                                false,
                                Some("Remote listing was truncated at the deck limit.".to_owned()),
                            );
                        }
                        Some(_) if page >= limits.max_pages => {
                            break (
                                false,
                                Some("Remote listing was truncated at the page limit.".to_owned()),
                            );
                        }
                        Some(next) => {
                            url = next;
                            page += 1;
                        }
                    }
                }
                Ok(Response { status: 404, .. }) => {
                    return Listing::Failed("Archidekt user was not found.".to_owned());
                }
                Ok(Response {
                    status: 401 | 403 | 429,
                    ..
                }) => {
                    return Listing::Failed(
                        "Archidekt blocked the request. Try again later.".to_owned(),
                    );
                }
                Err(error @ (HttpError::ByteLimit | HttpError::DurationLimit)) => {
                    break (false, Some(budget_message(&error).to_owned()));
                }
                _ => {
                    return Listing::Failed(
                        "Archidekt could not be reached. Try again shortly.".to_owned(),
                    );
                }
            }
        };
        let details: Vec<Result<RemoteDeck, String>> = stream::iter(rows)
            .map(|row| self.fetch_archidekt_detail(row, budget))
            .buffered(5)
            .collect()
            .await;
        let mut decks = Vec::with_capacity(details.len());
        for detail in details {
            match detail {
                Ok(deck) => decks.push(deck),
                Err(message) => return Listing::Failed(message),
            }
        }
        match (complete, message) {
            (false, Some(message)) => Listing::Truncated(decks, message),
            _ => Listing::Done(decks),
        }
    }

    async fn fetch_archidekt_detail(
        &self,
        row: Value,
        budget: &Budget,
    ) -> Result<RemoteDeck, String> {
        let id = id_text(row.get("id"));
        let url = format!("{}/api/decks/{id}/", self.archidekt_base);
        match self.get_limited(&self.client, &url, &[], budget).await {
            Ok(Response {
                status: 200,
                body: Some(body),
            }) if body.is_object() => {
                let commanders: Vec<&Value> = array(body.get("cards"))
                    .iter()
                    .filter(|card| {
                        array(card.get("categories"))
                            .iter()
                            .any(|category| category == "Commander")
                    })
                    .collect();
                let oracle = |card: &Value| {
                    card.get("card")
                        .and_then(|card| card.get("oracleCard"))
                        .cloned()
                };
                let colors: Vec<Value> = commanders
                    .iter()
                    .filter_map(|card| oracle(card))
                    .flat_map(|oracle| array(oracle.get("colorIdentity")).to_vec())
                    .filter_map(|color| {
                        color
                            .as_str()
                            .map(|name| Value::String(archidekt_color(name).to_owned()))
                    })
                    .collect();
                Ok(RemoteDeck {
                    name: string(row.get("name")).or_else(|| string(body.get("name"))),
                    commanders: commanders
                        .iter()
                        .filter_map(|card| {
                            string(oracle(card).as_ref().and_then(|o| o.get("name")))
                        })
                        .collect(),
                    color_identity: order_colors(&colors),
                    url: format!("{PUBLIC_ARCHIDEKT}/decks/{id}"),
                    source: Source::Archidekt,
                    updated_at: string(row.get("updatedAt")),
                })
            }
            Err(error @ (HttpError::ByteLimit | HttpError::DurationLimit)) => {
                Err(budget_message(&error).to_owned())
            }
            _ => Err("Archidekt deck details could not be reached. Try again shortly.".to_owned()),
        }
    }

    async fn fetch_manavault(
        &self,
        origin: &str,
        api_key: &str,
        budget: &Budget,
        limits: Limits,
    ) -> Listing {
        let mut decks = Vec::new();
        let mut page: u32 = 1;
        let headers = [("authorization", format!("Bearer {api_key}"))];
        let origin = origin.trim_end_matches('/');
        loop {
            let path = format!("/api/v1/decks?page={page}&per_page=100");
            match self.get_remote(origin, &path, &headers, budget).await {
                Ok(Response {
                    status: 200,
                    body: Some(body),
                }) if body.get("data").is_some_and(Value::is_array) => {
                    let rows = array(body.get("data"));
                    let deck_limit =
                        Self::add_rows(&mut decks, rows, limits, |row| manavault_deck(row, origin));
                    let total_pages = body
                        .get("pagination")
                        .and_then(|pagination| pagination.get("total_pages"))
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::from(page));
                    let more = u64::from(page) < total_pages && !rows.is_empty();
                    if let Some(listing) =
                        Self::continue_pages(&decks, deck_limit, page, more, limits)
                    {
                        return listing;
                    }
                    page += 1;
                }
                Ok(Response { status: 401, .. }) => {
                    return Listing::Failed(
                        "ManaVault rejected the API key. Create a new one in ManaVault Settings."
                            .to_owned(),
                    );
                }
                Ok(Response { status: 404, .. }) => {
                    return Listing::Failed(
                        "ManaVault has no deck API at this URL. Update ManaVault or check the instance URL.".to_owned(),
                    );
                }
                Ok(Response { status: 429, .. }) => {
                    return Listing::Failed(
                        "ManaVault rate-limited the request. Try again shortly.".to_owned(),
                    );
                }
                Err(HttpError::Blocked) => {
                    return Listing::Failed(
                        "ManaVault resolved to a blocked network address. Ask the operator to allow this host.".to_owned(),
                    );
                }
                Err(error @ (HttpError::ByteLimit | HttpError::DurationLimit)) => {
                    return budget_truncated(decks, &error);
                }
                _ => {
                    return Listing::Failed(
                        "ManaVault could not be reached. Try again shortly.".to_owned(),
                    );
                }
            }
        }
    }
}
