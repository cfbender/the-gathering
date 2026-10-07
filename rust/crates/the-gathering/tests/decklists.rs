//! Ported from `test/the_gathering/decklists_test.exs`, `decklists/destination_test.exs`,
//! `decklists/cache_test.exs` (the remote-deck cache key test; the cache itself is unit
//! tested in `decklists::cache`), `test/the_gathering_web/controllers/api/decklist_controller_test.exs`,
//! the index and sync tests of `remote_deck_controller_test.exs`, and
//! `test/the_gathering/games/sync_remote_decks_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lotus::Zone;
use lotus::decklist::{Allowlist, Resolver, Source, SystemResolver};
use serde_json::{Value, json};
use support::{TestApp, json_fixture};
use the_gathering::accounts::User;
use the_gathering::config::Config;
use the_gathering::decklists::destination::{self, Blocked};
use the_gathering::decklists::remote_decks::Limits;
use the_gathering::decklists::{DeckCard, DecklistError, LinkSource};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Answers from a fixed table; unknown hosts do not resolve.
struct StaticResolver(HashMap<String, Vec<IpAddr>>);

impl StaticResolver {
    fn new(entries: &[(&str, &str)]) -> Arc<Self> {
        let mut table: HashMap<String, Vec<IpAddr>> = HashMap::new();
        for (host, address) in entries {
            table
                .entry((*host).to_owned())
                .or_default()
                .push(address.parse().unwrap());
        }
        Arc::new(Self(table))
    }
}

impl Resolver for StaticResolver {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send + 'a>> {
        let answer = self
            .0
            .get(host)
            .cloned()
            .ok_or_else(|| std::io::Error::other("no such host"));
        Box::pin(async move { answer })
    }
}

/// Answers each lookup with the next queued set of addresses.
struct QueuedResolver(Mutex<VecDeque<Vec<IpAddr>>>);

impl Resolver for QueuedResolver {
    fn resolve<'a>(
        &'a self,
        _host: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send + 'a>> {
        let answer = self.0.lock().unwrap().pop_front().unwrap_or_default();
        Box::pin(async move { Ok(answer) })
    }
}

async fn stub_app(server: &MockServer, adjust: impl FnOnce(&mut Config)) -> TestApp {
    let uri = server.uri();
    TestApp::with_config(|config| {
        config.moxfield_api_base.clone_from(&uri);
        config.archidekt_api_base = uri;
        adjust(config);
    })
    .await
}

// ---- decklists_test.exs ----

#[tokio::test]
async fn parses_and_canonicalizes_provider_url_variants() {
    let app = TestApp::new().await;
    let token = "AbCdEfGhIjKlMnOpQrStUvWx";
    let cases = [
        // lotus requires Moxfield ids of at least five characters ("abc" in the Elixir test).
        (
            "https://www.moxfield.com/decks/abcde?x=1",
            Source::Moxfield,
            "abcde",
            "https://moxfield.com/decks/abcde",
        ),
        (
            "https://moxfield.com/decks/a-b_C/primer/",
            Source::Moxfield,
            "a-b_C",
            "https://moxfield.com/decks/a-b_C",
        ),
        (
            "https://archidekt.com/decks/123/some-slug",
            Source::Archidekt,
            "123",
            "https://archidekt.com/decks/123",
        ),
        (
            "https://www.archidekt.com/decks/456/?foo=bar",
            Source::Archidekt,
            "456",
            "https://archidekt.com/decks/456",
        ),
    ];
    for (input, source, id, canonical) in cases {
        let parsed = app.state.decklists.parse_url(input).unwrap();
        assert_eq!(parsed.source, LinkSource::Supported(source), "{input}");
        assert_eq!(parsed.id, id);
        assert_eq!(parsed.canonical_url, canonical);
    }
    for input in [
        format!("https://manavault.example.com/share/decks/{token}?view=grid"),
        format!("https://www.manavault.example.com/share/decks/{token}/"),
    ] {
        let parsed = app.state.decklists.parse_url(&input).unwrap();
        assert_eq!(
            parsed.source,
            LinkSource::Supported(Source::ManaVault),
            "{input}"
        );
        assert_eq!(parsed.id, token);
        assert_eq!(
            parsed.canonical_url,
            format!("https://manavault.example.com/share/decks/{token}")
        );
    }
}

#[tokio::test]
async fn treats_manavault_links_as_other_when_no_manavault_url_is_configured() {
    let app = TestApp::with_config(|config| config.manavault_url = None).await;
    let url = "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx";
    let parsed = app.state.decklists.parse_url(url).unwrap();
    assert_eq!(parsed.source, LinkSource::Other);
    assert_eq!(parsed.canonical_url, url);
    assert_eq!(
        app.state.decklists.resolve(url).await,
        Err(DecklistError::UnsupportedUrl)
    );
}

#[tokio::test]
async fn returns_other_for_valid_unknown_links_and_invalid_url_for_garbage() {
    let app = TestApp::new().await;
    let parsed = app
        .state
        .decklists
        .parse_url("http://example.com/a?b=1#section")
        .unwrap();
    assert_eq!(parsed.source, LinkSource::Other);
    assert_eq!(parsed.canonical_url, "http://example.com/a?b=1");
    assert_eq!(
        app.state.decklists.parse_url("not a URL"),
        Err(DecklistError::InvalidUrl)
    );
    assert_eq!(
        app.state
            .decklists
            .parse_url("ftp://moxfield.com/decks/abcde"),
        Err(DecklistError::InvalidUrl)
    );
    // Short share tokens and other share kinds are not deck links.
    let short = app
        .state
        .decklists
        .parse_url("https://manavault.example.com/share/decks/short")
        .unwrap();
    assert_eq!(short.source, LinkSource::Other);
}

fn card(name: &str, quantity: u32, zone: Zone, printing: Option<&str>) -> DeckCard {
    DeckCard {
        name: name.to_owned(),
        quantity,
        zone,
        printing_id: printing.map(str::to_owned),
    }
}

