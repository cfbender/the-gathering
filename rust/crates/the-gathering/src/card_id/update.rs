//! Downloads new card-recognition bundles at boot and every six hours, the way ManaVault's
//! scanner bundle updater does: from the newest published `scanner-bundle-*` release of this
//! repository (`cardid.publish --to github:cfbender/the-gathering` creates one per bundle, each
//! file a release asset), or from a direct `.../manifest.json` URL (`CARDID_BUNDLE_SOURCE`).
//!
//! A bundle whose version differs from `current` is downloaded into `.incoming/<version>`,
//! every file is checked against the manifest's sizes and SHA-256 sums, and only then is it
//! moved into place and `current` swapped to it; the old `current` becomes `previous`, and
//! every other version is removed. Rolling back means unpublishing the newest release.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{FILES, bundle_dir, current_manifest, valid_version};
use crate::config::CardIdBundleSource;
use crate::state::AppState;

/// Tag prefix of bundle releases, shared with ManaVault and Oracle's publisher.
const RELEASE_PREFIX: &str = "scanner-bundle-";
const INTERVAL: Duration = Duration::from_hours(6);
const SUMS: &str = "SHA256SUMS";
const REQUIRED: [&str; 4] = ["arts.json", "detector.onnx", "embed.onnx", "search.onnx"];

/// What a check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// `CARDID_BUNDLE_SOURCE=off`.
    Disabled,
    /// The repository has no published bundle release.
    NoRelease,
    /// The newest bundle is already `current`.
    Current,
    /// This version was installed and is now `current`.
    Installed(String),
}

/// Why a check failed.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("requesting {0}: {1}")]
    Request(String, String),
    #[error("unexpected releases response from GitHub")]
    InvalidReleases,
    #[error("invalid manifest")]
    InvalidManifest,
    #[error("the release has no {0} asset")]
    AssetMissing(String),
    #[error("{0} does not match the manifest")]
    InvalidFile(String),
    #[error("installing: {0}")]
    Io(#[from] io::Error),
}

/// Checks for a new bundle at boot and then every six hours, unless the source is off.
pub fn start(state: &AppState) {
    if state.config.cardid_bundle_source == CardIdBundleSource::Off {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            match check_for_update(&state).await {
                Ok(Status::Installed(version)) => {
                    tracing::info!("Installed card-recognition bundle {version}");
                }
                Ok(_) => {}
                Err(error) => tracing::warn!("Card-recognition bundle update failed: {error}"),
            }
        }
    });
}

/// Checks the configured source once and installs its bundle when it is not `current`.
pub async fn check_for_update(state: &AppState) -> Result<Status, UpdateError> {
    let source = &state.config.cardid_bundle_source;
    if *source == CardIdBundleSource::Off {
        return Ok(Status::Disabled);
    }
    // Release assets redirect to GitHub's object storage, which the shared client refuses.
    let client = reqwest::Client::builder()
        .user_agent(crate::state::user_agent())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_mins(10))
        .build()
        .map_err(|error| UpdateError::Request("a client".into(), error.to_string()))?;
    let data_dir = &state.config.data_dir;
    match source {
        CardIdBundleSource::Off => Ok(Status::Disabled),
        CardIdBundleSource::Github => {
            let releases = get(&client, &state.config.cardid_bundle_releases)
                .await?
                .json::<Value>()
                .await
                .map_err(|_| UpdateError::InvalidReleases)?;
            let Some(assets) = newest_release(&releases)? else {
                return Ok(Status::NoRelease);
            };
            let manifest = assets
                .get("manifest.json")
                .ok_or_else(|| UpdateError::AssetMissing("manifest.json".into()))?;
            update_from(&client, data_dir, manifest, |name| {
                assets.get(name).cloned()
            })
            .await
        }
        CardIdBundleSource::Manifest(url) => {
            update_from(&client, data_dir, url.as_str(), |name| {
                url.join(name).ok().map(String::from)
            })
            .await
        }
    }
}

/// The asset download URLs, by name, of the newest published (non-draft) bundle release.
fn newest_release(releases: &Value) -> Result<Option<HashMap<String, String>>, UpdateError> {
    let releases = releases.as_array().ok_or(UpdateError::InvalidReleases)?;
    let newest = releases
        .iter()
        .filter(|release| {
            release.get("draft") != Some(&Value::Bool(true))
                && release
                    .get("tag_name")
                    .and_then(Value::as_str)
                    .is_some_and(|tag| tag.starts_with(RELEASE_PREFIX))
        })
        .max_by_key(|release| {
            release
                .get("published_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
        });
    Ok(newest.map(|release| {
        release
            .get("assets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|asset| {
                Some((
                    asset.get("name")?.as_str()?.to_owned(),
                    asset.get("browser_download_url")?.as_str()?.to_owned(),
                ))
            })
            .collect()
    }))
}

