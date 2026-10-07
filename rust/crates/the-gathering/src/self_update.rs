//! Lets an administrator update the server from the admin UI (`TheGathering.SelfUpdate`).
//!
//! The app never replaces itself; it hands the job to whatever runs it, and that updater
//! decides what "newest" means for the installed channel:
//!
//! - systemd (the Proxmox LXC): `SELF_UPDATE_REQUEST_FILE` names a file that a
//!   `the-gathering-update.path` unit watches. Creating it makes systemd run `update`, which
//!   installs the newest build of the installed channel and restarts the service.
//! - Watchtower (Docker): `WATCHTOWER_HTTP_API_TOKEN` (and optionally `WATCHTOWER_URL`,
//!   `WATCHTOWER_IMAGE`) point at a Watchtower container with its update endpoint enabled.
//!
//! The running version is `priv/VERSION`: `vX.Y.Z` for tagged releases, `nightly-<commit>`
//! for builds of `main`, and `preview-<commit>` for pre-release builds of a branch published by
//! a manual run of the Release workflow. GitHub says what the newest one is; that answer is cached for 15
//! minutes because the API allows 60 anonymous requests an hour.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use tokio::sync::Mutex;

use crate::config::{Config, SelfUpdateConfig};

const NIGHTLY_URL: &str = "https://github.com/cfbender/the-gathering/releases/tag/nightly";
const PREVIEW_URL: &str = "https://github.com/cfbender/the-gathering/releases/tag/preview";
const DEFAULT_IMAGE: &str = "ghcr.io/cfbender/the-gathering";
const DEFAULT_WATCHTOWER_URL: &str = "http://watchtower:8080";
const CHECK_TTL: Duration = Duration::from_mins(15);
/// A request older than this without a restart has most likely failed; let the admin retry.
const PENDING: Duration = Duration::from_mins(15);
/// `DateTime.to_iso8601/1` of a microsecond `DateTime.utc_now()`.
const ISO_MICROS: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z");

/// Which stream of builds the running version came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Tagged `vX.Y.Z` releases.
    Release,
    /// `nightly-<commit>` builds of `main`.
    Nightly,
    /// `preview-<commit>` pre-release builds of a branch (the rolling `preview` tag).
    Preview,
}

impl Channel {
    /// The rolling tag and release page of a commit-addressed channel; `None` for releases.
    fn rolling(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Release => None,
            Self::Nightly => Some(("nightly", NIGHTLY_URL)),
            Self::Preview => Some(("preview", PREVIEW_URL)),
        }
    }
}

/// How this server can be updated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// A request file watched by a systemd path unit.
    Systemd,
    /// Watchtower's HTTP API.
    Watchtower,
}

/// The newest build on GitHub.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Latest {
    /// `v1.2.3` or `nightly-0123456`.
    pub version: String,
    /// Release page.
    pub url: String,
}

/// What the admin page shows (`AdminSoftwareUpdateJSON.show/1`).
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    /// The running version, or `None` for a development build.
    pub version: Option<String>,
    /// The running version's channel.
    pub channel: Option<Channel>,
    /// The configured updater.
    pub method: Option<Method>,
    /// Whether an update request is still being handled.
    pub pending: bool,
    /// When the last update was requested (ISO 8601, microseconds).
    pub requested_at: Option<String>,
    /// The newest build of the channel.
    pub latest: Option<Latest>,
    /// Whether `latest` is newer than the running version (`None` without `latest`).
    pub update_available: Option<bool>,
    /// Why GitHub could not be asked.
    pub check_error: Option<String>,
}

/// Why an update request failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    /// No updater is configured.
    Unsupported,
    /// Watchtower is already updating.
    UpdateInProgress,
    /// The request file cannot be written or Watchtower failed.
    UpdaterUnavailable,
}

#[derive(Debug, Default)]
struct State {
    latest: Option<(Channel, Result<Latest, String>, Instant)>,
    requested_at: Option<OffsetDateTime>,
}