async fn mount_fixture(server: &MockServer, route: &str, fixture: &str) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(fixture)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn resolves_a_moxfield_deck_with_partner_commanders() {
    let server = MockServer::start().await;
    mount_fixture(
        &server,
        "/v3/decks/all/partners",
        "decklists/moxfield_partner.json",
    )
    .await;
    let app = stub_app(&server, |_| {}).await;
    let deck = app
        .state
        .decklists
        .resolve("https://moxfield.com/decks/partners")
        .await
        .unwrap();
    assert_eq!(deck.name.as_deref(), Some("Partner Commander"));
    assert_eq!(
        deck.commanders,
        ["Kraum, Ludevic's Opus", "Malcolm, Keen-Eyed Navigator"]
    );
    assert_eq!(
        deck.color_identity,
        Some(vec!["U".to_owned(), "R".to_owned()])
    );
    assert_eq!(deck.author.as_deref(), Some("Goodybarsco"));
    assert_eq!(deck.card_count, Some(100));
    assert_eq!(
        deck.cards,
        [
            card(
                "Kraum, Ludevic's Opus",
                1,
                Zone::Commander,
                Some("5b4d8b79-7a17-4f07-9dd5-4bb3ee0d3a5d")
            ),
            card(
                "Malcolm, Keen-Eyed Navigator",
                1,
                Zone::Commander,
                Some("9d5b2c1e-3e77-4a4f-9d52-1b4a3c8e6f10")
            ),
            card(
                "Island",
                12,
                Zone::Mainboard,
                Some("a1b2c3d4-0000-4000-8000-000000000001")
            ),
            card(
                "Sol Ring",
                1,
                Zone::Mainboard,
                Some("7e0c2f04-1d50-4fcd-9f1c-3c2a1b0e9d8f")
            ),
        ]
    );
    let rendered = deck.to_json();
    assert_eq!(
        rendered["commanders"],
        json!([{"name": "Kraum, Ludevic's Opus"}, {"name": "Malcolm, Keen-Eyed Navigator"}])
    );
    assert_eq!(rendered["source"], "moxfield");
}

#[tokio::test]
async fn resolves_an_archidekt_deck_with_a_background() {
    let server = MockServer::start().await;
    mount_fixture(
        &server,
        "/api/decks/24907541/",
        "decklists/archidekt_background.json",
    )
    .await;
    let app = stub_app(&server, |_| {}).await;
    let deck = app
        .state
        .decklists
        .resolve("https://archidekt.com/decks/24907541/slug")
        .await
        .unwrap();
    assert_eq!(
        deck.commanders,
        ["Noble Heritage", "Wilson, Refined Grizzly"]
    );
    assert_eq!(
        deck.color_identity,
        Some(vec!["W".to_owned(), "G".to_owned()])
    );
    assert_eq!(deck.author.as_deref(), Some("Will3545"));
    // The Maybeboard is excluded from the deck, so neither the count nor the list has Cultivate.
    assert_eq!(deck.card_count, Some(100));
    let summary: Vec<(&str, u32, Zone)> = deck
        .cards
        .iter()
        .map(|card| (card.name.as_str(), card.quantity, card.zone))
        .collect();
    assert_eq!(
        summary,
        [
            ("Noble Heritage", 1, Zone::Commander),
            ("Wilson, Refined Grizzly", 1, Zone::Commander),
            ("Forest", 38, Zone::Mainboard),
            ("Other cards", 60, Zone::Mainboard),
        ]
    );
    let printings: Vec<Option<&str>> = deck
        .cards
        .iter()
        .map(|card| card.printing_id.as_deref())
        .collect();
    assert_eq!(
        printings,
        [
            Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e01"),
            Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e02"),
            Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03"),
            None
        ]
    );
}

/// The configured ManaVault origin, served by `server` (plain HTTP on its port; the test
/// resolver points the hostname at the stub).
async fn manavault_app(server: &MockServer) -> (TestApp, String) {
    let port = server.address().port();
    let origin = format!("http://manavault.example.com:{port}");
    let configured = origin.clone();
    let app = stub_app(server, |config| config.manavault_url = Some(configured)).await;
    app.state.decklists.set_resolver(StaticResolver::new(&[(
        "manavault.example.com",
        "127.0.0.1",
    )]));
    (app, origin)
}

#[tokio::test]
async fn resolves_a_manavault_shared_deck() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json_fixture("decklists/manavault.json")),
        )
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    let url = format!("{origin}/share/decks/AbCdEfGhIjKlMnOpQrStUvWx");
    let deck = app.state.decklists.resolve(&url).await.unwrap();
    assert_eq!(deck.url, url);
    assert_eq!(deck.name.as_deref(), Some("Shared Deck"));
    assert_eq!(deck.commanders, ["Shorikai, Genesis Engine"]);
    assert_eq!(
        deck.color_identity,
        Some(vec!["W".to_owned(), "U".to_owned()])
    );
    assert_eq!(deck.author, None);
    assert_eq!(deck.card_count, Some(100));
    // `considering` is left out; the preferred printing wins over the fallback.
    assert_eq!(
        deck.cards,
        [
            card(
                "Shorikai, Genesis Engine",
                1,
                Zone::Commander,
                Some("b3a0e8d4-1f2c-4c4e-9a55-6f1d2e3c4b01")
            ),
            card(
                "Sol Ring",
                1,
                Zone::Mainboard,
                Some("b3a0e8d4-1f2c-4c4e-9a55-6f1d2e3c4b02")
            ),
        ]
    );
    let request = &server.received_requests().await.unwrap()[0];
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["variables"]["id"], "AbCdEfGhIjKlMnOpQrStUvWx");
}

/// Answers the deck query page by page, following `variables.after`.
struct Pages;

fn node(zone: &str, name: &str, quantity: u32) -> Value {
    json!({"node": {"zone": zone, "quantity": quantity, "card": {"name": name}}})
}

impl Respond for Pages {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let (edges, page_info) = match body["variables"]["after"].as_str() {
            None => (
                vec![node("commander", "Shorikai, Genesis Engine", 1)],
                json!({"endCursor": "c1", "hasNextPage": true}),
            ),
            Some("c1") => (
                vec![
                    node("mainboard", "Sol Ring", 1),
                    node("mainboard", "Island", 30),
                ],
                json!({"endCursor": "c2", "hasNextPage": false}),
            ),
            Some(other) => panic!("unexpected cursor {other}"),
        };
        ResponseTemplate::new(200).set_body_json(json!({"data": {"deck": {
            "name": "Paged", "cardCount": 32, "commanderColorIdentity": ["W", "U"],
            "deckCards": {"pageInfo": page_info, "edges": edges}
        }}}))
    }
}

#[tokio::test]
async fn follows_manavault_deck_card_pages() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(Pages)
        .expect(2)
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    let deck = app
        .state
        .decklists
        .resolve(&format!("{origin}/share/decks/PagedPagedPagedPagedPaged"))
        .await
        .unwrap();
    let cursors: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            serde_json::from_slice::<Value>(&request.body).unwrap()["variables"]["after"].clone()
        })
        .collect();
    assert_eq!(cursors, [Value::Null, json!("c1")]);
    let cards: Vec<(&str, u32)> = deck
        .cards
        .iter()
        .map(|card| (card.name.as_str(), card.quantity))
        .collect();
    assert_eq!(
        cards,
        [
            ("Shorikai, Genesis Engine", 1),
            ("Sol Ring", 1),
            ("Island", 30)
        ]
    );
    assert_eq!(deck.commanders, ["Shorikai, Genesis Engine"]);
}

