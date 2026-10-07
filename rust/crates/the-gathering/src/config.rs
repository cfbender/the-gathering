//! Runtime configuration read from the environment, mirroring `config/*.exs`.
//!
//! `THE_GATHERING_ENV` picks the defaults (`prod` unless set): `dev` uses the repository's
//! development database and the Vite dev server, `prod` requires `SECRET_KEY_BASE` and keeps
//! data under `DATA_DIR`. Every variable the Elixir `runtime.exs` read keeps its name.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};

/// Which defaults apply (Mix's `config_env()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Env {
    /// Local development: Vite dev server, development database, auto sign-in.
    Dev,
    /// Production container or LXC install.
    Prod,
    /// The test suite.
    Test,
}

/// Where the SPA shell finds its script and style tags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViteMode {
    /// Point at the Vite dev server (HMR) at this origin.
    DevServer {
        /// For example `http://127.0.0.1:5173`.
        origin: String,
    },
    /// Read `priv/static/assets/react/.vite/manifest.json`.
    Manifest,
}

/// A fixed-window limit: `limit` hits per `scale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowLimit {
    /// Hits allowed per window.
    pub limit: u64,
    /// Window length.
    pub scale: Duration,
}

/// A token bucket for channel events.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BucketLimit {
    /// Burst size.
    pub capacity: f64,
    /// Sustained rate.
    pub refill_per_second: f64,
}

/// `config :the_gathering, TheGatheringWeb.RateLimit`.
#[derive(Clone, Debug)]
pub struct RateLimits {
    /// Password login and bootstrap registration, per client address.
    pub credentials: WindowLimit,
    /// Card-recognition corrections, per user.
    pub corrections: WindowLimit,
    /// Personal API key requests, per owner.
    pub api_keys: WindowLimit,
    /// Password sudo, per user and address.
    pub sudo: WindowLimit,
    /// Password sudo across everyone.
    pub sudo_global: u64,
    /// Webcam table config (mints TURN credentials), per user.
    pub turn_credentials: WindowLimit,
    /// Webcam table channel events, per connection.
    pub webcam_table_events: BucketLimit,
    /// Webcam table signaling, per connection.
    pub webcam_table_signals: BucketLimit,
    /// Webcam table joins, per user.
    pub webcam_table_joins: WindowLimit,
    /// Behind a reverse proxy, read the client address from `x-real-ip`/`x-forwarded-for`.
    pub trust_proxy_headers: bool,
}

impl RateLimits {
    fn defaults() -> Self {
        let minutes = |m: u64| Duration::from_secs(m * 60);
        Self {
            credentials: WindowLimit { limit: 10, scale: minutes(5) },
            corrections: WindowLimit { limit: 30, scale: minutes(1) },
            api_keys: WindowLimit { limit: 120, scale: minutes(1) },
            sudo: WindowLimit { limit: 5, scale: minutes(5) },
            sudo_global: 100,
            turn_credentials: WindowLimit { limit: 20, scale: minutes(5) },
            webcam_table_events: BucketLimit { capacity: 60.0, refill_per_second: 20.0 },
            webcam_table_signals: BucketLimit { capacity: 300.0, refill_per_second: 50.0 },
            webcam_table_joins: WindowLimit { limit: 30, scale: minutes(1) },
            trust_proxy_headers: false,
        }
    }

    /// Effectively unlimited buckets, as `config/test.exs` sets them.
    pub fn unlimited() -> Self {
        let big = WindowLimit { limit: 1_000_000, scale: Duration::from_secs(300) };
        Self {
            credentials: big,
            corrections: big,
            api_keys: big,
            sudo: big,
            sudo_global: 1_000_000,
            turn_credentials: big,
            webcam_table_events: BucketLimit { capacity: 1e6, refill_per_second: 1e6 },
            webcam_table_signals: BucketLimit { capacity: 1e6, refill_per_second: 1e6 },
            webcam_table_joins: WindowLimit { limit: 1_000_000, scale: Duration::from_secs(60) },
            trust_proxy_headers: false,
        }
    }
}