/// The self-update service. Calls are serialized like the Elixir `GenServer`'s.
#[derive(Debug)]
pub struct SelfUpdate {
    config: SelfUpdateConfig,
    version_file: PathBuf,
    http: reqwest::Client,
    state: Mutex<State>,
}

fn present(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|value| !value.is_empty())
}

impl SelfUpdate {
    /// Builds the service from the configuration (`priv/VERSION` under `priv_dir`).
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(crate::state::user_agent())
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .build()?;
        Ok(Self {
            config: config.self_update.clone(),
            version_file: config.priv_dir.join("VERSION"),
            http,
            state: Mutex::new(State::default()),
        })
    }

    /// The running version from `priv/VERSION`, or `None` for a development build.
    pub fn version(&self) -> Option<String> {
        let contents = std::fs::read_to_string(&self.version_file).ok()?;
        let trimmed = contents.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }

    /// How this server can be updated, or `None` when no updater is configured.
    pub fn method(&self) -> Option<Method> {
        if present(self.config.request_file.as_ref()).is_some() {
            Some(Method::Systemd)
        } else if present(self.config.watchtower_token.as_ref()).is_some() {
            Some(Method::Watchtower)
        } else {
            None
        }
    }

    /// Version, channel, updater, whether an update is pending, and what GitHub has newest.
    pub async fn status(&self) -> Status {
        let mut state = self.state.lock().await;
        self.refresh_latest(&mut state).await;
        self.build_status(&state)
    }

    /// Asks the configured updater to install the newest build of the installed channel.
    pub async fn request_update(&self) -> Result<Status, RequestError> {
        let mut state = self.state.lock().await;
        self.request(self.method()).await?;
        state.requested_at = Some(OffsetDateTime::now_utc());
        Ok(self.build_status(&state))
    }

    /// Forgets the cached check and the last request.
    pub async fn reset(&self) {
        *self.state.lock().await = State::default();
    }

    fn build_status(&self, state: &State) -> Status {
        let version = self.version();
        let channel = channel(version.as_deref());
        let (latest, check_error) = match (&state.latest, channel) {
            (Some((checked, result, _)), Some(channel)) if *checked == channel => match result {
                Ok(latest) => (Some(latest.clone()), None),
                Err(message) => (None, Some(message.clone())),
            },
            _ => (None, None),
        };
        let update_available = latest.as_ref().map(|latest| match (channel, &version) {
            (Some(channel), Some(version)) => update_available(channel, version, &latest.version),
            _ => version.as_deref() != Some(latest.version.as_str()),
        });
        Status {
            version,
            channel,
            method: self.method(),
            pending: self.pending(state),
            requested_at: state.requested_at.and_then(|at| at.format(ISO_MICROS).ok()),
            latest,
            update_available,
            check_error,
        }
    }

    /// systemd removes the request file once `update` has finished, so the file alone says
    /// whether the update is still running. Watchtower gives no such signal; a recent request
    /// counts as pending until the container has been replaced.
    fn pending(&self, state: &State) -> bool {
        match (self.method(), present(self.config.request_file.as_ref())) {
            (Some(Method::Systemd), Some(path)) => std::path::Path::new(path).exists(),
            _ => state.requested_at.is_some_and(|at| {
                let elapsed = OffsetDateTime::now_utc() - at;
                elapsed < time::Duration::try_from(PENDING).unwrap_or(time::Duration::MAX)
            }),
        }
    }

    async fn refresh_latest(&self, state: &mut State) {
        let Some(channel) = channel(self.version().as_deref()) else {
            state.latest = None;
            return;
        };
        if let Some((checked, _, at)) = &state.latest
            && *checked == channel
            && at.elapsed() < CHECK_TTL
        {
            return;
        }
        let result = self.fetch_latest(channel).await;
        state.latest = Some((channel, result, Instant::now()));
    }

    async fn fetch_latest(&self, channel: Channel) -> Result<Latest, String> {
        match channel {
            Channel::Release => {
                let body = self.github_get("/releases/latest").await?;
                match (
                    body.get("tag_name").and_then(Value::as_str),
                    body.get("html_url").and_then(Value::as_str),
                ) {
                    (Some(tag), Some(url)) if tag.starts_with('v') => Ok(Latest {
                        version: tag.to_owned(),
                        url: url.to_owned(),
                    }),
                    _ => Err(unexpected_response(&body)),
                }
            }
            Channel::Nightly | Channel::Preview => {
                let (tag, url) = channel.rolling().unwrap_or(("nightly", NIGHTLY_URL));
                let body = self.github_get(&format!("/git/ref/tags/{tag}")).await?;
                match body.pointer("/object/sha").and_then(Value::as_str) {
                    Some(sha) => Ok(Latest {
                        version: format!("{tag}-{}", sha.chars().take(7).collect::<String>()),
                        url: url.to_owned(),
                    }),
                    None => Err(unexpected_response(&body)),
                }
            }
        }
    }

    async fn github_get(&self, path: &str) -> Result<Value, String> {
        let response = self
            .http
            .get(format!("{}{path}", self.config.github_api))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .timeout(Duration::from_secs(8))
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("Update check against GitHub failed: {error}");
                return Err("Could not reach GitHub.".to_owned());
            }
        };
        let status = response.status();
        if status == StatusCode::OK {
            return match response.json::<Value>().await {
                Ok(body) if body.is_object() => Ok(body),
                Ok(body) => Err(unexpected_response(&body)),
                Err(error) => {
                    tracing::warn!("Update check got an unreadable GitHub response: {error}");
                    Err("GitHub returned an unexpected response.".to_owned())
                }
            };
        }
        let rate_limited = response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value.as_bytes() == b"0");
        if status == StatusCode::FORBIDDEN && rate_limited {
            Err("GitHub's API rate limit was reached; try again later.".to_owned())
        } else {
            Err(format!("GitHub answered with status {}.", status.as_u16()))
        }
    }

    async fn request(&self, method: Option<Method>) -> Result<(), RequestError> {
        match method {
            None => Err(RequestError::Unsupported),
            Some(Method::Systemd) => {
                let path = self.config.request_file.clone().unwrap_or_default();
                let contents = format!(
                    "{}\n",
                    OffsetDateTime::now_utc()
                        .format(ISO_MICROS)
                        .unwrap_or_default()
                );
                match tokio::fs::write(&path, contents).await {
                    Ok(()) => {
                        tracing::info!(
                            "Update requested; wrote {path} for the-gathering-update.path"
                        );
                        Ok(())
                    }
                    Err(error) => {
                        tracing::error!("Could not write the update request file {path}: {error}");
                        Err(RequestError::UpdaterUnavailable)
                    }
                }
            }
            Some(Method::Watchtower) => self.request_watchtower().await,
        }
    }

    async fn request_watchtower(&self) -> Result<(), RequestError> {
        let url = present(self.config.watchtower_url.as_ref()).unwrap_or(DEFAULT_WATCHTOWER_URL);
        let url = url.trim_end_matches('/');
        let image = present(self.config.watchtower_image.as_ref()).unwrap_or(DEFAULT_IMAGE);
        let token = self.config.watchtower_token.as_deref().unwrap_or_default();
        let response = self
            .http
            .post(format!("{url}/v1/update"))
            .query(&[("image", image), ("async", "true")])
            .bearer_auth(token)
            // Older Watchtower builds ignore `async` and only answer once the update has
            // finished, which is after this server has been replaced; a long wait is harmless.
            .timeout(Duration::from_secs(30))
            .send()
            .await;
        match response {
            Ok(response) if matches!(response.status().as_u16(), 200 | 202) => {
                tracing::info!("Update requested from Watchtower at {url}");
                Ok(())
            }
            Ok(response) if response.status() == StatusCode::TOO_MANY_REQUESTS => {
                Err(RequestError::UpdateInProgress)
            }
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                let body: String = body.chars().take(500).collect();
                tracing::error!("Watchtower refused the update request with {status}: {body}");
                Err(RequestError::UpdaterUnavailable)
            }
            Err(error) => {
                tracing::error!("Watchtower at {url} could not be reached: {error}");
                Err(RequestError::UpdaterUnavailable)
            }
        }
    }
}

