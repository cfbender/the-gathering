//! Validating member-supplied ManaVault origins (`TheGathering.Decklists.Destination`).

use url::Url;

const INVALID_ORIGIN: &str = "must be an allowed origin (scheme, host, and optional port only)";

/// `normalize_origin/1`: an `http(s)` origin with a lowercased host and no path, query,
/// fragment, or credentials. Plain HTTP needs `allow_insecure(host)`.
pub fn normalize_origin(value: &str, allow_insecure: &dyn Fn(&str) -> bool) -> Result<String, &'static str> {
    let url = Url::parse(value.trim()).map_err(|_| INVALID_ORIGIN)?;
    let scheme_ok = matches!(url.scheme(), "http" | "https");
    let host = url.host_str().unwrap_or_default().to_lowercase();
    let valid = scheme_ok
        && !host.is_empty()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none()
        && (url.scheme() == "https" || allow_insecure(&host));
    if !valid {
        return Err(INVALID_ORIGIN);
    }
    lotus::decklist::Origin::of(&url).map(|origin| origin.to_string()).ok_or(INVALID_ORIGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_origins() {
        let deny = |_: &str| false;
        assert_eq!(normalize_origin(" https://ManaVault.Example.com/ ", &deny).unwrap(), "https://manavault.example.com");
        assert_eq!(normalize_origin("https://mv.example.com:8443", &deny).unwrap(), "https://mv.example.com:8443");
        assert!(normalize_origin("http://mv.example.com", &deny).is_err());
        assert!(normalize_origin("http://mv.example.com", &|_: &str| true).is_ok());
        assert!(normalize_origin("https://mv.example.com/decks", &deny).is_err());
        assert!(normalize_origin("https://user@mv.example.com", &deny).is_err());
        assert!(normalize_origin("ftp://mv.example.com", &deny).is_err());
    }
}
