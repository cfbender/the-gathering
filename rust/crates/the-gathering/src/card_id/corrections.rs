//! Human-labelled click crops for card recognition.
//!
//! JPEGs stay native; Oracle's offline importer creates `card.png`. An outline drawn with
//! Shift+click arrives as `quad_source: "manual"` and is detector ground truth for Oracle;
//! otherwise the quad is the detector's own. Writes are serialized, with the append-only
//! label log as the commit point, and repeated capture/label submissions are idempotent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use base64::Engine;
use serde_json::Value;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::catalog::printing_id;
use crate::regex::{Regex, compile};

static UUID: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z"));

const FIELDS: [&str; 10] = [
    "capture_id",
    "label",
    "click",
    "quad",
    "quad_source",
    "up_vote",
    "bundle_version",
    "top1",
    "similarity",
    "margin",
];
/// `manual`: the clicker drew the outline, so Oracle may train the detector on it.
const QUAD_SOURCES: [&str; 2] = ["detector", "manual"];
const IMAGE_PREFIX: &str = "data:image/jpeg;base64,";
const MAX_ENCODED_BYTES: usize = 190_000;
const PAGE_SIZE: usize = 50;

/// Why a correction was refused.
#[derive(Debug, thiserror::Error)]
pub enum CorrectionError {
    /// Malformed payload (400).
    #[error("bad request")]
    BadRequest,
    /// Another member owns this capture (403).
    #[error("forbidden")]
    Forbidden,
    /// Writing to disk failed (500).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// One page of the label log.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Page {
    /// Labels, oldest first.
    pub corrections: Vec<Value>,
    /// Where the next page starts.
    pub cursor: usize,
    /// Whether this page was full.
    pub has_more: bool,
}

/// The corrections directory, `DATA_DIR/cardid/corrections`, with its writer lock.
#[derive(Debug)]
pub struct Corrections {
    dir: PathBuf,
    lock: tokio::sync::Mutex<()>,
}

fn uuid(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|id| UUID.is_match(id))
}

fn printing(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|id| printing_id::parse(id).is_some())
}

fn number(value: &Value, low: f64, high: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|n| value.is_number() && n >= low && n <= high)
}

fn optional_number(value: Option<&Value>, low: f64, high: f64) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(value) => number(value, low, high),
    }
}

fn point(value: &Value, low: f64, high: f64) -> bool {
    matches!(value.as_array().map(Vec::as_slice), Some([x, y]) if number(x, low, high) && number(y, low, high))
}

fn quad(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Array(points)) => {
            points.len() == 4 && points.iter().all(|p| point(p, -2048.0, 2048.0))
        }
        Some(_) => false,
    }
}

fn quad_source(source: Option<&Value>, quad: Option<&Value>) -> bool {
    match source {
        None | Some(Value::Null) => true,
        Some(Value::String(source))
            if source == "manual" && matches!(quad, None | Some(Value::Null)) =>
        {
            false
        }
        Some(Value::String(source)) => QUAD_SOURCES.contains(&source.as_str()),
        Some(_) => false,
    }
}

/// Reads baseline/progressive JPEG frame dimensions without decoding pixels. Full image
/// validation and the warp happen in the bounded offline importer.
fn jpeg_size(mut rest: &[u8]) -> Option<(u32, u32)> {
    loop {
        match rest {
            [0xFF, 0xC0 | 0xC2, _, _, 8, h1, h2, w1, w2, ..] => {
                return Some((
                    u32::from(u16::from_be_bytes([*w1, *w2])),
                    u32::from(u16::from_be_bytes([*h1, *h2])),
                ));
            }
            [0xFF, marker, l1, l2, tail @ ..] if !matches!(marker, 0xD8..=0xDA) => {
                let length = usize::from(u16::from_be_bytes([*l1, *l2]));
                if length < 2 {
                    return None;
                }
                rest = tail.get(length - 2..)?;
            }
            _ => return None,
        }
    }
}

fn validate(params: &Value) -> Option<Vec<u8>> {
    let encoded = params.get("image")?.as_str()?.strip_prefix(IMAGE_PREFIX)?;
    if encoded.len() > MAX_ENCODED_BYTES {
        return None;
    }
    let click = params.get("click");
    let valid = uuid(params.get("capture_id"))
        && printing(params.get("label"))
        && click.is_some_and(|click| point(click, 0.0, 640.0))
        && quad(params.get("quad"))
        && quad_source(params.get("quad_source"), params.get("quad"))
        && optional_number(params.get("up_vote"), 0.0, 2.0)
        && optional_number(params.get("similarity"), -2.0, 2.0)
        && optional_number(params.get("margin"), 0.0, 4.0)
        && (matches!(params.get("top1"), None | Some(Value::Null)) || printing(params.get("top1")))
        && params
            .get("bundle_version")
            .and_then(Value::as_str)
            .is_some_and(|version| version.len() <= 120);
    if !valid {
        return None;
    }
    let jpeg = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let rest = jpeg.strip_prefix(&[0xFF, 0xD8])?;
    if !jpeg.ends_with(&[0xFF, 0xD9]) {
        return None;
    }
    let (width, height) = jpeg_size(rest)?;
    let [x, y] = click?.as_array()?.as_slice() else {
        return None;
    };
    let (x, y) = (x.as_f64()?, y.as_f64()?);
    let fits = (1..=640).contains(&width)
        && (1..=640).contains(&height)
        && x <= f64::from(width)
        && y <= f64::from(height);
    fits.then_some(jpeg)
}