/// Answers `pages` deck pages of one card each, then stops (or never stops with `None`).
struct ManyPages(Option<u32>);

impl Respond for ManyPages {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let page: u32 = body["variables"]["after"].as_str().map_or(1, |cursor| {
            cursor.trim_start_matches('c').parse::<u32>().unwrap() + 1
        });
        let more = self.0.is_none_or(|pages| page < pages);
        ResponseTemplate::new(200).set_body_json(json!({"data": {"deck": {
            "name": "Long", "cardCount": page, "commanderColorIdentity": ["G"],
            "deckCards": {
                "pageInfo": {"endCursor": format!("c{page}"), "hasNextPage": more},
                "edges": [node("mainboard", &format!("Card {page}"), 1)]
            }
        }}}))
    }
}

/// The Elixir adapter stopped after four pages and returned a truncated list; the Rust
/// server follows lotus's default budget (ten pages) and keeps every card.
#[tokio::test]
async fn follows_manavault_decks_past_four_pages() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(ManyPages(Some(6)))
        .expect(6)
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    let deck = app
        .state
        .decklists
        .resolve(&format!("{origin}/share/decks/LongLongLongLongLongLong"))
        .await
        .unwrap();
    let names: Vec<&str> = deck.cards.iter().map(|card| card.name.as_str()).collect();
    assert_eq!(
        names,
        ["Card 1", "Card 2", "Card 3", "Card 4", "Card 5", "Card 6"]
    );
}

/// A deck that needs more than the page budget is an error, never a silently shortened list.
#[tokio::test]
async fn refuses_a_manavault_deck_over_the_page_budget() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(ManyPages(None))
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    let result = app
        .state
        .decklists
        .resolve(&format!(
            "{origin}/share/decks/EndlessEndlessEndlessEndless"
        ))
        .await;
    assert!(
        matches!(
            result,
            Err(the_gathering::decklists::DecklistError::UpstreamError)
        ),
        "{result:?}"
    );
    let requests = server.received_requests().await.unwrap().len();
    assert_eq!(requests, 10, "lotus's default budget is ten pages");
}

#[tokio::test]
async fn maps_missing_and_private_upstream_responses() {
    let server = MockServer::start().await;
    Mock::given(path("/v3/decks/all/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(path("/v3/decks/all/private"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    assert_eq!(
        app.state
            .decklists
            .resolve("https://moxfield.com/decks/missing")
            .await,
        Err(DecklistError::NotFound)
    );
    assert_eq!(
        app.state
            .decklists
            .resolve("https://moxfield.com/decks/private")
            .await,
        Err(DecklistError::Private)
    );
}

#[tokio::test]
async fn caches_successful_resolutions_but_not_failures() {
    let server = MockServer::start().await;
    Mock::given(path("/v3/decks/all/cache-me"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json_fixture("decklists/moxfield_partner.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v3/decks/all/flaky"))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    let url = "https://moxfield.com/decks/cache-me";
    let first = app.state.decklists.resolve(url).await.unwrap();
    let second = app.state.decklists.resolve(url).await.unwrap();
    assert_eq!(first, second);
    for _ in 0..2 {
        assert_eq!(
            app.state
                .decklists
                .resolve("https://moxfield.com/decks/flaky")
                .await,
            Err(DecklistError::UpstreamError)
        );
    }
}

// ---- destination_test.exs ----

#[test]
fn accepts_only_normalized_https_origins() {
    let deny = |_: &str| false;
    assert_eq!(
        destination::normalize_origin(" HTTPS://Vault.Example.COM:444/ ", &deny).as_deref(),
        Ok("https://vault.example.com:444")
    );
    for url in [
        "https://user:pass@vault.example.com",
        "https://vault.example.com/api",
        "https://vault.example.com?admin=1",
        "https://vault.example.com#fragment",
        "https://vault.example.com:99999",
        "https://vault.example.com:not-a-port",
        "http://vault.example.com",
    ] {
        assert!(destination::normalize_origin(url, &deny).is_err(), "{url}");
    }
}

#[tokio::test]
async fn profile_validation_rejects_credentials_paths_query_strings_and_fragments() {
    let app = TestApp::new().await;
    let user = app.member("member").await;
    for url in [
        "https://user@vault.example.com",
        "https://vault.example.com/api",
        "https://vault.example.com?q=1",
        "https://vault.example.com#fragment",
    ] {
        let attrs = json!({"display_name": "User", "manavault_url": url});
        let error = app
            .state
            .accounts
            .update_profile(&user, &attrs, |_| false)
            .await
            .unwrap_err();
        let the_gathering::error::ApiError::Validation(errors) = error else {
            panic!("{url}: {error:?}")
        };
        assert_eq!(
            errors.messages("manavault_url"),
            ["must be an allowed origin (scheme, host, and optional port only)"]
        );
    }
}

#[tokio::test]
async fn blocks_loopback_private_link_local_tailscale_and_ipv6_private_destinations() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "100.64.0.1",
        "[::1]",
        "[fc00::1]",
        "[fe80::1]",
        "[::ffff:127.0.0.1]",
    ] {
        let result = destination::resolve(
            &format!("https://{address}"),
            &Allowlist::new(),
            &SystemResolver,
        )
        .await;
        assert_eq!(result, Err(Blocked), "{address}");
    }
}

#[tokio::test]
async fn rejects_a_hostname_if_any_dns_answer_is_private_and_observes_changed_answers() {
    let resolver = QueuedResolver(Mutex::new(VecDeque::from([
        vec!["93.184.216.34".parse().unwrap()],
        vec!["127.0.0.1".parse().unwrap()],
        vec![
            "93.184.216.34".parse().unwrap(),
            "10.0.0.1".parse().unwrap(),
        ],
    ])));
    let (_, address) =
        destination::resolve("https://vault.example.com", &Allowlist::new(), &resolver)
            .await
            .unwrap();
    assert_eq!(address, "93.184.216.34".parse::<IpAddr>().unwrap());
    assert_eq!(
        destination::resolve("https://vault.example.com", &Allowlist::new(), &resolver).await,
        Err(Blocked)
    );
    assert_eq!(
        destination::resolve("https://vault.example.com", &Allowlist::new(), &resolver).await,
        Err(Blocked)
    );
}

