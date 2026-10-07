//! Shared server state handed to every request handler and background job.

use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::accounts::Accounts;
use crate::config::Config;
use crate::db::Pool;
use crate::rate_limit::RateLimiter;
use crate::web::channels::presence::Presence;
use crate::web::channels::pubsub::PubSub;
use crate::webcam::WebcamTables;

/// Cheap to clone; everything lives behind one `Arc`.
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

/// The state's contents.
pub struct Inner {
    /// Runtime configuration.
    pub config: Config,
    /// Database pool.
    pub pool: Pool,
    /// Accounts and sessions.
    pub accounts: Accounts,
    /// Fixed-window rate limiter (Hammer).
    pub rate_limiter: RateLimiter,
    /// Outbound HTTP client for trusted APIs (Discord, GitHub, Cloudflare).
    pub http: reqwest::Client,
    /// The webcam table SFU.
    pub sfu: the_gathering_sfu::Sfu,
    /// Session topics (`users_sessions:<token>`) whose sockets must disconnect.
    pub session_disconnects: broadcast::Sender<String>,
    /// Channel topic subscriptions (`TheGathering.PubSub`).
    pub pubsub: PubSub,
    /// Channel presence (`TheGatheringWeb.Presence`).
    pub presence: Presence,
    /// Running webcam table rooms.
    pub webcam_tables: WebcamTables,
}

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    /// Builds the state around an open, migrated pool.
    pub fn new(config: Config, pool: Pool) -> anyhow::Result<Self> {
        let accounts = Accounts {
            pool: pool.clone(),
            secret_key_base: config.secret_key_base.clone(),
            bcrypt_cost: config.bcrypt_cost,
        };
        let http = reqwest::Client::builder()
            .user_agent(user_agent())
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        let (session_disconnects, _) = broadcast::channel(64);
        let sfu = the_gathering_sfu::Sfu::new(the_gathering_sfu::Settings {
            port_min: config.sfu.port_min,
            port_max: config.sfu.port_max,
            public_ip: config.sfu.public_ip.as_deref().and_then(|ip| ip.parse().ok()),
            ipv6: config.sfu.ipv6,
            relay: relay_servers(&config, &http),
        });
        let pubsub = PubSub::new();
        let presence = Presence::new(pubsub.clone());
        let webcam_tables = WebcamTables::new(pool.clone(), pubsub.clone());
        Ok(Self(Arc::new(Inner {
            config,
            pool,
            accounts,
            rate_limiter: RateLimiter::new(),
            http,
            sfu,
            session_disconnects,
            pubsub,
            presence,
            webcam_tables,
        })))
    }
}

impl AppState {
    /// Disconnects realtime sockets opened with this session token (logging out or a
    /// password change broadcasts `disconnect` to the session topic).
    pub fn disconnect_session(&self, token: &[u8]) {
        let _ = self.session_disconnects.send(crate::web::auth::user_session_topic(token));
    }
}

/// Relay-only SFU mode (`Sfu.relay_servers/0`): every connection fetches fresh Cloudflare
/// TURN credentials. Without Cloudflare TURN configured the SFU listens directly.
fn relay_servers(config: &Config, http: &reqwest::Client) -> Option<the_gathering_sfu::RelayServers> {
    if !config.sfu.relay_only || !crate::cloudflare_turn::configured(&config.cloudflare_turn) {
        return None;
    }
    let http = http.clone();
    let turn = config.cloudflare_turn.clone();
    Some(Arc::new(move || {
        let http = http.clone();
        let turn = turn.clone();
        Box::pin(async move { crate::cloudflare_turn::relay_servers(&http, &turn).await })
    }))
}

/// The User-Agent sent to third parties.
pub fn user_agent() -> String {
    format!("the-gathering/{} (+https://github.com/cfbender/the-gathering)", env!("CARGO_PKG_VERSION"))
}