/// A stable fifth of captures is held out for evaluation.
fn split_for(id: &str) -> &'static str {
    let remainder = Sha1::digest(id.as_bytes())
        .iter()
        .fold(0_u32, |acc, byte| (acc * 256 + u32::from(*byte)) % 5);
    if remainder == 0 { "eval" } else { "train" }
}

impl Corrections {
    /// Corrections under `DATA_DIR/cardid/corrections`.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: super::bundle_dir(data_dir).join("corrections"),
            lock: tokio::sync::Mutex::new(()),
        }
    }

    /// The directory.
    pub fn directory(&self) -> &Path {
        &self.dir
    }

    /// Validates and stores a correction for `user_id`, returning its capture id.
    pub async fn save(&self, params: &Value, user_id: i64) -> Result<String, CorrectionError> {
        let jpeg = validate(params).ok_or(CorrectionError::BadRequest)?;
        let id = params
            .get("capture_id")
            .and_then(Value::as_str)
            .ok_or(CorrectionError::BadRequest)?
            .to_owned();
        let _guard = self.lock.lock().await;
        let dir = self.dir.join(&id);
        let owner_path = dir.join("owner");
        let owner = user_id.to_string();
        if let Ok(existing) = tokio::fs::read_to_string(&owner_path).await
            && existing != owner
        {
            return Err(CorrectionError::Forbidden);
        }
        let mut row: BTreeMap<String, Value> = FIELDS
            .iter()
            .filter_map(|field| {
                params
                    .get(*field)
                    .map(|value| ((*field).to_owned(), value.clone()))
            })
            .collect();
        row.insert("split".to_owned(), Value::String(split_for(&id).to_owned()));
        row.insert(
            "source".to_owned(),
            Value::String("webcam-table".to_owned()),
        );
        let encoded = serde_json::to_string(&row).map_err(std::io::Error::other)?;
        let latest_path = dir.join("label.json");
        if tokio::fs::read_to_string(&latest_path)
            .await
            .ok()
            .as_deref()
            != Some(encoded.as_str())
        {
            tokio::fs::create_dir_all(&dir).await?;
            tokio::fs::write(&owner_path, &owner).await?;
            // Never overwrite the image of an existing capture, including after a relabel.
            let crop = dir.join("crop.jpg");
            if tokio::fs::metadata(&crop).await.is_err() {
                tokio::fs::write(&crop, &jpeg).await?;
            }
            let mut log = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join("labels.jsonl"))
                .await?;
            log.write_all(format!("{encoded}\n").as_bytes()).await?;
            log.flush().await?;
            tokio::fs::write(&latest_path, &encoded).await?;
        }
        Ok(id)
    }

    /// Up to 50 labels after `cursor`.
    pub async fn page(&self, cursor: usize) -> Result<Page, CorrectionError> {
        let contents = match tokio::fs::read_to_string(self.dir.join("labels.jsonl")).await {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let corrections: Vec<Value> = contents
            .lines()
            .filter(|line| !line.is_empty())
            .skip(cursor)
            .take(PAGE_SIZE)
            .map(|line| serde_json::from_str(line).map_err(std::io::Error::other))
            .collect::<Result<_, _>>()?;
        let count = corrections.len();
        Ok(Page {
            corrections,
            cursor: cursor + count,
            has_more: count == PAGE_SIZE,
        })
    }

    /// A capture's stored crop.
    pub async fn crop_path(&self, id: &str) -> Option<PathBuf> {
        if !UUID.is_match(id) {
            return None;
        }
        let path = self.dir.join(id).join("crop.jpg");
        tokio::fs::metadata(&path)
            .await
            .ok()
            .filter(std::fs::Metadata::is_file)
            .map(|_| path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frame_dimensions_after_other_segments() {
        // SOI, an APP0 segment of length 4, then SOF0 with 8-bit precision, 20x10.
        let rest = [
            0xFF, 0xE0, 0x00, 0x04, 0xAA, 0xBB, 0xFF, 0xC0, 0x00, 0x11, 8, 0x00, 0x0A, 0x00, 0x14,
        ];
        assert_eq!(jpeg_size(&rest), Some((20, 10)));
        assert_eq!(jpeg_size(&[0xFF, 0xDA, 0, 2]), None);
    }

    #[test]
    fn splits_a_fifth_for_evaluation_deterministically() {
        assert_eq!(split_for("a"), split_for("a"));
        let evals = (0..1000)
            .filter(|i| split_for(&format!("capture-{i}")) == "eval")
            .count();
        assert!((150..250).contains(&evals), "{evals}");
    }
}