#[tokio::test]
async fn operator_allowlist_permits_a_private_http_host() {
    let allowed = vec!["vault.internal".to_owned()];
    let insecure = |host: &str| destination::allowed_host(host, &allowed);
    assert_eq!(
        destination::normalize_origin("http://vault.internal:4000", &insecure).as_deref(),
        Ok("http://vault.internal:4000")
    );
    let resolver = StaticResolver::new(&[("vault.internal", "192.168.1.20")]);
    let allowlist = Allowlist::parse(allowed.iter().map(String::as_str));
    let (origin, address) =
        destination::resolve("http://vault.internal:4000", &allowlist, resolver.as_ref())
            .await
            .unwrap();
    assert_eq!(address, "192.168.1.20".parse::<IpAddr>().unwrap());
    assert_eq!(origin.port_or_default(), 4000);
}

// ---- decklist_controller_test.exs ----

async fn member(app: &TestApp) -> User {
    let user = app.member("member").await;
    app.log_in(&user).await;
    user
}

#[tokio::test]
async fn post_resolve_returns_the_decklist_envelope() {
    let server = MockServer::start().await;
    Mock::given(path("/v3/decks/all/api-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "API Deck",
            "createdByUser": {"userName": "brewer"},
            "boards": {
                "commanders": {"count": 1, "cards": {"one": {"quantity": 1, "card": {
                    "name": "Atraxa, Praetors' Voice", "color_identity": ["W", "U", "B", "G"]
                }}}},
                "mainboard": {"count": 99, "cards": {}}
            }
        })))
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    member(&app).await;
    let body = app
        .post(
            "/api/decklists/resolve",
            json!({"url": "https://moxfield.com/decks/api-test"}),
        )
        .await
        .assert_json(200);
    let data = &body["data"];
    assert_eq!(data["source"], "moxfield");
    assert_eq!(data["id"], "api-test");
    assert_eq!(data["url"], "https://moxfield.com/decks/api-test");
    assert_eq!(data["name"], "API Deck");
    assert_eq!(data["author"], "brewer");
    assert_eq!(data["card_count"], 100);
    assert_eq!(
        data["commanders"],
        json!([{"name": "Atraxa, Praetors' Voice"}])
    );
    assert_eq!(data["color_identity"], json!(["W", "U", "B", "G"]));
    assert!(data["fetched_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn returns_a_field_error_for_invalid_and_unsupported_urls() {
    let app = TestApp::new().await;
    member(&app).await;
    for body in [
        json!({"url": "not a URL"}),
        json!({"url": "https://example.com/a-deck"}),
        json!({}),
    ] {
        let response = app.post("/api/decklists/resolve", body).await;
        assert_eq!(
            response.assert_json(422),
            json!({"errors": {"url": ["is not a supported deck-list URL"]}})
        );
    }
}

#[tokio::test]
async fn maps_upstream_failures_to_bad_gateway() {
    let server = MockServer::start().await;
    Mock::given(path("/api/decks/123/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    member(&app).await;
    let response = app
        .post(
            "/api/decklists/resolve",
            json!({"url": "https://archidekt.com/decks/123"}),
        )
        .await;
    assert_eq!(
        response.assert_json(502),
        json!({"errors": {"detail": "Bad Gateway"}})
    );
}

async fn wilson_deck(app: &TestApp) -> (i64, i64) {
    let player = app.sql_player("Brewer").await;
    let deck = app
        .sql_deck(
            player,
            "Wilson",
            "Wilson, Refined Grizzly",
            json!({"decklist_url": "https://archidekt.com/decks/24907541/slug"}),
        )
        .await;
    (player, deck)
}

#[tokio::test]
async fn decklist_returns_the_playable_list_with_catalog_details_and_images() {
    let server = MockServer::start().await;
    mount_fixture(
        &server,
        "/api/decks/24907541/",
        "decklists/archidekt_background.json",
    )
    .await;
    let app = stub_app(&server, |_| {}).await;
    member(&app).await;
    let (_, deck) = wilson_deck(&app).await;
    app.catalog_card(json!({
        "id": "catalog-forest", "oracle_id": "oracle-forest", "name": "Forest", "type_line": "Basic Land — Forest",
        "image_uris": {
            "small": "https://cards.scryfall.io/small/front/1/2/12345678-1234-1234-1234-123456789abc.jpg",
            "normal": "https://cards.scryfall.io/normal/front/1/2/12345678-1234-1234-1234-123456789abc.jpg"
        }
    }))
    .await;
    app.catalog_card(json!({
        "id": "catalog-other", "oracle_id": "oracle-other", "name": "Other Cards", "type_line": "Creature — Bear",
        "mana_cost": "{1}{G}",
        "image_uris": {"small": "https://cards.scryfall.io/small/front/a/b/abcdef00-1234-1234-1234-123456789abc.jpg"}
    }))
    .await;

    let body = app
        .get(&format!("/api/decks/{deck}/decklist"))
        .await
        .assert_json(200);
    let data = &body["data"];
    assert_eq!(data["source"], "archidekt");
    assert_eq!(data["url"], "https://archidekt.com/decks/24907541");
    assert_eq!(data["name"], "I am a public servant");
    let cards = data["cards"].as_array().unwrap();
    let summary: Vec<(String, u64, String)> = cards
        .iter()
        .map(|card| {
            (
                card["name"].as_str().unwrap().to_owned(),
                card["quantity"].as_u64().unwrap(),
                card["zone"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Noble Heritage".to_owned(), 1, "commander".to_owned()),
            (
                "Wilson, Refined Grizzly".to_owned(),
                1,
                "commander".to_owned()
            ),
            ("Forest".to_owned(), 38, "mainboard".to_owned()),
            ("Other cards".to_owned(), 60, "mainboard".to_owned()),
        ]
    );
    let find = |name: &str| cards.iter().find(|card| card["name"] == name).unwrap();
    let forest = find("Forest");
    assert_eq!(forest["card_id"], "catalog-forest");
    assert_eq!(forest["type_line"], "Basic Land — Forest");
    assert_eq!(
        forest["printing_id"],
        "0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03"
    );
    // The list's printing wins over the catalog's image.
    let source =
        "https://cards.scryfall.io/normal/front/0/c/0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03.jpg";
    let expected: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("url", source)
        .finish();
    assert_eq!(
        forest["image_uris"]["normal"],
        format!("/api/card-images?{expected}")
    );
    // No printing on the list: fall back to the catalog card, matched case-insensitively.
    let other = find("Other cards");
    assert_eq!(other["card_id"], "catalog-other");
    assert_eq!(other["mana_cost"], "{1}{G}");
    assert!(
        other["image_uris"]["small"]
            .as_str()
            .unwrap()
            .contains("abcdef00-1234-1234-1234-123456789abc")
    );
    // Unknown to the catalog: listed with no details.
    let heritage = find("Noble Heritage");
    assert_eq!(heritage["card_id"], Value::Null);
    assert_eq!(heritage["type_line"], Value::Null);
    assert_eq!(heritage["game_changer"], false);
}

#[tokio::test]
async fn decklist_404s_for_decks_without_a_supported_link() {
    let app = TestApp::new().await;
    member(&app).await;
    let (player, _) = wilson_deck(&app).await;
    let unlinked = app.sql_deck(player, "Plain", "Plain", json!({})).await;
    let other = app
        .sql_deck(
            player,
            "Elsewhere",
            "Elsewhere",
            json!({"decklist_url": "https://example.com/my-deck"}),
        )
        .await;
    for deck in [unlinked, other, 0] {
        app.get(&format!("/api/decks/{deck}/decklist"))
            .await
            .assert_json(404);
    }
}

#[tokio::test]
async fn decklist_maps_private_lists_to_404_and_upstream_failures_to_502() {
    let server = MockServer::start().await;
    Mock::given(path("/api/decks/24907541/"))
        .respond_with(ResponseTemplate::new(403))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/decks/24907541/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    member(&app).await;
    let (_, deck) = wilson_deck(&app).await;
    app.get(&format!("/api/decks/{deck}/decklist"))
        .await
        .assert_json(404);
    app.get(&format!("/api/decks/{deck}/decklist"))
        .await
        .assert_json(502);
}

// ---- remote_deck_controller_test.exs (index) and cache_test.exs ----

async fn set_profile(app: &TestApp, user: &User, attrs: Value) -> User {
    let mut attrs = attrs;
    attrs["display_name"] = json!(user.display_name);
    app.state
        .accounts
        .update_profile(user, &attrs, |_| true)
        .await
        .unwrap()
}

fn source<'a>(body: &'a Value, name: &str) -> &'a Value {
    body["data"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source"] == name)
        .unwrap()
}

#[tokio::test]
async fn remote_decks_normalizes_configured_public_deck_sources() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/decks/search-sfw"))
        .and(query_param("authorUserNames", "mox-brewer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "pageNumber": 1, "totalPages": 1,
            "data": [{
                "publicId": "mox-id", "name": "Mox Deck", "publicUrl": "https://moxfield.com/decks/mox-id",
                "commanders": [{"card": {"name": "Muldrotha, the Gravetide"}}],
                "colorIdentity": ["U", "B", "G"], "lastUpdatedAtUtc": "2026-09-20T10:00:00Z"
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/decks/v3/"))
        .and(query_param("ownerUsername", "arch-brewer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "next": null,
            "results": [{"id": 42, "name": "Arch Deck", "updatedAt": "2026-09-19T10:00:00Z"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/decks/42/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"cards": [
            {"categories": ["Commander"], "card": {"oracleCard": {"name": "Wilson, Refined Grizzly", "colorIdentity": ["Green"]}}},
            {"categories": ["Commander"], "card": {"oracleCard": {"name": "Noble Heritage", "colorIdentity": ["White"]}}}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    let user = member(&app).await;
    set_profile(
        &app,
        &user,
        json!({"moxfield_username": "mox-brewer", "archidekt_username": "arch-brewer"}),
    )
    .await;

    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert_eq!(
        body["data"]["decks"],
        json!([
            {"source": "moxfield", "name": "Mox Deck", "commanders": ["Muldrotha, the Gravetide"],
             "color_identity": ["U", "B", "G"], "url": "https://moxfield.com/decks/mox-id",
             "updated_at": "2026-09-20T10:00:00Z"},
            {"source": "archidekt", "name": "Arch Deck", "commanders": ["Wilson, Refined Grizzly", "Noble Heritage"],
             "color_identity": ["W", "G"], "url": "https://archidekt.com/decks/42", "updated_at": "2026-09-19T10:00:00Z"}
        ])
    );
    assert_eq!(
        source(&body, "moxfield"),
        &json!({"source": "moxfield", "configured": true, "error": null})
    );
    assert_eq!(source(&body, "manavault")["configured"], false);
}

#[tokio::test]
async fn remote_decks_returns_clear_per_source_errors_without_failing_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_string("blocked"))
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    let user = member(&app).await;
    set_profile(
        &app,
        &user,
        json!({"moxfield_username": "blocked-user", "manavault_url": "https://vault.example.com"}),
    )
    .await;
    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert_eq!(body["data"]["decks"], json!([]));
    assert!(
        source(&body, "moxfield")["error"]
            .as_str()
            .unwrap()
            .contains("blocked")
    );
    assert!(
        source(&body, "manavault")["error"]
            .as_str()
            .unwrap()
            .contains("API key")
    );
}

/// A member's own ManaVault, served by `server` under a hostname the test resolver points
/// at the stub; the operator allowlists the host because it resolves to loopback.
async fn vault_app(server: &MockServer) -> (TestApp, User, String) {
    let app = stub_app(server, |config| {
        config.manavault_allowed_hosts = vec!["vault.example.com".to_owned()];
    })
    .await;
    app.state
        .decklists
        .set_resolver(StaticResolver::new(&[("vault.example.com", "127.0.0.1")]));
    let user = member(&app).await;
    let origin = format!("http://vault.example.com:{}", server.address().port());
    (app, user, origin)
}

#[tokio::test]
async fn remote_decks_lists_manavault_decks_with_the_users_api_key_across_pages() {
    let server = MockServer::start().await;
    let (app, user, origin) = vault_app(&server).await;
    let host = format!("vault.example.com:{}", server.address().port());
    for (page, body) in [
        (
            "1",
            json!({"data": [{
                "id": 42, "name": "Muldrotha Reanimator", "commanders": ["Muldrotha, the Gravetide"],
                "commanderColorIdentity": ["G", "B", "U"], "updated_at": "2026-09-20T14:32:10Z",
                "publicly_shared": true, "public_share_url": "https://vault.example.com/share/decks/AbCdEf123456"
            }], "pagination": {"page": 1, "per_page": 100, "total": 2, "total_pages": 2}}),
        ),
        (
            "2",
            json!({"data": [{
                "id": 7, "name": "Private brew",
                "commanders": ["Ardenn, Intrepid Archaeologist", "Kediss, Emberclaw Familiar"],
                "commanderColorIdentity": ["R", "W"], "updated_at": "2026-09-21T09:00:00Z",
                "publicly_shared": false, "public_share_url": null
            }], "pagination": {"page": 2, "per_page": 100, "total": 2, "total_pages": 2}}),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path("/api/v1/decks"))
            .and(query_param("page", page))
            .and(header("authorization", "Bearer mvk_test_key"))
            .and(header("host", host.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
    }
    set_profile(
        &app,
        &user,
        json!({"manavault_url": format!("{origin}/"), "manavault_api_key": "mvk_test_key"}),
    )
    .await;

    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    let decks = body["data"]["decks"].as_array().unwrap();
    assert_eq!(decks.len(), 2);
    assert_eq!(decks[0]["name"], "Private brew");
    assert_eq!(decks[0]["source"], "manavault");
    assert_eq!(
        decks[0]["commanders"],
        json!([
            "Ardenn, Intrepid Archaeologist",
            "Kediss, Emberclaw Familiar"
        ])
    );
    assert_eq!(decks[0]["color_identity"], json!(["W", "R"]));
    assert_eq!(decks[0]["url"], format!("{origin}/decks/7"));
    assert_eq!(decks[1]["name"], "Muldrotha Reanimator");
    assert_eq!(decks[1]["color_identity"], json!(["U", "B", "G"]));
    assert_eq!(
        decks[1]["url"],
        "https://vault.example.com/share/decks/AbCdEf123456"
    );
    assert_eq!(
        source(&body, "manavault"),
        &json!({"source": "manavault", "configured": true, "error": null})
    );
}

#[tokio::test]
async fn remote_decks_reports_a_rejected_manavault_api_key_without_failing_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"error": {"code": "unauthorized"}})),
        )
        .mount(&server)
        .await;
    let (app, user, origin) = vault_app(&server).await;
    set_profile(
        &app,
        &user,
        json!({"manavault_url": origin, "manavault_api_key": "mvk_revoked"}),
    )
    .await;
    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert_eq!(body["data"]["decks"], json!([]));
    assert!(
        source(&body, "manavault")["error"]
            .as_str()
            .unwrap()
            .contains("rejected the API key")
    );
}

#[tokio::test]
async fn remote_decks_blocks_a_private_manavault_host_that_is_not_allowlisted() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    app.state
        .decklists
        .set_resolver(StaticResolver::new(&[("vault.example.com", "127.0.0.1")]));
    let user = member(&app).await;
    let origin = format!("http://vault.example.com:{}", server.address().port());
    set_profile(
        &app,
        &user,
        json!({"manavault_url": origin, "manavault_api_key": "mvk_test_key"}),
    )
    .await;
    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert!(
        source(&body, "manavault")["error"]
            .as_str()
            .unwrap()
            .contains("blocked network address")
    );
}

#[tokio::test]
async fn remote_decks_caches_a_users_result() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"pageNumber": 1, "totalPages": 1, "data": []})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    let user = member(&app).await;
    set_profile(&app, &user, json!({"moxfield_username": "cached-user"})).await;
    for _ in 0..2 {
        assert_eq!(
            app.get("/api/session/remote-decks").await.assert_json(200)["data"]["decks"],
            json!([])
        );
    }
}

#[tokio::test]
async fn remote_decks_deduplicates_concurrent_cache_misses_for_one_user() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"pageNumber": 1, "totalPages": 1, "data": []}))
                .set_delay(Duration::from_millis(200)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    let user = app.member("member").await;
    let user = set_profile(&app, &user, json!({"moxfield_username": "concurrent-user"})).await;
    let (first, second) = tokio::join!(
        app.state.decklists.remote.list(&user),
        app.state.decklists.remote.list(&user)
    );
    assert_eq!(first, second);
}

