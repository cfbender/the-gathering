//! Rate limits on credentials, sudo, and API keys.
//!
//! Requests go through `oneshot` without `ConnectInfo`, so the peer address is 127.0.0.1;
//! clients are told apart with `x-forwarded-for` and `trust_proxy_headers`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use std::net::SocketAddr;
use std::time::Duration;

use axum::http::{HeaderMap, Method};
use serde_json::{Value, json};
use support::TestApp;
use the_gathering::config::{Config, WindowLimit};
use the_gathering::web::api::client_ip;

fn limit(limit: u64) -> WindowLimit {
    WindowLimit {
        limit,
        scale: Duration::from_mins(5),
    }
}

async fn app(adjust: impl FnOnce(&mut Config)) -> TestApp {
    TestApp::with_config(|config| {
        config.rate_limits.trust_proxy_headers = true;
        adjust(config);
    })
    .await
}

async fn from(app: &TestApp, ip: &str, path: &str, body: Value) -> support::TestResponse {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", ip.parse().unwrap());
    app.request_with(Method::POST, path, Some(body), headers)
        .await
}

fn assert_too_many(response: &support::TestResponse) {
    assert_eq!(
        response.assert_json(429),
        json!({"errors": {"detail": "Too Many Requests"}})
    );
    let retry_after: u64 = response.header("retry-after").unwrap().parse().unwrap();
    assert!((1..=300).contains(&retry_after), "{retry_after}");
}

#[tokio::test]
async fn credential_endpoints_share_a_per_address_bucket_and_answer_429_when_exhausted() {
    let app = app(|config| config.rate_limits.credentials = limit(3)).await;
    let attacker = "10.200.0.1";
    let neighbour = "10.200.0.2";

    for _ in 0..3 {
        from(
            &app,
            attacker,
            "/api/session",
            json!({"username": "nobody", "password": "wrong"}),
        )
        .await
        .assert_json(401);
    }
    let denied = from(
        &app,
        attacker,
        "/api/session",
        json!({"username": "a", "password": "b"}),
    )
    .await;
    assert_too_many(&denied);

    // Bootstrap registration draws from the same bucket, so an exhausted address cannot
    // switch endpoints to keep guessing.
    from(
        &app,
        attacker,
        "/api/users",
        json!({"username": "x", "password": "y"}),
    )
    .await
    .assert_json(429);

    // Other clients are unaffected.
    from(
        &app,
        neighbour,
        "/api/session",
        json!({"username": "nobody", "password": "wrong"}),
    )
    .await
    .assert_json(401);
}

#[tokio::test]
async fn requests_under_the_limit_pass_through_untouched() {
    let app = app(|config| config.rate_limits.credentials = limit(2)).await;
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "10.200.1.1".parse().unwrap());
    let response = app
        .request_with(Method::GET, "/api/health", None, headers)
        .await;
    assert_eq!(response.status.as_u16(), 200);
}

#[tokio::test]
async fn sudo_has_an_account_and_client_bucket_and_returns_retry_after_independently() {
    let app = app(|config| {
        config.rate_limits.sudo = limit(3);
        config.rate_limits.sudo_global = 100;
    })
    .await;
    let admin = app.unique_admin().await;
    let attacker = "10.201.0.1";

    for _ in 0..3 {
        app.log_in(&admin).await;
        from(
            &app,
            attacker,
            "/api/session/sudo",
            json!({"password": "wrong-password"}),
        )
        .await
        .assert_json(401);
    }
    app.log_in(&admin).await;
    let denied = from(
        &app,
        attacker,
        "/api/session/sudo",
        json!({"password": "wrong-password"}),
    )
    .await;
    assert_too_many(&denied);

    // The ordinary login bucket and another client remain available.
    app.clear_cookies();
    from(
        &app,
        attacker,
        "/api/session",
        json!({"username": "missing", "password": "wrong-password"}),
    )
    .await
    .assert_json(401);

    app.log_in(&admin).await;
    from(
        &app,
        "10.201.0.2",
        "/api/session/sudo",
        json!({"password": "wrong-password"}),
    )
    .await
    .assert_json(401);
}

#[tokio::test]
async fn sudo_has_a_global_budget_across_clients() {
    let app = app(|config| {
        config.rate_limits.sudo = limit(10);
        config.rate_limits.sudo_global = 2;
    })
    .await;
    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    for ip in ["10.202.0.1", "10.202.0.2"] {
        from(
            &app,
            ip,
            "/api/session/sudo",
            json!({"password": "wrong-password"}),
        )
        .await
        .assert_json(401);
    }
    assert_too_many(
        &from(
            &app,
            "10.202.0.3",
            "/api/session/sudo",
            json!({"password": "wrong-password"}),
        )
        .await,
    );
}

#[tokio::test]
async fn client_ip_uses_the_socket_address_unless_proxy_headers_are_trusted() {
    let app = TestApp::new().await;
    let peer: SocketAddr = "192.168.0.9:5000".parse().unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    assert_eq!(client_ip(&app.state, &headers, Some(peer)), "192.168.0.9");
}

#[tokio::test]
async fn client_ip_prefers_x_real_ip_then_the_last_x_forwarded_for_hop_when_trusted() {
    let app = app(|_| {}).await;
    let peer: SocketAddr = "192.168.0.9:5000".parse().unwrap();

    let mut forwarded = HeaderMap::new();
    forwarded.insert("x-forwarded-for", "1.2.3.4, 203.0.113.5".parse().unwrap());
    assert_eq!(client_ip(&app.state, &forwarded, Some(peer)), "203.0.113.5");

    let mut real = forwarded.clone();
    real.insert("x-real-ip", "2001:db8::7".parse().unwrap());
    assert_eq!(client_ip(&app.state, &real, Some(peer)), "2001:db8::7");

    let mut garbage = HeaderMap::new();
    garbage.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
    assert_eq!(client_ip(&app.state, &garbage, Some(peer)), "192.168.0.9");

    assert_eq!(
        client_ip(&app.state, &HeaderMap::new(), Some(peer)),
        "192.168.0.9"
    );
    // Without a socket address (`oneshot` in tests) the peer is the loopback address.
    assert_eq!(client_ip(&app.state, &HeaderMap::new(), None), "127.0.0.1");
}
