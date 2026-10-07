//! Deck lists from Moxfield, Archidekt, and ManaVault (`TheGathering.Decklists`), built on
//! lotus's link parsing and fetchers.
//!
//! Only the configured ManaVault origin (`MANAVAULT_URL`) resolves share links, which keeps
//! the server from fetching arbitrary hosts; members' own instances are only contacted to
//! list their decks ([`remote_decks`]), through the destination policy.

pub mod cache;
pub mod destination;
pub mod remote_decks;

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use lotus::Zone;
use lotus::decklist::{
    Allowlist, DeckLink, DecklistClient, FetchError, Limits, Origin, Resolver, ShareKind,
    ShareLink, Source, SystemResolver,
};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use self::cache::TtlCache;
use self::remote_decks::RemoteDecks;
use crate::config::Config;

/// ManaVault share tokens are at least this long.
const MIN_SHARE_TOKEN_BYTES: usize = 20;

/// Which service a link belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSource {
    /// A supported source.
    Supported(Source),
    /// A valid URL on a site the app does not resolve.
    Other,
}

/// A recognized link (`parse_url/1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedUrl {
    /// The service.
    pub source: LinkSource,
    /// The service's deck id (the URL itself for other sites).
    pub id: String,
    /// The canonical public URL.
    pub canonical_url: String,
}

/// Why a deck list could not be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecklistError {
    /// Not an `http(s)` URL with a plausible host.
    #[error("invalid deck-list URL")]
    InvalidUrl,
    /// A URL on a site the app does not resolve.
    #[error("unsupported deck-list URL")]
    UnsupportedUrl,
    /// The source has no such deck.
    #[error("deck not found")]
    NotFound,
    /// The deck is private.
    #[error("deck is private")]
    Private,
    /// The source failed or answered unexpectedly.
    #[error("deck-list source failed")]
    UpstreamError,
    /// The ManaVault server predates the share query's fields (lotus
    /// `FetchError::ServerTooOld`); it answers, but cannot share this deck until upgraded.
    #[error("{SERVER_TOO_OLD}")]
    ServerTooOld,
}

/// What members see when a linked ManaVault is older than the share query needs
/// (`lotus::decklist::manavault::MIN_SERVER_VERSION`; a test keeps the two in step).
pub const SERVER_TOO_OLD: &str =
    "This ManaVault server is too old to share deck lists; it needs v1.3.0 or newer.";

/// One entry of the playable list (commander zone and main deck; maybe-, side-, and
/// considering boards are left out).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckCard {
    /// Card name as the source spells it.
    pub name: String,
    /// Copies (at least one).
    pub quantity: u32,
    /// [`Zone::Commander`] or [`Zone::Mainboard`].
    pub zone: Zone,
    /// The exact Scryfall printing the list names, when it records one.
    pub printing_id: Option<String>,
}

/// Public metadata and the playable list of a resolved deck
/// (`TheGathering.Decklists.Decklist`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decklist {
    /// The service.
    pub source: Source,
    /// The service's id for the deck.
    pub id: String,
    /// Canonical public URL.
    pub url: String,
    /// Deck name.
    pub name: Option<String>,
    /// Commander names, in source order.
    pub commanders: Vec<String>,
    /// The commanders' identity in WUBRG order; `None` when the source exposes none.
    pub color_identity: Option<Vec<String>>,
    /// Author.
    pub author: Option<String>,
    /// The source's own card count.
    pub card_count: Option<u64>,
    /// The playable list.
    pub cards: Vec<DeckCard>,
    /// When it was fetched.
    pub fetched_at: OffsetDateTime,
}

impl Decklist {
    fn from_lotus(list: lotus::decklist::Decklist) -> Self {
        let cards = list
            .playable()
            .map(|entry| DeckCard {
                name: entry.name.clone(),
                quantity: entry.quantity.get(),
                zone: entry.zone,
                printing_id: entry.scryfall_id.as_ref().map(|id| id.as_str().to_owned()),
            })
            .collect();
        let colors: Vec<String> = list
            .color_identity
            .iter()
            .map(|color| color.code().to_owned())
            .collect();
        Self {
            source: list.source,
            id: list.id,
            url: list.url,
            name: list.name,
            commanders: list.commanders,
            color_identity: if colors.is_empty() {
                None
            } else {
                Some(colors)
            },
            author: list.author,
            card_count: list.card_count,
            cards,
            fetched_at: OffsetDateTime::now_utc(),
        }
    }

