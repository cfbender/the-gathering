//! Keeping the Comprehensive Rules current.
//!
//! Wizards of the Coast links each release's text file from its rules page
//! (`MagicCompRules 20260925.txt` on `media.wizards.com`). A check reads that page and
//! installs the linked release when its URL differs from the installed one. The server
//! checks shortly after boot and then daily; administrators can check on demand.

use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Mutex;

use super::{Status, install, parse, record_check, status};
use crate::regex::{Regex, compile};
use crate::state::AppState;

/// Links to a release's text file.
static TXT_LINK: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?i)href\s*=\s*["']([^"'<>]*MagicCompRules[^"'<>]*\.txt)["']"#));
/// The release date in a file name.
static FILE_DATE: LazyLock<Regex> = LazyLock::new(|| compile(r"(\d{8})"));

/// Time between scheduled checks.
const CHECK_INTERVAL: Duration = Duration::from_hours(24);
/// The first check after boot.
const FIRST_CHECK: Duration = Duration::from_secs(15);
/// Downloads take longer than API calls; the release is about 1 MB.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// One check at a time, scheduled or requested.
#[derive(Debug, Default)]
pub struct RulesSync {
    running: Mutex<()>,
}

/// What a check did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The linked release was downloaded and installed.
    Installed,
    /// The linked release is the installed one.
    Current,
}

/// The newest text release the rules page links, resolved against `page_url`. Only links
/// on `media.wizards.com` (or the page's own origin) are accepted.
pub fn release_url(page_url: &url::Url, html: &str) -> Option<url::Url> {
    TXT_LINK
        .captures_iter(html)
        .filter_map(|captures| captures.get(1))
        .filter_map(|link| page_url.join(link.as_str().trim()).ok())
        .filter(|link| {
            (link.scheme() == "https" && link.host_str() == Some("media.wizards.com"))
                || link.origin() == page_url.origin()
        })
        .max_by_key(|link| {
            FILE_DATE
                .captures(link.path())
                .and_then(|captures| captures.get(1))
                .map(|date| date.as_str().to_owned())
        })
}

async fn fetch(state: &AppState, url: &url::Url) -> Result<String, String> {
    let response = state
        .http
        .get(url.clone())
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("could not reach {url}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("could not read {url}: {error}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn check_unlocked(state: &AppState, force: bool) -> Result<Outcome, String> {
    let page_url = url::Url::parse(&state.config.rules_page_url)
        .map_err(|error| format!("invalid rules page URL: {error}"))?;
    let page = fetch(state, &page_url).await?;
    let release = release_url(&page_url, &page)
        .ok_or_else(|| format!("{page_url} links no Comprehensive Rules text file"))?;
    let installed = status(&state.pool)
        .await
        .map_err(|error| error.to_string())?;
    if !force
        && installed.rule_count > 0
        && installed.source_url.as_deref() == Some(release.as_str())
    {
        return Ok(Outcome::Current);
    }
    let text = fetch(state, &release).await?;
    let document = parse(&text)
        .ok_or_else(|| format!("{release} is not in the Comprehensive Rules text layout"))?;
    install(&state.pool, release.as_str(), &document)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        rules = document.rules.len(),
        glossary = document.glossary.len(),
        "installed the Comprehensive Rules from {release}"
    );
    Ok(Outcome::Installed)
}

impl RulesSync {
    /// Checks for a new release and installs it; `force` reinstalls the linked release even
    /// when it is the installed one. Waits for a check already running. Records the check
    /// and returns the resulting status, or the error that stopped it.
    pub async fn check(&self, state: &AppState, force: bool) -> (Result<Outcome, String>, Status) {
        let _running = self.running.lock().await;
        let outcome = check_unlocked(state, force).await;
        match &outcome {
            Ok(Outcome::Installed) => {}
            Ok(Outcome::Current) => {
                if let Err(error) = record_check(&state.pool, None).await {
                    tracing::error!("recording a rules check: {error}");
                }
            }
            Err(error) => {
                tracing::warn!("Comprehensive Rules check failed: {error}");
                if let Err(error) = record_check(&state.pool, Some(error)).await {
                    tracing::error!("recording a rules check: {error}");
                }
            }
        }
        let status = status(&state.pool).await.unwrap_or_default();
        (outcome, status)
    }
}

/// Starts the scheduled checks when `rules_sync_enabled`: shortly after boot, then daily.
pub fn start(state: &AppState) {
    if !state.config.rules_sync_enabled {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let _ = state.rules_sync.check(&state, false).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_newest_text_release() {
        let page = url::Url::parse("https://magic.wizards.com/en/rules").unwrap();
        let html = r#"
            <a href="https://media.wizards.com/2026/downloads/MagicCompRules 20260925.pdf">PDF</a>
            <a class="cta" href="https://media.wizards.com/2026/downloads/MagicCompRules 20260411.txt">old</a>
            <a class="cta" href="https://media.wizards.com/2026/downloads/MagicCompRules 20260925.txt">TXT</a>
            <a href="https://evil.example.com/MagicCompRules 20991231.txt">elsewhere</a>
        "#;
        assert_eq!(
            release_url(&page, html).map(String::from).as_deref(),
            Some("https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt")
        );
        assert_eq!(release_url(&page, "<p>No rules here</p>"), None);
    }
}
