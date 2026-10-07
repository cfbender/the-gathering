//! Rendering the summary card to PNG (`TheGathering.Games.SummaryImage`).
//!
//! Elixir shelled out to `rsvg-convert`; this renders in-process with `resvg`, using the
//! system fonts (the container ships the `DejaVu` family). Commander art is downloaded from Scryfall's CDN
//! only, without redirects and with a size cap, and embedded as `data:` URIs so the
//! renderer never fetches anything itself.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use base64::Engine;
use futures_util::{StreamExt, stream};
use resvg::{tiny_skia, usvg};

use crate::catalog::{self, CardRef, images};

use super::model::Game;
use super::summary_card::{self, ArtKey, Images};

const MAX_ART_BYTES: usize = 2_000_000;
const CONCURRENCY: usize = 6;
const TASK_TIMEOUT: Duration = Duration::from_secs(6);

/// Why a summary could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// More than 30 summaries this minute.
    #[error("rate limited")]
    RateLimited,
    /// The SVG could not be rasterized.
    #[error("render failed: {0}")]
    Failed(String),
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Downloads commander art. `origin` replaces the scheme, host, and port of allowed URLs
/// (tests point it at a mock server); the allowlist always checks the original URL.
#[derive(Clone, Debug)]
pub struct ArtFetcher {
    client: reqwest::Client,
    origin: Option<String>,
}

impl ArtFetcher {
    /// A fetcher using `client` (which must not follow redirects).
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, origin: None }
    }

    /// Sends allowed requests to `origin` instead (tests).
    #[must_use]
    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }

    /// `SummaryImage.fetch_art/2`: a `data:` URI for a JPEG or PNG from
    /// `https://cards.scryfall.io`, or `None` for anything else (other hosts, ports,
    /// credentials, redirects, non-200 responses, other formats, or more than 2 MB).
    pub async fn fetch_art(&self, url: Option<&str>) -> Option<String> {
        let parsed = url::Url::parse(url?).ok()?;
        let allowed = parsed.scheme() == "https"
            && parsed.host_str() == Some("cards.scryfall.io")
            && parsed.port_or_known_default() == Some(443)
            && parsed.username().is_empty()
            && parsed.password().is_none();
        if !allowed {
            return None;
        }
        let target = match &self.origin {
            Some(origin) => {
                let mut target = format!("{}{}", origin.trim_end_matches('/'), parsed.path());
                if let Some(query) = parsed.query() {
                    target.push('?');
                    target.push_str(query);
                }
                target
            }
            None => parsed.to_string(),
        };
        let response = self.client.get(target).timeout(Duration::from_secs(3)).send().await.ok()?;
        if response.status() != reqwest::StatusCode::OK {
            return None;
        }
        let mut body = Vec::new();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            body.extend_from_slice(&chunk.ok()?);
            if body.len() > MAX_ART_BYTES {
                return None;
            }
        }
        let kind = if body.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "jpeg"
        } else if body.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            "png"
        } else {
            return None;
        };
        Some(format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(&body)))
    }
}

/// Every commander and partner slot of the game's decks.
fn art_keys(game: &Game) -> Vec<ArtKey> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    for deck in game.seats.iter().filter_map(|seat| seat.deck.as_ref()) {
        let slots = [
            Some((deck.commander_card_id.clone(), deck.commander_name.clone(), deck.commander_printing_id.clone())),
            deck.partner_name
                .clone()
                .map(|name| (deck.partner_card_id.clone(), name, deck.partner_printing_id.clone())),
        ];
        for key in slots.into_iter().flatten() {
            if seen.insert(key.clone()) {
                keys.push(key);
            }
        }
    }
    keys
}

/// Downloads the art for every commander slot (six at a time, six seconds each).
pub async fn artwork(pool: &crate::db::Pool, fetcher: &ArtFetcher, game: &Game) -> Result<Images, sqlx::Error> {
    let keys = art_keys(game);
    let refs: Vec<CardRef> = keys
        .iter()
        .flat_map(|(id, name, printing)| {
            [CardRef::Card(id.clone(), Some(name.clone())), CardRef::Printing(printing.clone())]
        })
        .collect();
    let urls = catalog::art_crop_urls_in(&mut *pool.acquire().await?, &refs).await?;
    let sources: Vec<(ArtKey, Option<String>)> = keys
        .into_iter()
        .map(|key| {
            let url = urls.art_crop_url(key.0.as_deref(), Some(&key.1), key.2.as_deref());
            let source = url.as_deref().and_then(images::source);
            (key, source)
        })
        .collect();
    Ok(stream::iter(sources)
        .map(|(key, source)| async move {
            let image = tokio::time::timeout(TASK_TIMEOUT, fetcher.fetch_art(source.as_deref())).await.ok().flatten();
            image.map(|image| (key, image))
        })
        .buffer_unordered(CONCURRENCY)
        .filter_map(|entry| async move { entry })
        .collect()
        .await)
}

static FONTS: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_system_fonts();
    fonts.set_sans_serif_family("DejaVu Sans");
    fonts.set_monospace_family("DejaVu Sans Mono");
    Arc::new(fonts)
});

/// Rasterizes an SVG to PNG.
pub fn rasterize(svg: &str) -> Result<Vec<u8>, RenderError> {
    let options = usvg::Options { fontdb: Arc::clone(&FONTS), ..usvg::Options::default() };
    let tree = usvg::Tree::from_str(svg, &options).map_err(|error| RenderError::Failed(error.to_string()))?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| RenderError::Failed("empty image".into()))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|error| RenderError::Failed(error.to_string()))
}

/// `SummaryImage.render/1` without the rate limit: downloads art and renders the PNG.
pub async fn render(pool: &crate::db::Pool, fetcher: &ArtFetcher, game: &Game) -> Result<Vec<u8>, RenderError> {
    let images = artwork(pool, fetcher, game).await?;
    let svg = summary_card::svg(game, &images);
    tokio::task::spawn_blocking(move || rasterize(&svg))
        .await
        .map_err(|error| RenderError::Failed(error.to_string()))?
}
