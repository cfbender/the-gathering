//! The bounded, shared disk cache for unchanged Scryfall JPEGs
//! (`TheGathering.Catalog.CardImages`, the `GenServer` half). Never fetches arbitrary
//! origins: only sources [`images::valid_source`] accepts.
//!
//! Files live in `DATA_DIR/card-images/<sha256(source)>.jpg` for 30 days, the directory is
//! capped at 512 MiB (oldest first), at most four downloads run at once with up to 128
//! distinct images waiting, concurrent requests for one image share a download, and a CDN
//! 429 pauses every download for its `retry-after` (at least 30 seconds).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use super::images;

const TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const CONCURRENCY: usize = 4;
const MAX_PENDING: usize = 128;
const SOURCE_ORIGIN: &str = "https://cards.scryfall.io";

/// Why an image could not be served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Not an accepted Scryfall source (400).
    BadRequest,
    /// The CDN has no such image (404).
    NotFound,
    /// The CDN failed, rate limited us, or sent something that is not a JPEG (502).
    BadGateway,
}

/// Whether the bytes came from disk (`hit`) or the CDN (`miss`), for `x-card-image-cache`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheStatus {
    /// Served from disk.
    Hit,
    /// Downloaded now.
    Miss,
}

impl CacheStatus {
    /// The header value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
        }
    }
}

type Reply = Result<(Bytes, CacheStatus), ImageError>;

enum Download {
    Ok(Bytes),
    Failed(ImageError),
    RateLimited(i64),
}

#[derive(Default)]
struct State {
    /// Cached files: size and creation time (Unix seconds).
    entries: HashMap<String, (u64, i64)>,
    /// Callers waiting on each in-flight or queued download.
    pending: HashMap<String, Vec<oneshot::Sender<Reply>>>,
    /// Running downloads.
    active: usize,
    /// Downloads waiting for a slot.
    queue: VecDeque<(String, String)>,
    /// No downloads start before this Unix second.
    paused_until: i64,
}

struct Inner {
    root: PathBuf,
    base: String,
    client: reqwest::Client,
    state: Mutex<State>,
}

/// Handle to the cache; cheap to clone.
#[derive(Clone)]
pub struct CardImages(Arc<Inner>);

impl std::fmt::Debug for CardImages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardImages")
            .field("root", &self.0.root)
            .finish_non_exhaustive()
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

fn key(source: &str) -> String {
    hex(&Sha256::digest(source.as_bytes()))
}

/// Lowercase hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.jpg"))
}