fn unexpected_response(body: &Value) -> String {
    let body: String = body.to_string().chars().take(500).collect();
    tracing::warn!("Update check got an unexpected GitHub response: {body}");
    "GitHub returned an unexpected response.".to_owned()
}

/// Which stream of builds `version` came from.
pub fn channel(version: Option<&str>) -> Option<Channel> {
    match version {
        Some(version) if version.starts_with('v') => Some(Channel::Release),
        Some(version) if version.starts_with("nightly") => Some(Channel::Nightly),
        Some(version) if version.starts_with("preview") => Some(Channel::Preview),
        _ => None,
    }
}

/// Whether `latest` is newer than `current` on `channel`: releases compare as semantic
/// versions (numerically, not lexically), nightly and preview builds by commit prefix.
pub fn update_available(channel: Channel, current: &str, latest: &str) -> bool {
    match channel {
        Channel::Release => {
            if let (Some(current), Some(latest)) = (
                current.strip_prefix('v').and_then(parse_version),
                latest.strip_prefix('v').and_then(parse_version),
            ) {
                return latest > current;
            }
        }
        Channel::Nightly | Channel::Preview => {
            let prefix = channel.rolling().map(|(tag, _)| format!("{tag}-"));
            let commit = |version: &str| {
                prefix
                    .as_deref()
                    .and_then(|prefix| version.strip_prefix(prefix))
                    .filter(|commit| !commit.is_empty())
                    .map(str::to_owned)
            };
            if let (Some(current), Some(latest)) = (commit(current), commit(latest)) {
                return !(latest.starts_with(&current) || current.starts_with(&latest));
            }
        }
    }
    current != latest
}

