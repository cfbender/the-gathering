//! The SPA shell, JSON 404s and errors, the health check, parameter filtering in logs, and
//! the endpoint and router behavior around them (request ids, HEAD, CSRF, body parsing,
//! secure browser headers).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::{TestApp, capture_logs};
use the_gathering::changeset::Changeset;
use the_gathering::error::ApiError;
use the_gathering::web::params::filter_values;

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(*name, value.parse().unwrap());
    }
    map
}

async fn render(error: ApiError) -> (StatusCode, Value) {
    let response = error.into_response();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

// -- app controller --------------------------------------------------------------

#[tokio::test]
async fn get_root_serves_the_spa_shell_with_a_csrf_token_and_the_react_entrypoint() {
    let app = TestApp::new().await;
    let response = app.get("/").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    let html = response.text();
    assert!(html.contains(r#"<div id="root"></div>"#));
    let csrf = regex::Regex::new(r#"<meta name="csrf-token" content="[^"]+""#).unwrap();
    assert!(csrf.is_match(&html));
    assert!(html.contains("assets/react/src/main.tsx"));
    assert_eq!(
        response.header("cache-control"),
        Some("no-cache, no-store, must-revalidate")
    );
}

#[tokio::test]
async fn signed_in_users_get_their_saved_appearance_on_html_for_the_first_paint() {
    let app = TestApp::new().await;
    assert!(app.get("/").await.text().contains(r#"<html lang="en">"#));

    let user = app.unique_member().await;
    let user = app
        .state
        .accounts
        .update_appearance(
            &user,
            &support::input(json!({"palette": "kanagawa", "theme_style": "classic"})),
        )
        .await
        .unwrap();
    app.log_in(&user).await;
    assert!(
        app.get("/")
            .await
            .text()
            .contains(r#"<html lang="en" data-palette="kanagawa" data-theme-style="classic">"#)
    );
}

#[tokio::test]
async fn client_side_routes_fall_through_to_the_spa_shell() {
    let app = TestApp::new().await;
    let response = app.get("/games/123/anything").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.text().contains(r#"<div id="root"></div>"#));
}

#[tokio::test]
async fn only_webcam_table_documents_are_cross_origin_isolated() {
    let app = TestApp::new().await;
    for path in [
        format!("/table/{}", uuid::Uuid::new_v4()),
        "/table".to_owned(),
    ] {
        let table = app.get(&path).await;
        assert!(table.text().contains(r#"<div id="root"></div>"#));
        assert_eq!(
            table.header("cross-origin-opener-policy"),
            Some("same-origin")
        );
        assert_eq!(
            table.header("cross-origin-embedder-policy"),
            Some("require-corp")
        );
    }

    for path in ["/", "/games"] {
        let response = app.get(path).await;
        assert!(response.text().contains(r#"<div id="root"></div>"#));
        assert_eq!(response.header("cross-origin-opener-policy"), None);
        assert_eq!(response.header("cross-origin-embedder-policy"), None);
    }
}

#[tokio::test]
async fn requests_proxied_by_the_vite_dev_server_get_relative_script_urls() {
    let app = TestApp::new().await;
    assert!(
        app.get("/")
            .await
            .text()
            .contains(r#"src="http://127.0.0.1:5173/@vite/client""#)
    );

    let proxied = app
        .request_with(
            Method::GET,
            "/",
            None,
            headers(&[("x-the-gathering-vite-proxy", "1")]),
        )
        .await
        .text();
    assert!(proxied.contains(r#"src="/@vite/client""#));
    assert!(!proxied.contains("127.0.0.1:5173"));
}

#[tokio::test]
async fn unknown_api_routes_return_json_404_rather_than_the_shell() {
    let app = TestApp::new().await;
    for path in ["/api/does-not-exist", "/api", "/api/games/1/nope"] {
        assert_eq!(
            app.get(path).await.assert_json(404),
            json!({"errors": {"detail": "Not Found"}}),
            "{path}"
        );
    }
    // Known paths with another method fall through to the same JSON 404.
    assert_eq!(
        app.put("/api/health", json!({})).await.assert_json(404),
        json!({"errors": {"detail": "Not Found"}})
    );
    assert_eq!(
        app.delete("/api/registration").await.assert_json(404),
        json!({"errors": {"detail": "Not Found"}})
    );
}

#[tokio::test]
async fn the_browser_pipeline_sends_phoenix_secure_headers() {
    let app = TestApp::new().await;
    for path in ["/", "/games", "/table/abc", "/auth/discord"] {
        let response = app.get(path).await;
        assert_eq!(
            response.header("content-security-policy"),
            Some("base-uri 'self'; frame-ancestors 'self';"),
            "{path}"
        );
        assert_eq!(
            response.header("referrer-policy"),
            Some("strict-origin-when-cross-origin")
        );
        assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(
            response.header("x-permitted-cross-domain-policies"),
            Some("none")
        );
    }
    // The JSON API pipeline does not add them.
    let api = app.get("/api/health").await;
    assert_eq!(api.header("content-security-policy"), None);
}

#[tokio::test]
async fn head_requests_are_answered_like_gets_without_a_body() {
    let app = TestApp::new().await;
    for path in ["/", "/api/health"] {
        let response = app.request(Method::HEAD, path, None).await;
        assert_eq!(response.status, StatusCode::OK, "{path}");
        assert!(response.body.is_empty());
        assert!(response.header("x-request-id").is_some());
    }
}

// -- Plug.RequestId -----------------------------------------------------------------------

#[tokio::test]
async fn every_routed_response_carries_a_request_id() {
    let app = TestApp::new().await;
    let generated = app.get("/api/health").await;
    let id = generated.header("x-request-id").unwrap();
    assert_eq!(id.len(), 20);
    assert_ne!(
        app.get("/api/health").await.header("x-request-id"),
        Some(id)
    );
    assert!(app.get("/").await.header("x-request-id").is_some());
    assert!(
        app.get("/api/nothing-here")
            .await
            .header("x-request-id")
            .is_some()
    );

    // A client id of 20 to 200 bytes is kept; others are replaced.
    let own = "client-request-id-0123456789";
    let kept = app
        .request_with(
            Method::GET,
            "/api/health",
            None,
            headers(&[("x-request-id", own)]),
        )
        .await;
    assert_eq!(kept.header("x-request-id"), Some(own));
    let replaced = app
        .request_with(
            Method::GET,
            "/api/health",
            None,
            headers(&[("x-request-id", "short")]),
        )
        .await;
    assert_ne!(replaced.header("x-request-id"), Some("short"));
}

// -- fallback controller ---------------------------------------------------------

#[tokio::test]
async fn renders_changeset_errors_per_field_with_interpolated_placeholders() {
    let params = json!({"name": "", "seats": 1});
    let mut cs = Changeset::new(&params);
    let name = cs.string("name").or(None);
    let seats = cs.integer("seats").or(None);
    cs.required("name", name.as_ref());
    cs.at_least("seats", seats, 2);
    let errors = cs.finish().unwrap_err();

    assert_eq!(
        render(errors.into()).await,
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"errors": {
                "name": ["can't be blank"],
                "seats": ["must be greater than or equal to 2"]
            }})
        )
    );
}

#[tokio::test]
async fn maps_error_atoms_to_their_status_and_reason_phrase() {
    for (error, code, phrase) in [
        (ApiError::BadRequest, 400, "Bad Request"),
        (ApiError::Unauthorized, 401, "Unauthorized"),
        (ApiError::Forbidden, 403, "Forbidden"),
        (ApiError::NotFound, 404, "Not Found"),
        (ApiError::Conflict, 409, "Conflict"),
        (ApiError::BadGateway, 502, "Bad Gateway"),
    ] {
        let (status, body) = render(error).await;
        assert_eq!(status.as_u16(), code);
        assert_eq!(body, json!({"errors": {"detail": phrase}}));
    }
}

#[tokio::test]
async fn api_mutations_without_a_valid_csrf_token_are_rejected() {
    let app = TestApp::new().await;
    app.get("/").await; // a session with a CSRF token
    for token in [None, Some("not-the-token")] {
        let mut extra = HeaderMap::new();
        // The harness adds a valid token unless the request already carries one.
        extra.insert("x-csrf-token", token.unwrap_or("").parse().unwrap());
        for path in ["/api/anything", "/api/session"] {
            let response = app
                .request_with(Method::POST, path, Some(json!({})), extra.clone())
                .await;
            assert_eq!(
                response.assert_json(403),
                json!({"errors": {"detail": "Forbidden"}}),
                "{path} with {token:?}"
            );
        }
    }

    // Without any session at all.
    app.clear_cookies();
    let response = app
        .request_with(
            Method::DELETE,
            "/api/session",
            None,
            headers(&[("x-csrf-token", "")]),
        )
        .await;
    assert_eq!(
        response.assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
}

#[tokio::test]
async fn malformed_json_bodies_and_non_integer_ids_are_bad_requests() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;

    let token = app.csrf_token();
    let cookie = app.cookie_value().unwrap();
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/players")
        .header("content-type", "application/json")
        .header("x-csrf-token", token)
        .header(
            "cookie",
            format!("{}={cookie}", the_gathering::web::session::COOKIE),
        )
        .body(axum::body::Body::from("{not json"))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.router.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"errors": {"detail": "Bad Request"}})
    );

    assert_eq!(
        app.get("/api/players/not-a-number").await.assert_json(400),
        json!({"errors": {"detail": "Bad Request"}})
    );
}

#[tokio::test]
async fn json_endpoints_refuse_other_content_types() {
    let app = TestApp::new().await;
    app.admin("owner").await;
    let token = app.csrf_token();
    let cookie = app.cookie_value().unwrap();
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/session")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("x-csrf-token", token)
        .header(
            "cookie",
            format!("{}={cookie}", the_gathering::web::session::COOKIE),
        )
        .body(axum::body::Body::from(format!(
            "username=owner&password={}",
            support::PASSWORD
        )))
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.router.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"errors": {"detail": "Unsupported Media Type"}})
    );
}

#[tokio::test]
async fn oversized_bodies_and_bad_path_ids_are_json_errors() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    let huge = "x".repeat(the_gathering::web::BODY_LIMIT + 1);
    assert_eq!(
        app.post("/api/session/api-keys", json!({ "name": huge }))
            .await
            .assert_json(413),
        json!({"errors": {"detail": "Payload Too Large"}})
    );
    assert_eq!(
        app.delete("/api/session/api-keys/not-a-number")
            .await
            .assert_json(400),
        json!({"errors": {"detail": "Bad Request"}})
    );
}

// -- health controller -----------------------------------------------------------

#[tokio::test]
async fn get_api_health_reports_the_database_as_reachable() {
    let app = TestApp::new().await;
    assert_eq!(
        app.get("/api/health").await.assert_json(200),
        json!({"status": "ok"})
    );
}

// -- error json ------------------------------------------------------------------

#[tokio::test]
async fn renders_404_and_500() {
    assert_eq!(
        render(ApiError::NotFound).await.1,
        json!({"errors": {"detail": "Not Found"}})
    );
    let (status, body) = render(ApiError::Internal(anyhow::anyhow!("secret cause"))).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"errors": {"detail": "Internal Server Error"}}));
}

// -- parameter filter ------------------------------------------------------------

#[test]
fn filter_values_redacts_manavault_and_oauth_parameters() {
    let secrets = json!({
        "manavault_api_key": "sentinel-manavault-key",
        "code": "sentinel-oauth-code",
        "state": "sentinel-oauth-state"
    });
    let logged = format!("params={}", filter_values(&secrets));
    assert!(!logged.contains("sentinel-manavault-key"));
    assert!(!logged.contains("sentinel-oauth-code"));
    assert!(!logged.contains("sentinel-oauth-state"));
    assert!(logged.contains("[FILTERED]"));
}

#[tokio::test]
async fn request_logging_never_shows_sensitive_parameters() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;

    let (guard, logs) = capture_logs();
    app.patch(
        "/api/session/user",
        json!({"manavault_api_key": "sentinel-manavault-key", "display_name": "Visible"}),
    )
    .await
    .assert_json(200);
    app.post(
        "/api/session",
        json!({"username": "nobody", "password": "sentinel-password"}),
    )
    .await
    .assert_json(401);
    app.get("/auth/discord/callback?code=sentinel-oauth-code&state=sentinel-oauth-state")
        .await;
    app.get("/api/cards?q=visible-query&token=sentinel-token")
        .await;
    drop(guard);

    let logs = logs.contents();
    for secret in [
        "sentinel-manavault-key",
        "sentinel-password",
        "sentinel-oauth-code",
        "sentinel-oauth-state",
        "sentinel-token",
    ] {
        assert!(!logs.contains(secret), "{secret} logged:\n{logs}");
    }
    assert!(logs.contains("[FILTERED]"), "{logs}");
    assert!(logs.contains("PATCH /api/session/user"), "{logs}");
    assert!(logs.contains("Sent 200 in"), "{logs}");
}

// -- UserAuth -----------------------------------------------------------------------------

#[tokio::test]
async fn week_old_session_tokens_are_reissued() {
    let app = TestApp::new().await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    let old = app.session().user_token.unwrap();
    sqlx::query("UPDATE users_tokens SET inserted_at = ? WHERE token = ?")
        .bind(the_gathering::db::UtcDateTime::now().plus(time::Duration::days(-7)))
        .bind(&old)
        .execute(app.pool())
        .await
        .unwrap();

    app.get("/api/session").await.assert_json(200);
    let new = app.session().user_token.unwrap();
    assert_ne!(new, old);
    let accounts = &app.state.accounts;
    assert!(
        accounts
            .get_user_by_session_token(&old)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        accounts
            .get_user_by_session_token(&new)
            .await
            .unwrap()
            .is_some()
    );
}