#[tokio::test]
async fn remote_decks_does_not_follow_manavault_redirects() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", "http://127.0.0.1/admin")
                .set_body_string("redirect"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (app, user, origin) = vault_app(&server).await;
    set_profile(
        &app,
        &user,
        json!({"manavault_url": origin, "manavault_api_key": "mvk_redirect"}),
    )
    .await;
    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert!(
        source(&body, "manavault")["error"]
            .as_str()
            .unwrap()
            .contains("could not be reached")
    );
}

/// Always claims another page.
struct Endless;

impl Respond for Endless {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let page = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "pageNumber")
            .map(|(_, value)| value.into_owned())
            .unwrap();
        ResponseTemplate::new(200)
            .set_body_json(json!({"totalPages": 1000, "data": [{"id": format!("deck-{page}"), "name": format!("Deck {page}")}]}))
    }
}

#[tokio::test]
async fn remote_decks_truncates_an_endpoint_that_always_returns_another_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(Endless)
        .expect(2)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    app.state.decklists.remote.set_limits(Limits {
        max_pages: 2,
        ..Limits::default()
    });
    let user = member(&app).await;
    set_profile(&app, &user, json!({"moxfield_username": "endless"})).await;
    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert_eq!(body["data"]["decks"].as_array().unwrap().len(), 2);
    assert!(
        source(&body, "moxfield")["error"]
            .as_str()
            .unwrap()
            .contains("page limit")
    );
}