/// Precedence-ordered parts of a semantic version (`Version.compare/2` ignores build metadata).
fn parse_version(version: &str) -> Option<(u64, u64, u64, semver::Prerelease)> {
    let parsed = semver::Version::parse(version).ok()?;
    Some((parsed.major, parsed.minor, parsed.patch, parsed.pre))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_release_versions_numerically_not_lexically() {
        assert!(update_available(Channel::Release, "v0.9.0", "v0.10.0"));
        assert!(!update_available(Channel::Release, "v0.10.0", "v0.9.0"));
        assert!(!update_available(Channel::Release, "v1.2.3", "v1.2.3"));
        assert!(update_available(Channel::Release, "v1.0.0-rc.1", "v1.0.0"));
        // Unparsable versions differ by text.
        assert!(update_available(Channel::Release, "v1.0", "v1.1"));
        assert!(!update_available(Channel::Release, "v1.0", "v1.0"));
    }

    #[test]
    fn accepts_nightly_commits_of_different_lengths() {
        assert!(!update_available(
            Channel::Nightly,
            "nightly-0123456",
            "nightly-0123456789ab"
        ));
        assert!(update_available(
            Channel::Nightly,
            "nightly-0123456",
            "nightly-fedcba9"
        ));
    }

    #[test]
    fn preview_builds_compare_by_commit_like_nightlies() {
        // The LXC installer records the checksum, the app the commit; both prefix-compare.
        assert!(!update_available(
            Channel::Preview,
            "preview-0123456",
            "preview-0123456789ab"
        ));
        assert!(update_available(
            Channel::Preview,
            "preview-0123456",
            "preview-fedcba9"
        ));
    }

    #[test]
    fn channels_follow_the_version_prefix() {
        assert_eq!(channel(Some("v1.0.0")), Some(Channel::Release));
        assert_eq!(channel(Some("nightly-abc")), Some(Channel::Nightly));
        assert_eq!(channel(Some("preview-abc")), Some(Channel::Preview));
        assert_eq!(channel(Some("dev")), None);
        assert_eq!(channel(None), None);
    }
}