    /// `fetched_at` as ISO 8601 with microseconds (`DateTime.to_iso8601/1`).
    pub fn fetched_at_iso(&self) -> String {
        let micros = self
            .fetched_at
            .replace_nanosecond(self.fetched_at.microsecond() * 1_000)
            .unwrap_or(self.fetched_at);
        micros.format(&Rfc3339).unwrap_or_default()
    }

    /// `DecklistJSON.show/1`: the metadata, without `nil` fields.
    pub fn to_json(&self) -> Value {
        let mut data = Map::new();
        data.insert("source".into(), json!(self.source.as_str()));
        data.insert("id".into(), json!(self.id));
        data.insert("url".into(), json!(self.url));
        if let Some(name) = &self.name {
            data.insert("name".into(), json!(name));
        }
        let commanders: Vec<Value> = self
            .commanders
            .iter()
            .map(|name| json!({ "name": name }))
            .collect();
        data.insert("commanders".into(), Value::Array(commanders));
        if let Some(colors) = &self.color_identity {
            data.insert("color_identity".into(), json!(colors));
        }
        if let Some(author) = &self.author {
            data.insert("author".into(), json!(author));
        }
        if let Some(count) = self.card_count {
            data.insert("card_count".into(), json!(count));
        }
        data.insert("fetched_at".into(), json!(self.fetched_at_iso()));
        Value::Object(data)
    }
}

/// A [`Resolver`] that can be replaced at runtime (tests stub DNS through it).
#[derive(Clone)]
pub struct SharedResolver(Arc<RwLock<Arc<dyn Resolver>>>);

impl std::fmt::Debug for SharedResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedResolver")
    }
}

impl SharedResolver {
    fn current(&self) -> Arc<dyn Resolver> {
        match self.0.read() {
            Ok(inner) => inner.clone(),
            Err(_) => Arc::new(SystemResolver),
        }
    }

    fn set(&self, resolver: Arc<dyn Resolver>) {
        if let Ok(mut inner) = self.0.write() {
            *inner = resolver;
        }
    }
}

impl Resolver for SharedResolver {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send + 'a>> {
        let inner = self.current();
        Box::pin(async move { inner.resolve(host).await })
    }
}

/// Deck-list resolution: the lotus client, the configured ManaVault origin, the shared
/// result cache, and members' remote deck listings.
#[derive(Debug)]
pub struct Decklists {
    client: DecklistClient,
    cache: TtlCache<String, Decklist>,
    manavault: Option<Origin>,
    resolver: SharedResolver,
    /// Members' Moxfield, Archidekt, and ManaVault deck listings.
    pub remote: RemoteDecks,
}

fn valid_host(host: &str) -> bool {
    host.contains('.') || matches!(host, "localhost" | "[::1]" | "::1")
}

impl Decklists {
    /// Builds the clients from the configuration.
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let resolver = SharedResolver(Arc::new(RwLock::new(Arc::new(SystemResolver))));
        let manavault = config.manavault_url.as_deref().and_then(Origin::parse);
        // The configured instance is operator-trusted (the Elixir server never checked it),
        // so its host passes the destination policy even on a private network.
        let mut allowed: Vec<&str> = config
            .manavault_allowed_hosts
            .iter()
            .map(String::as_str)
            .collect();
        if let Some(origin) = &manavault {
            allowed.push(&origin.host);
        }
        let client = DecklistClient::builder(&format!(
            "TheGathering/{} deck metadata resolver (+https://github.com/cfbender/the-gathering)",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(8))
        // lotus's default budget (10 pages, as ManaVault's own importer uses). A deck that
        // needs more is an error rather than a silently shortened list: the Elixir adapter
        // stopped after four pages and returned what it had, a bug its own comment warned
        // against ("not silently cut short").
        .limits(Limits::default())
        .allowlist(Allowlist::parse(allowed))
        .resolver(Arc::new(resolver.clone()))
        .moxfield_api_base(format!(
            "{}/v3/decks/all/",
            config.moxfield_api_base.trim_end_matches('/')
        ))
        .archidekt_api_base(format!(
            "{}/api/decks/",
            config.archidekt_api_base.trim_end_matches('/')
        ))
        .build()
        .map_err(|error| anyhow::anyhow!("building the deck-list client: {error}"))?;
        Ok(Self {
            client,
            cache: TtlCache::new(cache::TTL, cache::MAX_ENTRIES),
            manavault,
            remote: RemoteDecks::new(config, resolver.clone())?,
            resolver,
        })
    }

    /// Replaces the DNS resolver used for ManaVault hosts.
    pub fn set_resolver(&self, resolver: Arc<dyn Resolver>) {
        self.resolver.set(resolver);
    }