#[tokio::test]
async fn remote_decks_reports_response_byte_and_total_duration_budgets() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"totalPages": 1, "data": [], "padding": "x".repeat(200)})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let app = stub_app(&server, |_| {}).await;
    app.state.decklists.remote.set_limits(Limits {
        max_bytes: 100,
        ..Limits::default()
    });
    let user = member(&app).await;
    let user = set_profile(&app, &user, json!({"moxfield_username": "oversized"})).await;
    let body = app.get("/api/session/remote-decks").await.assert_json(200);
    assert!(
        source(&body, "moxfield")["error"]
            .as_str()
            .unwrap()
            .contains("response budget")
    );

    app.state.decklists.remote.clear_cache();
    app.state.decklists.remote.set_limits(Limits {
        duration: Duration::ZERO,
        ..Limits::default()
    });
    let result = app.state.decklists.remote.list(&user).await;
    let moxfield = result
        .sources
        .iter()
        .find(|source| source.source == Source::Moxfield)
        .unwrap();
    assert!(moxfield.error.as_deref().unwrap().contains("time budget"));
}

#[tokio::test]
async fn remote_cache_keys_and_values_do_not_contain_plaintext_api_keys() {
    let app = TestApp::new().await;
    let user = app.member("member").await;
    let secret = "sentinel-plain-api-key";
    let user = set_profile(&app, &user, json!({"manavault_api_key": secret})).await;
    assert_eq!(user.manavault_api_key.as_deref(), Some(secret));
    app.state.decklists.remote.list(&user).await;
    let cache = app.state.decklists.remote.cache();
    assert_eq!(cache.keys(), [user.id]);
    assert!(!format!("{:?}", cache.values()).contains(secret));
}

