//! The published card-recognition bundle the webcam table loads in the browser
//! (`TheGathering.CardId`).
//!
//! Bundles are built by Oracle (<https://github.com/cfbender/oracle>, `python -m cardid.export`)
//! and copied to the server with `python -m cardid.publish <bundle> --to host:DATA_DIR/cardid`,
//! which leaves this layout:
//!
//! ```text
//! DATA_DIR/cardid/<version>/{manifest.json,arts.json,detector.onnx,embed.onnx,search.onnx}
//! DATA_DIR/cardid/current -> <version>
//! ```
//!
//! The app never runs the models; it serves the files of the version `current` points at.

pub mod corrections;

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde_json::Value;

use crate::regex::{Regex, compile};

/// Bundle file names the app serves, in the order the browser loads them.
pub const FILES: [&str; 6] = [
    "manifest.json",
    "arts.json",
    "detector.onnx",
    "embed.onnx",
    "search.onnx",
    "printings.json",
];

static VERSION: LazyLock<Regex> = LazyLock::new(|| compile(r"\A[A-Za-z0-9][A-Za-z0-9._-]*\z"));

/// The bundle root, `DATA_DIR/cardid`.
pub fn bundle_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cardid")
}

fn valid_version(version: &str) -> bool {
    version != "current" && VERSION.is_match(version)
}

/// `current_manifest/0`: the manifest of the bundle `current` points at, or `None` when
/// nothing valid has been published. The version is read from the manifest, not the
/// symlink, so a plain copied directory named `current` works too.
pub async fn current_manifest(data_dir: &Path) -> Option<Value> {
    let contents = tokio::fs::read(bundle_dir(data_dir).join("current").join("manifest.json"))
        .await
        .ok()?;
    let manifest: Value = serde_json::from_slice(&contents).ok()?;
    let version = manifest.get("version")?.as_str()?;
    valid_version(version).then_some(manifest)
}

/// `file_path/2`: a bundle file, refusing names and versions outside the bundle root.
pub async fn file_path(data_dir: &Path, version: &str, name: &str) -> Option<PathBuf> {
    if !valid_version(version) || !FILES.contains(&name) {
        return None;
    }
    let path = bundle_dir(data_dir).join(version).join(name);
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    metadata.is_file().then_some(path)
}

/// The content type each bundle file is served with.
pub fn content_type(name: &str) -> &'static str {
    let json = Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if json {
        "application/json"
    } else {
        "application/octet-stream"
    }
}