/// Discord OAuth sign-in credentials.
#[derive(Clone, Debug)]
pub struct DiscordOAuthConfig {
    /// Application client id.
    pub client_id: String,
    /// Application client secret.
    pub client_secret: String,
    /// Discord's base URL (tests point it at a stub).
    pub api_base: String,
    /// Authorization page base URL.
    pub authorize_url: String,
}

/// Optional Discord bot.
#[derive(Clone, Debug)]
pub struct DiscordBotConfig {
    /// Bot token; the bot is off without one.
    pub token: String,
    /// Restrict commands to one guild.
    pub guild_id: Option<String>,
    /// SpellBot's user id, whose "game ready" posts start tracking.
    pub spellbot_user_id: String,
}

/// Static WebRTC servers handed to browsers.
#[derive(Clone, Debug, Default)]
pub struct WebcamTableConfig {
    /// STUN URLs.
    pub stun_urls: Vec<String>,
    /// TURN URLs.
    pub turn_urls: Vec<String>,
    /// TURN username.
    pub turn_username: Option<String>,
    /// TURN credential.
    pub turn_credential: Option<String>,
}

/// Cloudflare Realtime TURN key.
#[derive(Clone, Debug, Default)]
pub struct CloudflareTurnConfig {
    /// TURN key id.
    pub key_id: Option<String>,
    /// API token for minting credentials.
    pub api_token: Option<String>,
    /// Credential lifetime.
    pub ttl_seconds: u64,
    /// API base (tests point it at a stub).
    pub api_base: String,
}

/// The webcam table SFU's media transport.
#[derive(Clone, Debug)]
pub struct SfuConfig {
    /// First UDP port.
    pub port_min: u16,
    /// Last UDP port.
    pub port_max: u16,
    /// Address announced to browsers.
    pub public_ip: Option<String>,
    /// Also listen on IPv6.
    pub ipv6: bool,
    /// Relay all media through TURN.
    pub relay_only: bool,
}

/// Self-update from the admin UI.
#[derive(Clone, Debug, Default)]
pub struct SelfUpdateConfig {
    /// File whose creation asks systemd to update (LXC installs).
    pub request_file: Option<String>,
    /// Watchtower HTTP API.
    pub watchtower_url: Option<String>,
    /// Watchtower API token.
    pub watchtower_token: Option<String>,
    /// Image Watchtower updates.
    pub watchtower_image: Option<String>,
    /// GitHub API base (tests point it at a stub).
    pub github_api: String,
}

