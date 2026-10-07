//! Shared server state handed to every request handler and background job.

use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::accounts::Accounts;
use crate::config::Config;
use crate::db::Pool;
use crate::rate_limit::RateLimiter;

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
            // Relay-only mode needs Cloudflare TURN credentials (wired with CloudflareTurn).
            relay: None,
        });
        Ok(Self(Arc::new(Inner {
            config,
            pool,
            accounts,
            rate_limiter: RateLimiter::new(),
            http,
            sfu,
            session_disconnects,
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

/// The User-Agent sent to third parties.
pub fn user_agent() -> String {
    format!("the-gathering/{} (+https://github.com/cfbender/the-gathering)", env!("CARGO_PKG_VERSION"))
}
