//! Request ids and request logging.
//!
//! Every routed request gets an `x-request-id`: the client's own when it is 20 to 200 bytes
//! long, otherwise a fresh one. The response echoes it, and [`trace_layer`] logs each
//! request in a span carrying the method, path (never the query string), and id, with the
//! status and latency when the response is ready. Bodies and parameters are never logged.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::Level;

use crate::crypto;

/// The request id header.
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

/// Gives the request its id and echoes it on the response.
pub async fn layer(mut request: Request, next: Next) -> Response {
    let id = request_id(&request);
    request.headers_mut().insert(HEADER, id.clone());
    let mut response = next.run(request).await;
    response.headers_mut().insert(HEADER, id);
    response
}

/// The span each request is logged in.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestSpan;

impl<B> MakeSpan<B> for RequestSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        let id = request
            .headers()
            .get(HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        tracing::info_span!(
            "request",
            method = %request.method(),
            path = request.uri().path(),
            request_id = id,
        )
    }
}

/// Logs every request at info level with its status and latency.
pub fn trace_layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>, RequestSpan> {
    TraceLayer::new_for_http()
        .make_span_with(RequestSpan)
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}