/// Everything the server reads at boot.
#[derive(Clone, Debug)]
pub struct Config {
    /// Which defaults apply.
    pub env: Env,
    /// HTTP listen address.
    pub bind: IpAddr,
    /// HTTP port.
    pub port: u16,
    /// Public URL scheme, host, and port used for generated links.
    pub url_scheme: String,
    /// Public host.
    pub url_host: String,
    /// Public port.
    pub url_port: u16,
    /// SQLite database file.
    pub database_path: PathBuf,
    /// Connection pool size.
    pub pool_size: u32,
    /// Runtime data (card-recognition bundles, image cache).
    pub data_dir: PathBuf,
    /// `priv/` (static files, `VERSION`).
    pub priv_dir: PathBuf,
    /// Signs and encrypts cookies and stored credentials.
    pub secret_key_base: String,
    /// SPA asset mode.
    pub vite: ViteMode,
    /// Sign anonymous requests in as an administrator (development only).
    pub dev_auto_login: bool,
    /// bcrypt cost.
    pub bcrypt_cost: u32,
    /// Rate limits.
    pub rate_limits: RateLimits,
    /// Discord OAuth, when both credentials are set.
    pub discord_oauth: Option<DiscordOAuthConfig>,
    /// Discord bot, when a token is set.
    pub discord_bot: Option<DiscordBotConfig>,
    /// Time zone for Discord start times without one.
    pub discord_default_timezone: String,
    /// Bearer token for the card-recognition corrections export.
    pub cardid_corrections_token: Option<String>,
    /// Administrator the export token acts for.
    pub cardid_corrections_admin_id: Option<i64>,
    /// Scheduled Scryfall sync.
    pub catalog_sync_enabled: bool,
    /// Interval between scheduled syncs.
    pub catalog_sync_interval: Duration,
    /// Close idle webcam tables in the background.
    pub webcam_table_pruning_enabled: bool,
    /// Static WebRTC servers.
    pub webcam_table: WebcamTableConfig,
    /// Cloudflare TURN.
    pub cloudflare_turn: CloudflareTurnConfig,
    /// SFU transport.
    pub sfu: SfuConfig,
    /// Self-hosted ManaVault origin whose share links resolve.
    pub manavault_url: Option<String>,
    /// Private ManaVault hosts members may use.
    pub manavault_allowed_hosts: Vec<String>,
    /// Permit plain-HTTP personal ManaVault origins.
    pub manavault_allow_insecure_urls: bool,
    /// Self-update.
    pub self_update: SelfUpdateConfig,
    /// Scryfall API base (tests point it at a stub).
    pub scryfall_api_base: String,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn flag(name: &str) -> bool {
    matches!(var(name).as_deref(), Some("true" | "1"))
}

fn split_urls(name: &str) -> Vec<String> {
    var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_var<T: std::str::FromStr>(name: &str, default: T) -> anyhow::Result<T> {
    match var(name) {
        None => Ok(default),
        Some(value) => value
            .trim()
            .parse()
            .ok()
            .with_context(|| format!("{name} must be a number, got {value:?}")),
    }
}

const DEV_SECRET: &str = "ZaS8tKZTnpfTRp5R8v1UFpZEDXq7CSdB0Y5T2QHXtV/LpOmiTf/6hyCLSp+HwuHD";

impl Config {
    /// Reads the environment.
    pub fn from_env() -> anyhow::Result<Self> {
        let env = match var("THE_GATHERING_ENV").as_deref() {
            Some("dev") => Env::Dev,
            Some("test") => Env::Test,
            None | Some("prod") => Env::Prod,
            Some(other) => bail!("THE_GATHERING_ENV must be dev, test, or prod; got {other:?}"),
        };
        let repo_root = std::env::current_dir()?;
        let (data_dir, database_path, secret) = match env {
            Env::Prod => {
                let data_dir = PathBuf::from(var("DATA_DIR").unwrap_or_else(|| "/data".into()));
                let database_path = var("DATABASE_PATH")
                    .map_or_else(|| data_dir.join("the_gathering.db"), PathBuf::from);
                let secret = var("SECRET_KEY_BASE").context(
                    "environment variable SECRET_KEY_BASE is missing; generate one with `openssl rand -base64 64`",
                )?;
                (data_dir, database_path, secret)
            }
            Env::Dev | Env::Test => {
                let data_dir = var("DATA_DIR").map_or_else(|| repo_root.join("data"), PathBuf::from);
                let database_path = var("DATABASE_PATH")
                    .map_or_else(|| repo_root.join("the_gathering_dev.db"), PathBuf::from);
                (data_dir, database_path, var("SECRET_KEY_BASE").unwrap_or_else(|| DEV_SECRET.into()))
            }
        };
        if secret.len() < 64 {
            bail!("SECRET_KEY_BASE must be at least 64 bytes");
        }

        let scheme = var("PHX_SCHEME").unwrap_or_else(|| {
            if env == Env::Prod { "https".into() } else { "http".into() }
        });
        let port: u16 = parse_var("PORT", 4000)?;
        let default_url_port = match (env, scheme.as_str()) {
            (Env::Prod, "https") => 443,
            (Env::Prod, _) => 80,
            _ => port,
        };

        let discord_oauth = match (var("DISCORD_CLIENT_ID"), var("DISCORD_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => Some(DiscordOAuthConfig {
                client_id,
                client_secret,
                api_base: "https://discord.com/api".into(),
                authorize_url: "https://discord.com/oauth2/authorize".into(),
            }),
            (None, None) => None,
            _ => {
                tracing::warn!(
                    "Discord OAuth sign-in stays disabled: set both DISCORD_CLIENT_ID and DISCORD_CLIENT_SECRET"
                );
                None
            }
        };

        let stun_urls = match split_urls("WEBRTC_STUN_URLS").as_slice() {
            [] => vec!["stun:stun.l.google.com:19302".into(), "stun:stun.cloudflare.com:3478".into()],
            [none] if none == "none" => Vec::new(),
            urls => urls.to_vec(),
        };

        let port_range = var("WEBRTC_SFU_PORT_RANGE").unwrap_or_else(|| "50000-50100".into());
        let (port_min, port_max) = match port_range.split_once('-') {
            Some((first, last)) => (first.trim().parse()?, last.trim().parse()?),
            None => {
                let single = port_range.trim().parse()?;
                (single, single)
            }
        };

        let corrections_admin_id = var("CARDID_CORRECTIONS_ADMIN_ID")
            .and_then(|id| id.parse::<i64>().ok())
            .filter(|id| *id > 0);

        let mut rate_limits = if env == Env::Test { RateLimits::unlimited() } else { RateLimits::defaults() };
        rate_limits.trust_proxy_headers = flag("TRUST_PROXY_HEADERS");

        let catalog_sync_hours: u64 = parse_var("CATALOG_SYNC_INTERVAL_HOURS", 168)?;

        Ok(Self {
            env,
            bind: if env == Env::Prod {
                IpAddr::V6(Ipv6Addr::UNSPECIFIED)
            } else {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            },
            port,
            url_host: var("PHX_HOST").unwrap_or_else(|| "localhost".into()),
            url_port: parse_var("PHX_URL_PORT", default_url_port)?,
            url_scheme: scheme,
            database_path,
            pool_size: parse_var("POOL_SIZE", 5)?,
            priv_dir: var("PRIV_DIR").map_or_else(|| repo_root.join("priv"), PathBuf::from),
            data_dir,
            secret_key_base: secret,
            vite: if env == Env::Dev {
                ViteMode::DevServer {
                    origin: format!("http://127.0.0.1:{}", var("VITE_PORT").unwrap_or_else(|| "5173".into())),
                }
            } else {
                ViteMode::Manifest
            },
            dev_auto_login: env == Env::Dev && var("DEV_AUTO_LOGIN").as_deref() != Some("false"),
            bcrypt_cost: if env == Env::Test { 4 } else { 12 },
            rate_limits,
            discord_oauth,
            discord_bot: var("DISCORD_BOT_TOKEN").map(|token| DiscordBotConfig {
                token,
                guild_id: var("DISCORD_GUILD_ID"),
                spellbot_user_id: var("DISCORD_SPELLBOT_USER_ID")
                    .unwrap_or_else(|| "725510263251402832".into()),
            }),
            discord_default_timezone: var("DISCORD_DEFAULT_TIMEZONE")
                .unwrap_or_else(|| "America/New_York".into()),
            cardid_corrections_token: var("CARDID_CORRECTIONS_TOKEN"),
            cardid_corrections_admin_id: corrections_admin_id,
            catalog_sync_enabled: env != Env::Test && var("CATALOG_SYNC_ENABLED").as_deref() != Some("false"),
            catalog_sync_interval: Duration::from_secs(catalog_sync_hours.saturating_mul(3600)),
            webcam_table_pruning_enabled: env != Env::Test,
            webcam_table: WebcamTableConfig {
                stun_urls,
                turn_urls: split_urls("WEBRTC_TURN_URLS"),
                turn_username: var("WEBRTC_TURN_USERNAME"),
                turn_credential: var("WEBRTC_TURN_CREDENTIAL"),
            },
            cloudflare_turn: CloudflareTurnConfig {
                key_id: var("CLOUDFLARE_TURN_KEY_ID"),
                api_token: var("CLOUDFLARE_TURN_API_TOKEN"),
                ttl_seconds: parse_var("CLOUDFLARE_TURN_TTL_SECONDS", 21_600)?,
                api_base: "https://rtc.live.cloudflare.com".into(),
            },
            sfu: SfuConfig {
                port_min,
                port_max,
                public_ip: var("WEBRTC_SFU_PUBLIC_IP"),
                ipv6: flag("WEBRTC_SFU_IPV6"),
                relay_only: flag("WEBRTC_SFU_RELAY_ONLY"),
            },
            manavault_url: var("MANAVAULT_URL"),
            manavault_allowed_hosts: var("MANAVAULT_ALLOWED_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(|host| host.trim().to_lowercase())
                .filter(|host| !host.is_empty())
                .collect(),
            manavault_allow_insecure_urls: flag("MANAVAULT_ALLOW_INSECURE_URLS"),
            self_update: SelfUpdateConfig {
                request_file: var("SELF_UPDATE_REQUEST_FILE"),
                watchtower_url: var("WATCHTOWER_URL"),
                watchtower_token: var("WATCHTOWER_HTTP_API_TOKEN"),
                watchtower_image: var("WATCHTOWER_IMAGE"),
                github_api: "https://api.github.com/repos/cfbender/the-gathering".into(),
            },
            scryfall_api_base: "https://api.scryfall.com".into(),
        })
    }

    /// A configuration for tests: a scratch database and data directory, no background jobs.
    pub fn for_test(database_path: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            env: Env::Test,
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 4002,
            url_scheme: "http".into(),
            url_host: "localhost".into(),
            url_port: 4002,
            database_path,
            pool_size: 1,
            data_dir,
            priv_dir: PathBuf::from("priv"),
            secret_key_base: "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl".into(),
            vite: ViteMode::DevServer { origin: "http://127.0.0.1:5173".into() },
            dev_auto_login: false,
            bcrypt_cost: 4,
            rate_limits: RateLimits::unlimited(),
            discord_oauth: Some(DiscordOAuthConfig {
                client_id: "discord-client-id".into(),
                client_secret: "discord-client-secret".into(),
                api_base: "https://discord.com/api".into(),
                authorize_url: "https://discord.com/oauth2/authorize".into(),
            }),
            discord_bot: None,
            discord_default_timezone: "America/New_York".into(),
            cardid_corrections_token: None,
            cardid_corrections_admin_id: None,
            catalog_sync_enabled: false,
            catalog_sync_interval: Duration::from_secs(168 * 3600),
            webcam_table_pruning_enabled: false,
            webcam_table: WebcamTableConfig::default(),
            cloudflare_turn: CloudflareTurnConfig {
                ttl_seconds: 21_600,
                api_base: "https://rtc.live.cloudflare.com".into(),
                ..CloudflareTurnConfig::default()
            },
            sfu: SfuConfig { port_min: 50_000, port_max: 50_100, public_ip: None, ipv6: false, relay_only: false },
            manavault_url: Some("https://manavault.example.com".into()),
            manavault_allowed_hosts: Vec::new(),
            manavault_allow_insecure_urls: false,
            self_update: SelfUpdateConfig {
                github_api: "https://api.github.com/repos/cfbender/the-gathering".into(),
                ..SelfUpdateConfig::default()
            },
            scryfall_api_base: "https://api.scryfall.com".into(),
        }
    }

    /// The public base URL, such as `https://games.example.com` (Phoenix's `Endpoint.url/0`).
    pub fn public_url(&self) -> String {
        let default_port = matches!(
            (self.url_scheme.as_str(), self.url_port),
            ("https", 443) | ("http", 80)
        );
        if default_port {
            format!("{}://{}", self.url_scheme, self.url_host)
        } else {
            format!("{}://{}:{}", self.url_scheme, self.url_host, self.url_port)
        }
    }

    /// `priv/static`.
    pub fn static_dir(&self) -> PathBuf {
        self.priv_dir.join("static")
    }
}