async fn get(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, UpdateError> {
    let failed = |message: String| UpdateError::Request(url.to_owned(), message);
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| failed(error.to_string()))?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(failed(response.status().to_string()))
    }
}

async fn update_from(
    client: &reqwest::Client,
    data_dir: &Path,
    manifest_url: &str,
    asset: impl Fn(&str) -> Option<String>,
) -> Result<Status, UpdateError> {
    let bytes = get(client, manifest_url)
        .await?
        .bytes()
        .await
        .map_err(|error| UpdateError::Request(manifest_url.to_owned(), error.to_string()))?;
    let manifest: Value =
        serde_json::from_slice(&bytes).map_err(|_| UpdateError::InvalidManifest)?;
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| valid_version(version))
        .ok_or(UpdateError::InvalidManifest)?
        .to_owned();
    let files = manifest
        .get("files")
        .and_then(Value::as_object)
        .filter(|files| REQUIRED.iter().all(|name| files.contains_key(*name)))
        .ok_or(UpdateError::InvalidManifest)?
        .clone();
    // Names become paths under `.incoming`, so only the known bundle files are accepted.
    if let Some(name) = files
        .keys()
        .find(|name| !FILES.contains(&name.as_str()) || *name == "manifest.json")
    {
        return Err(UpdateError::InvalidFile(name.clone()));
    }
    let current = current_manifest(data_dir).await;
    let current = current.as_ref().and_then(|current| current.get("version"));
    if current.and_then(Value::as_str) == Some(version.as_str()) {
        return Ok(Status::Current);
    }

    let root = bundle_dir(data_dir);
    let incoming = root.join(".incoming").join(&version);
    let _ = tokio::fs::remove_dir_all(&incoming).await;
    let result = async {
        tokio::fs::create_dir_all(&incoming).await?;
        tokio::fs::write(incoming.join("manifest.json"), &bytes).await?;
        for name in files.keys().map(String::as_str).chain([SUMS]) {
            let url = asset(name).ok_or_else(|| UpdateError::AssetMissing(name.to_owned()))?;
            download(client, &url, &incoming.join(name)).await?;
        }
        let (root, incoming, version) = (root.clone(), incoming.clone(), version.clone());
        tokio::task::spawn_blocking(move || {
            install(&root, &version, &files, &incoming).map(|()| Status::Installed(version))
        })
        .await
        .map_err(io::Error::other)?
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&incoming).await;
    result
}

async fn download(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> Result<(), UpdateError> {
    let mut response = get(client, url).await?;
    let mut file = tokio::fs::File::create(destination).await?;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| UpdateError::Request(url.to_owned(), error.to_string()))?
    {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

/// Verifies the downloaded files, moves them to `<version>`, points `current` at it (and
/// `previous` at the old `current`), and removes every other version.
fn install(
    root: &Path,
    version: &str,
    files: &Map<String, Value>,
    incoming: &Path,
) -> Result<(), UpdateError> {
    for (name, expected) in files {
        let invalid = || UpdateError::InvalidFile(name.clone());
        let bytes = expected.get("bytes").and_then(Value::as_u64);
        let sha256 = expected.get("sha256").and_then(Value::as_str);
        let path = incoming.join(name);
        let size = std::fs::metadata(&path)?.len();
        let mut hasher = Sha256::new();
        io::copy(&mut std::fs::File::open(&path)?, &mut hasher)?;
        let actual = crate::imports::csv::hex(&hasher.finalize());
        if bytes != Some(size) || !sha256.is_some_and(|sha| sha.eq_ignore_ascii_case(&actual)) {
            return Err(invalid());
        }
    }

    let destination = root.join(version);
    // A version already on disk is not `current` (that was checked), so replace it with the
    // verified copy; a rollback re-installs the bundle `previous` points at.
    if destination.exists() {
        std::fs::remove_dir_all(&destination)?;
    }
    std::fs::rename(incoming, &destination)?;
    let previous = std::fs::read_link(root.join("current")).ok();
    swap_symlink(root, "current", Path::new(version))?;
    if let Some(previous) = previous.filter(|previous| previous != Path::new(version)) {
        swap_symlink(root, "previous", &previous)?;
    }

    let keep: Vec<PathBuf> = ["current", "previous"]
        .iter()
        .filter_map(|link| std::fs::read_link(root.join(link)).ok())
        .collect();
    for entry in std::fs::read_dir(root)?.flatten() {
        let name = entry.file_name();
        let removable = name
            .to_str()
            .is_some_and(|name| valid_version(name) && !keep.iter().any(|kept| kept == name));
        if removable && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

/// Points `root/name` at `target` atomically, through a temporary link renamed over it.
fn swap_symlink(root: &Path, name: &str, target: &Path) -> io::Result<()> {
    let temporary = root.join(format!("{name}.tmp"));
    let _ = std::fs::remove_file(&temporary);
    std::os::unix::fs::symlink(target, &temporary)?;
    std::fs::rename(&temporary, root.join(name))
}