// ---- games/sync_remote_decks_test.exs and remote_deck_controller_test.exs (sync) ----

mod sync {
    use super::*;
    use the_gathering::games::{GamesError, sync_remote_decks};

    fn moxfield_deck(id: &str, name: &str, commanders: &[&str], colors: &[&str]) -> Value {
        json!({
            "publicId": id,
            "name": name,
            "publicUrl": format!("https://moxfield.com/decks/{id}"),
            "commanders": commanders.iter().map(|name| json!({"card": {"name": name}})).collect::<Vec<_>>(),
            "colorIdentity": colors,
            "lastUpdatedAtUtc": "2026-09-20T10:00:00Z"
        })
    }

    async fn stub_moxfield(server: &MockServer, decks: Vec<Value>) {
        Mock::given(method("GET"))
            .and(path("/v2/decks/search-sfw"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"pageNumber": 1, "totalPages": 1, "data": decks})),
            )
            .mount(server)
            .await;
    }

    struct Ctx {
        app: TestApp,
        user: User,
        player: the_gathering::games::Player,
    }

    async fn setup(server: &MockServer) -> Ctx {
        let app = stub_app(server, |_| {}).await;
        let user = app.unique_member().await;
        let player = app
            .player_with(json!({"name": "Brewer"}), Some(user.id))
            .await;
        let user = set_profile(&app, &user, json!({"moxfield_username": "brewer"})).await;
        Ctx { app, user, player }
    }

    async fn decks(ctx: &Ctx) -> Vec<the_gathering::games::Deck> {
        ctx.app
            .state
            .games
            .list_decks(false, Some(ctx.player.id))
            .await
            .unwrap()
            .into_iter()
            .map(|(deck, _)| deck)
            .collect()
    }

    async fn deck(ctx: &Ctx, id: i64) -> the_gathering::games::Deck {
        ctx.app.state.games.get_deck(id).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn links_a_same_commander_deck_without_renaming_it_and_creates_the_rest() {
        let server = MockServer::start().await;
        let ctx = setup(&server).await;
        ctx.app
            .card("krenko", "Krenko, Mob Boss", &["R"], json!({}), true)
            .await;
        let mine = ctx
            .app
            .deck(ctx.player.id, "Goblins!!", "krenko, mob boss")
            .await;
        stub_moxfield(
            &server,
            vec![
                moxfield_deck("a", "Krenko Storm", &["Krenko, Mob Boss"], &["R"]),
                moxfield_deck("b", "Krenko Budget", &["Krenko, Mob Boss"], &["R"]),
            ],
        )
        .await;

        let result = sync_remote_decks::run(&ctx.app.state, &ctx.user)
            .await
            .unwrap();
        assert_eq!((result.created, result.updated), (1, 1));
        assert!(result.errors.is_empty());

        let linked = deck(&ctx, mine.id).await;
        assert_eq!(linked.name, "Goblins!!");
        assert_eq!(
            linked.decklist_url.as_deref(),
            Some("https://moxfield.com/decks/a")
        );
        assert_eq!(
            linked
                .decklist_source
                .map(the_gathering::games::DecklistSource::as_str),
            Some("moxfield")
        );
        assert_eq!(linked.commander_card_id.as_deref(), Some("krenko"));
        assert_eq!(linked.color_identity, "R");

        // The second Krenko list must not steal the link; it becomes its own deck.
        let created: Vec<_> = decks(&ctx)
            .await
            .into_iter()
            .filter(|deck| deck.id != mine.id)
            .collect();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].name, "Krenko Budget");
        assert_eq!(
            created[0].decklist_url.as_deref(),
            Some("https://moxfield.com/decks/b")
        );
    }

    #[tokio::test]
    async fn matches_partner_pairs_in_either_order_and_never_repoints_a_linked_deck() {
        let server = MockServer::start().await;
        let ctx = setup(&server).await;
        let pair = ctx
            .app
            .deck_with(json!({
                "player_id": ctx.player.id,
                "name": "Tymna Thrasios",
                "commander_name": "Tymna the Weaver",
                "partner_name": "Thrasios, Triton Hero"
            }))
            .await;
        let elsewhere = ctx
            .app
            .deck_with(json!({
                "player_id": ctx.player.id,
                "name": "Meren",
                "commander_name": "Meren of Clan Nel Toth",
                "decklist_url": "https://archidekt.com/decks/9"
            }))
            .await;
        stub_moxfield(
            &server,
            vec![
                moxfield_deck(
                    "p",
                    "Blue Farm",
                    &["Thrasios, Triton Hero", "Tymna the Weaver"],
                    &["W", "U", "G"],
                ),
                moxfield_deck(
                    "m",
                    "Meren Reanimator",
                    &["Meren of Clan Nel Toth"],
                    &["B", "G"],
                ),
            ],
        )
        .await;

        let result = sync_remote_decks::run(&ctx.app.state, &ctx.user)
            .await
            .unwrap();
        assert_eq!((result.created, result.updated), (1, 1));
        assert_eq!(
            deck(&ctx, pair.id).await.decklist_url.as_deref(),
            Some("https://moxfield.com/decks/p")
        );
        assert_eq!(
            deck(&ctx, elsewhere.id).await.decklist_url.as_deref(),
            Some("https://archidekt.com/decks/9")
        );
        assert!(
            decks(&ctx)
                .await
                .iter()
                .any(|deck| deck.name == "Meren Reanimator"
                    && deck.decklist_url.as_deref() == Some("https://moxfield.com/decks/m"))
        );
    }

    #[tokio::test]
    async fn refreshes_a_deck_already_linked_by_url_from_the_host() {
        let server = MockServer::start().await;
        let ctx = setup(&server).await;
        let old = ctx
            .app
            .deck_with(json!({
                "player_id": ctx.player.id,
                "name": "Old name",
                "commander_name": "Old commander",
                "decklist_url": "https://moxfield.com/decks/a"
            }))
            .await;
        stub_moxfield(
            &server,
            vec![moxfield_deck(
                "a",
                "New name",
                &["Krenko, Mob Boss"],
                &["R"],
            )],
        )
        .await;
        let result = sync_remote_decks::run(&ctx.app.state, &ctx.user)
            .await
            .unwrap();
        assert_eq!((result.created, result.updated), (0, 1));
        let refreshed = deck(&ctx, old.id).await;
        assert_eq!(refreshed.name, "New name");
        assert_eq!(refreshed.commander_name, "Krenko, Mob Boss");
    }

    #[tokio::test]
    async fn reports_a_failed_host_and_syncs_the_others() {
        let server = MockServer::start().await;
        let ctx = setup(&server).await;
        let user = set_profile(&ctx.app, &ctx.user, json!({"archidekt_username": "brewer"})).await;
        stub_moxfield(
            &server,
            vec![moxfield_deck(
                "a",
                "Krenko Storm",
                &["Krenko, Mob Boss"],
                &["R"],
            )],
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/decks/v3/"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let result = sync_remote_decks::run(&ctx.app.state, &user).await.unwrap();
        assert_eq!((result.created, result.updated), (1, 0));
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].source, Source::Archidekt);
        assert!(!result.errors[0].error.is_empty());
    }

    #[tokio::test]
    async fn rejects_users_with_no_deck_host_configured_or_no_linked_player() {
        let app = TestApp::new().await;
        let bare = app.unique_member().await;
        assert!(matches!(
            sync_remote_decks::run(&app.state, &bare).await,
            Err(GamesError::BadRequest)
        ));
        app.player_with(json!({"name": "Unhosted"}), Some(bare.id))
            .await;
        let bare = app.reload(&bare).await.unwrap();
        assert!(matches!(
            sync_remote_decks::run(&app.state, &bare).await,
            Err(GamesError::BadRequest)
        ));
    }

    #[tokio::test]
    async fn post_sync_creates_and_updates_manavault_decks() {
        let server = MockServer::start().await;
        let (app, user, origin) = vault_app(&server).await;
        let player = app
            .player_with(json!({"name": "Chooser"}), Some(user.id))
            .await;
        app.card(
            "atraxa",
            "Atraxa, Praetors' Voice",
            &["W", "U", "B", "G"],
            json!({}),
            true,
        )
        .await;
        app.card("krenko", "Krenko, Mob Boss", &["R"], json!({}), true)
            .await;
        let existing = app
            .deck_with(json!({
                "player_id": player.id,
                "name": "Old name",
                "commander_name": "Old commander",
                "decklist_url": format!("{origin}/decks/1")
            }))
            .await;
        set_profile(
            &app,
            &user,
            json!({"manavault_url": origin, "manavault_api_key": "mvk_test_key"}),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/decks"))
            .and(header("authorization", "Bearer mvk_test_key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"id": 1, "name": "Atraxa counters", "commanders": ["Atraxa, Praetors' Voice"],
                     "commanderColorIdentity": ["W", "U", "B", "G"], "updated_at": "2026-09-20T12:00:00Z"},
                    {"id": 2, "name": "Goblin rush", "commanders": ["Krenko, Mob Boss"],
                     "commanderColorIdentity": ["R"], "updated_at": "2026-09-20T12:00:00Z"}
                ],
                "pagination": {"total_pages": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let body = app
            .post("/api/session/remote-decks/sync", json!({}))
            .await
            .assert_json(200);
        assert_eq!(
            body,
            json!({"data": {"created": 1, "updated": 1, "errors": []}})
        );
        let updated = app
            .state
            .games
            .get_deck(existing.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Atraxa counters");
        assert_eq!(updated.commander_card_id.as_deref(), Some("atraxa"));
        assert_eq!(updated.color_identity, "WUBG");
        let created: Vec<_> = app
            .state
            .games
            .list_decks(false, Some(player.id))
            .await
            .unwrap()
            .into_iter()
            .map(|(deck, _)| deck)
            .filter(|deck| deck.id != existing.id)
            .collect();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].name, "Goblin rush");
        assert_eq!(created[0].commander_card_id.as_deref(), Some("krenko"));
        assert_eq!(
            created[0].decklist_url.as_deref(),
            Some(format!("{origin}/decks/2").as_str())
        );
    }

    #[tokio::test]
    async fn post_sync_without_a_deck_host_is_a_bad_request() {
        let app = TestApp::new().await;
        member(&app).await;
        app.post("/api/session/remote-decks/sync", json!({}))
            .await
            .assert_json(400);
    }
}

