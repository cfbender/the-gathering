//! URLs for the shared Scryfall image cache.
//!
//! Browsers load card images through `/api/card-images?url=<scryfall source>`, which only
//! accepts Scryfall CDN JPEG URLs.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::regex::{Regex, compile};

static SOURCE: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"\Ahttps://cards\.scryfall\.io/(small|normal|art_crop)/(front|back)/[0-9a-f]/[0-9a-f]/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.jpg(?:\?[0-9]+)?\z",
    )
});
static PRINTING_ID: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z"));

/// Whether `source` is a Scryfall CDN image the cache may fetch.
pub fn valid_source(source: &str) -> bool {
    SOURCE.is_match(source)
}

/// The cached URL for a Scryfall image, or the input unchanged when it is not one.
pub fn url(source: &str) -> String {
    if valid_source(source) {
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("url", source)
            .finish();
        format!("/api/card-images?{query}")
    } else {
        source.to_owned()
    }
}

/// [`url`] for an optional source.
pub fn url_opt(source: Option<&str>) -> Option<String> {
    source.map(url)
}

/// [`url`] for every variant of an `image_uris` map.
pub fn urls(images: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    images
        .iter()
        .map(|(variant, source)| (variant.clone(), url(source)))
        .collect()
}

/// Cached `small` and `normal` front images of one printing, derived from its Scryfall id.
/// `None` for anything that is not a printing UUID.
pub fn printing_urls(id: &str) -> Option<BTreeMap<String, String>> {
    if !PRINTING_ID.is_match(id) {
        return None;
    }
    let mut chars = id.chars();
    let (a, b) = (chars.next()?, chars.next()?);
    Some(
        ["small", "normal"]
            .into_iter()
            .map(|variant| {
                (
                    variant.to_owned(),
                    url(&format!(
                        "https://cards.scryfall.io/{variant}/front/{a}/{b}/{id}.jpg"
                    )),
                )
            })
            .collect(),
    )
}

/// The Scryfall source behind a `/api/card-images` URL, or the input unchanged.
pub fn source(url: &str) -> Option<String> {
    match url.strip_prefix("/api/card-images?") {
        Some(query) => url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "url")
            .map(|(_, value)| value.into_owned()),
        None => Some(url.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE_URL: &str = "https://cards.scryfall.io/art_crop/front/a/b/ab000000-0000-0000-0000-000000000000.jpg?1700000000";

    #[test]
    fn wraps_only_scryfall_sources() {
        let wrapped = url(SOURCE_URL);
        assert!(wrapped.starts_with("/api/card-images?url=https%3A%2F%2Fcards.scryfall.io"));
        assert_eq!(source(&wrapped).as_deref(), Some(SOURCE_URL));
        assert_eq!(
            url("https://evil.example/x.jpg"),
            "https://evil.example/x.jpg"
        );
        let printing = printing_urls("ab000000-0000-0000-0000-000000000000").unwrap();
        assert!(printing["normal"].contains("normal%2Ffront%2Fa%2Fb%2Fab000000"));
        assert!(printing_urls("nope").is_none());
    }
}