    /// The configured ManaVault origin (`manavault_url/0`).
    pub fn manavault_url(&self) -> Option<&Origin> {
        self.manavault.as_ref()
    }

    /// The resolution cache (keyed by canonical URL).
    pub fn cache(&self) -> &TtlCache<String, Decklist> {
        &self.cache
    }

    /// `parse_url/1`: recognizes Moxfield and Archidekt deck links and share links on the
    /// configured ManaVault host (or its `www.` alias); any other `http(s)` URL with a
    /// plausible host is [`LinkSource::Other`].
    ///
    /// Moxfield and Archidekt ids follow lotus's stricter patterns (Moxfield ids need at
    /// least five characters), so shorter ids are other links.
    pub fn parse_url(&self, url: &str) -> Result<ParsedUrl, DecklistError> {
        let parsed = url::Url::parse(url.trim()).map_err(|_| DecklistError::InvalidUrl)?;
        let host = parsed.host_str().map(str::to_lowercase).unwrap_or_default();
        if !matches!(parsed.scheme(), "http" | "https") || host.is_empty() || !valid_host(&host) {
            return Err(DecklistError::InvalidUrl);
        }
        let link = DeckLink::parse(url).map_err(|_| DecklistError::InvalidUrl)?;
        let supported = |source: Source, id: &str, canonical_url: String| ParsedUrl {
            source: LinkSource::Supported(source),
            id: id.to_owned(),
            canonical_url,
        };
        match &link {
            DeckLink::Moxfield { id } => {
                return Ok(supported(Source::Moxfield, id, link.canonical_url()));
            }
            DeckLink::Archidekt { id } => {
                return Ok(supported(Source::Archidekt, id, link.canonical_url()));
            }
            DeckLink::ManaVault {
                origin: Some(origin),
                share,
            } => {
                if let Some(manavault) = &self.manavault
                    && share.kind == ShareKind::Deck
                    && share.token.len() >= MIN_SHARE_TOKEN_BYTES
                    && (origin.host == manavault.host
                        || origin.host == format!("www.{}", manavault.host))
                {
                    let canonical = manavault.join(&share.path());
                    return Ok(supported(Source::ManaVault, &share.token, canonical));
                }
            }
            DeckLink::ManaVault { origin: None, .. } | DeckLink::Other { .. } => {}
        }
        let mut other = parsed;
        other.set_fragment(None);
        let canonical_url = other.to_string();
        Ok(ParsedUrl {
            source: LinkSource::Other,
            id: canonical_url.clone(),
            canonical_url,
        })
    }

    /// `resolve/1`: the deck behind a supported link, cached for five minutes on success.
    pub async fn resolve(&self, url: &str) -> Result<Decklist, DecklistError> {
        let parsed = self.parse_url(url)?;
        let LinkSource::Supported(source) = parsed.source else {
            return Err(DecklistError::UnsupportedUrl);
        };
        if let Some(decklist) = self.cache.fetch(&parsed.canonical_url) {
            return Ok(decklist);
        }
        let fetched = match source {
            Source::Moxfield => self.client.fetch_moxfield(&parsed.id).await,
            Source::Archidekt => self.client.fetch_archidekt(&parsed.id).await,
            Source::ManaVault => {
                let origin = self
                    .manavault
                    .as_ref()
                    .ok_or(DecklistError::UnsupportedUrl)?;
                let share = ShareLink {
                    kind: ShareKind::Deck,
                    token: parsed.id.clone(),
                };
                self.client.fetch_manavault(origin, &share).await
            }
        };
        let decklist = match fetched {
            Ok(list) => Decklist::from_lotus(list),
            Err(FetchError::NotFound) => return Err(DecklistError::NotFound),
            Err(FetchError::Forbidden) => return Err(DecklistError::Private),
            Err(FetchError::ServerTooOld) => return Err(DecklistError::ServerTooOld),
            Err(error) => {
                tracing::warn!("resolving {}: {error}", parsed.canonical_url);
                return Err(DecklistError::UpstreamError);
            }
        };
        let decklist = Decklist {
            url: parsed.canonical_url.clone(),
            ..decklist
        };
        self.cache.put(parsed.canonical_url, decklist.clone());
        Ok(decklist)
    }
}

/// Sweeps expired cache entries every minute (the Elixir cache's sweep timer).
pub fn start_cache_sweeper(state: &crate::state::AppState) {
    let state = state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_mins(1));
        loop {
            ticker.tick().await;
            state.decklists.cache.sweep();
            state.decklists.remote.sweep();
        }
    });
}