// ---- ManaVault servers older than the share query (lotus `FetchError::ServerTooOld`) ----

/// A pre-1.3.0 ManaVault rejects the share query's `commanderColorIdentity` field.
fn too_old_manavault() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "data": null,
        "errors": [{"message": "Cannot query field \"commanderColorIdentity\" on type \"Deck\"."}]
    }))
}

#[test]
fn server_too_old_message_names_lotus_minimum_version() {
    assert!(the_gathering::decklists::SERVER_TOO_OLD.contains(&format!(
        "v{}",
        lotus::decklist::manavault::MIN_SERVER_VERSION
    )));
}

#[tokio::test]
async fn resolve_reports_a_manavault_server_too_old_as_a_url_error_not_a_502() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(too_old_manavault())
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    member(&app).await;
    let url = format!("{origin}/share/decks/AbCdEfGhIjKlMnOpQrStUvWx");
    assert!(matches!(
        app.state.decklists.resolve(&url).await,
        Err(the_gathering::decklists::DecklistError::ServerTooOld)
    ));
    let response = app
        .post("/api/decklists/resolve", json!({"url": url}))
        .await;
    assert_eq!(
        response.assert_json(422),
        json!({"errors": {"url": [
            "This ManaVault server is too old to share deck lists; it needs v1.3.0 or newer."
        ]}})
    );
}

#[tokio::test]
async fn decklist_reports_a_manavault_server_too_old_with_its_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(too_old_manavault())
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    member(&app).await;
    let player = app.sql_player("Brewer").await;
    let deck = app
        .sql_deck(
            player,
            "Shorikai",
            "Shorikai, Genesis Engine",
            json!({"decklist_url": format!("{origin}/share/decks/AbCdEfGhIjKlMnOpQrStUvWx")}),
        )
        .await;
    let body = app
        .get(&format!("/api/decks/{deck}/decklist"))
        .await
        .assert_json(422);
    assert_eq!(
        body,
        json!({"errors": {"detail":
            "This ManaVault server is too old to share deck lists; it needs v1.3.0 or newer."
        }})
    );
}

/// GraphQL errors unrelated to the versioned fields stay generic upstream failures.
#[tokio::test]
async fn unrelated_manavault_graphql_errors_stay_bad_gateway() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": null, "errors": [{"message": "rate limited"}]
        })))
        .mount(&server)
        .await;
    let (app, origin) = manavault_app(&server).await;
    member(&app).await;
    let url = format!("{origin}/share/decks/AbCdEfGhIjKlMnOpQrStUvWx");
    app.post("/api/decklists/resolve", json!({"url": url}))
        .await
        .assert_json(502);
}