fn modified_seconds(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

/// Removes expired entries, then the oldest until the total fits; returns the ids removed.
fn prune(entries: &mut HashMap<String, (u64, i64)>) -> Vec<String> {
    let now = now();
    let mut total: u64 = entries.values().map(|(size, _)| *size).sum();
    let mut by_age: Vec<(String, u64, i64)> = entries
        .iter()
        .map(|(id, (size, time))| (id.clone(), *size, *time))
        .collect();
    by_age.sort_by_key(|(_, _, time)| *time);
    let mut removed = Vec::new();
    for (id, size, time) in by_age {
        if total > MAX_BYTES || now - time >= TTL_SECONDS {
            entries.remove(&id);
            total = total.saturating_sub(size);
            removed.push(id);
        }
    }
    removed
}

impl CardImages {
    /// Opens (creating) `data_dir/card-images`, indexing and pruning what is there.
    /// Downloads go to `base` in place of `https://cards.scryfall.io`.
    pub fn new(data_dir: &Path, base: &str) -> anyhow::Result<Self> {
        let root = data_dir.join("card-images");
        std::fs::create_dir_all(&root)?;
        let mut entries = HashMap::new();
        for entry in std::fs::read_dir(&root)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jpg") {
                continue;
            }
            let (Some(stem), Ok(metadata)) = (
                path.file_stem().and_then(|stem| stem.to_str()),
                entry.metadata(),
            ) else {
                continue;
            };
            entries.insert(
                stem.to_owned(),
                (metadata.len(), modified_seconds(&metadata)),
            );
        }
        for id in prune(&mut entries) {
            let _ = std::fs::remove_file(path(&root, &id));
        }
        let client = reqwest::Client::builder()
            .user_agent(crate::state::user_agent())
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(13))
            .build()?;
        Ok(Self(Arc::new(Inner {
            root,
            base: base.trim_end_matches('/').to_owned(),
            client,
            state: Mutex::new(State {
                entries,
                ..State::default()
            }),
        })))
    }

    /// The Unix second downloads resume after a CDN 429 (0 when not paused).
    pub fn paused_until(&self) -> i64 {
        self.0.state.lock().map_or(0, |state| state.paused_until)
    }

    /// Downloads running right now.
    pub fn active_downloads(&self) -> usize {
        self.0.state.lock().map_or(0, |state| state.active)
    }

    /// `fetch/1`: the JPEG for an accepted Scryfall source, from disk or the CDN.
    pub async fn fetch(&self, source: &str) -> Reply {
        if !images::valid_source(source) {
            return Err(ImageError::BadRequest);
        }
        let id = key(source);
        if let Some(body) = self.cached(&id).await {
            return Ok((body, CacheStatus::Hit));
        }
        let (sender, receiver) = oneshot::channel();
        {
            let Ok(mut state) = self.0.state.lock() else {
                return Err(ImageError::BadGateway);
            };
            if state.paused_until > now() {
                return Err(ImageError::BadGateway);
            }
            if let Some(waiting) = state.pending.get_mut(&id) {
                waiting.push(sender);
            } else if state.pending.len() >= MAX_PENDING {
                return Err(ImageError::BadGateway);
            } else {
                state.pending.insert(id.clone(), vec![sender]);
                state.queue.push_back((id, source.to_owned()));
            }
        }
        self.dispatch();
        receiver.await.unwrap_or(Err(ImageError::BadGateway))
    }

    async fn cached(&self, id: &str) -> Option<Bytes> {
        let fresh = {
            let state = self.0.state.lock().ok()?;
            state
                .entries
                .get(id)
                .is_some_and(|(_, created)| now() - created < TTL_SECONDS)
        };
        if !fresh {
            return None;
        }
        tokio::fs::read(path(&self.0.root, id))
            .await
            .ok()
            .map(Bytes::from)
    }

    fn dispatch(&self) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        while state.active < CONCURRENCY {
            let Some((id, source)) = state.queue.pop_front() else {
                break;
            };
            state.active += 1;
            let cache = self.clone();
            tokio::spawn(async move {
                let result = cache.download(&source).await;
                cache.finish(&id, result).await;
            });
        }
    }

    async fn finish(&self, id: &str, result: Download) {
        let reply = match result {
            Download::Ok(body) => {
                self.store(id, &body).await;
                Ok((body, CacheStatus::Miss))
            }
            Download::Failed(error) => Err(error),
            Download::RateLimited(seconds) => {
                self.pause(seconds);
                Err(ImageError::BadGateway)
            }
        };
        let callers = match self.0.state.lock() {
            Ok(mut state) => {
                state.active = state.active.saturating_sub(1);
                state.pending.remove(id).unwrap_or_default()
            }
            Err(_) => Vec::new(),
        };
        for caller in callers {
            let _ = caller.send(reply.clone());
        }
        self.dispatch();
    }

    /// Fails every queued download and refuses new ones for `seconds`.
    fn pause(&self, seconds: i64) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        let queued: Vec<(String, String)> = state.queue.drain(..).collect();
        for (id, _) in queued {
            for caller in state.pending.remove(&id).unwrap_or_default() {
                let _ = caller.send(Err(ImageError::BadGateway));
            }
        }
        state.paused_until = now() + seconds;
    }

    async fn store(&self, id: &str, body: &[u8]) {
        let target = path(&self.0.root, id);
        let temporary = target.with_extension("jpg.tmp");
        let written = match tokio::fs::write(&temporary, body).await {
            Ok(()) => tokio::fs::rename(&temporary, &target).await,
            Err(error) => Err(error),
        };
        if written.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
            return;
        }
        let removed = match self.0.state.lock() {
            Ok(mut state) => {
                let size = u64::try_from(body.len()).unwrap_or(u64::MAX);
                state.entries.insert(id.to_owned(), (size, now()));
                prune(&mut state.entries)
            }
            Err(_) => Vec::new(),
        };
        for id in removed {
            let _ = tokio::fs::remove_file(path(&self.0.root, &id)).await;
        }
    }

    async fn download(&self, source: &str) -> Download {
        let url = match source.strip_prefix(SOURCE_ORIGIN) {
            Some(rest) => format!("{}{rest}", self.0.base),
            None => return Download::Failed(ImageError::BadRequest),
        };
        let Ok(response) = self
            .0
            .client
            .get(&url)
            .header(reqwest::header::ACCEPT, "image/jpeg")
            .send()
            .await
        else {
            return Download::Failed(ImageError::BadGateway);
        };
        match response.status().as_u16() {
            200 => {}
            404 => return Download::Failed(ImageError::NotFound),
            429 => {
                let seconds = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<i64>().ok())
                    .map_or(30, |seconds| seconds.max(30));
                return Download::RateLimited(seconds);
            }
            _ => return Download::Failed(ImageError::BadGateway),
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else {
                return Download::Failed(ImageError::BadGateway);
            };
            if body.len().saturating_add(chunk.len()) > MAX_IMAGE_BYTES {
                return Download::Failed(ImageError::BadGateway);
            }
            body.extend_from_slice(&chunk);
        }
        if body.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Download::Ok(Bytes::from(body))
        } else {
            Download::Failed(ImageError::BadGateway)
        }
    }
}
