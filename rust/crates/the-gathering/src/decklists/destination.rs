//! Validating member-supplied ManaVault origins and resolving them to an address the
//! server may contact (`TheGathering.Decklists.Destination`), on lotus's [`Allowlist`].

use std::net::IpAddr;

use lotus::decklist::{Allowlist, Origin, Resolver};
use url::Url;

const INVALID_ORIGIN: &str = "must be an allowed origin (scheme, host, and optional port only)";

/// `normalize_origin/1`: an `http(s)` origin with a lowercased host and no path, query,
/// fragment, or credentials. Plain HTTP needs `allow_insecure(host)`.
pub fn normalize_origin(
    value: &str,
    allow_insecure: &dyn Fn(&str) -> bool,
) -> Result<String, &'static str> {
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
    lotus::decklist::Origin::of(&url)
        .map(|origin| origin.to_string())
        .ok_or(INVALID_ORIGIN)
}

/// The destination resolved to no address or to one the policy blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the destination resolved to a blocked network address")]
pub struct Blocked;

/// `allowed_host?/1`: whether the operator listed `host` in `MANAVAULT_ALLOWED_HOSTS`.
pub fn allowed_host(host: &str, allowed_hosts: &[String]) -> bool {
    let host = host.to_lowercase();
    allowed_hosts.contains(&host)
}

/// `resolve/1`: the origin and the first address it resolves to, provided the host is
/// allowlisted or every address (IPv4 and IPv6) is public. An IP literal is its own answer;
/// no answer at all is blocked. Callers connect to the returned address only, so a later
/// DNS change cannot redirect the request.
pub async fn resolve(
    origin: &str,
    allowlist: &Allowlist,
    resolver: &dyn Resolver,
) -> Result<(Origin, IpAddr), Blocked> {
    let origin = Origin::parse(origin).ok_or(Blocked)?;
    let mut addresses = match origin.ip_literal() {
        Some(address) => vec![address],
        None => resolver.resolve(&origin.host).await.unwrap_or_default(),
    };
    let mut seen = Vec::with_capacity(addresses.len());
    addresses.retain(|address| {
        let fresh = !seen.contains(address);
        seen.push(*address);
        fresh
    });
    if !allowlist.allows(&origin.host, &addresses) {
        return Err(Blocked);
    }
    let address = addresses.first().copied().ok_or(Blocked)?;
    Ok((origin, address))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_origins() {
        let deny = |_: &str| false;
        assert_eq!(
            normalize_origin(" https://ManaVault.Example.com/ ", &deny).unwrap(),
            "https://manavault.example.com"
        );
        assert_eq!(
            normalize_origin("https://mv.example.com:8443", &deny).unwrap(),
            "https://mv.example.com:8443"
        );
        assert!(normalize_origin("http://mv.example.com", &deny).is_err());
        assert!(normalize_origin("http://mv.example.com", &|_: &str| true).is_ok());
        assert!(normalize_origin("https://mv.example.com/decks", &deny).is_err());
        assert!(normalize_origin("https://user@mv.example.com", &deny).is_err());
        assert!(normalize_origin("ftp://mv.example.com", &deny).is_err());
    }
}
