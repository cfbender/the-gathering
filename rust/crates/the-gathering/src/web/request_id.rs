//! `Plug.RequestId` and Phoenix's request logging (`Plug.Telemetry` + `Phoenix.Logger`).
//!
//! Every routed request gets an `x-request-id` response header (the client's own when it is
//! 20 to 200 bytes long, otherwise a fresh one) and runs inside a span carrying it. The log
//! shows the method and path (never the query string) and the response status and time;
//! parameters are logged at debug level by [`super::params::Params`], filtered.

use std::time::Instant;

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;

use crate::crypto;

/// The header Plug reads and writes.
pub const HEADER: &str = "x-request-id";

fn request_id(request: &Request) -> HeaderValue {
    request
        .headers()
        .get(HEADER)
        .filter(|value| (20..=200).contains(&value.len()))
        .cloned()
        .or_else(|| {
            HeaderValue::from_str(&crypto::url_encode64(&crypto::random_bytes::<15>())).ok()
        })
        .unwrap_or_else(|| HeaderValue::from_static("unknown-request-id--"))
}

/// Tags the request with an id and logs it.
pub async fn layer(request: Request, next: Next) -> Response {
    let id = request_id(&request);
    let span = tracing::info_span!("request", request_id = id.to_str().unwrap_or_default());
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    async move {
        let started = Instant::now();
        tracing::info!("{method} {path}");
        let mut response = next.run(request).await;
        tracing::info!(
            "Sent {} in {}ms",
            response.status().as_u16(),
            started.elapsed().as_millis()
        );
        response.headers_mut().insert(HEADER, id);
        response
    }
    .instrument(span)
    .await
}
