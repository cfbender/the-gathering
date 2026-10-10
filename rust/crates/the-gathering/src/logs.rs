//! Live server logs: a `tracing` layer that copies each event into a bounded broadcast hub
//! administrators can follow (`GET /api/admin/server-logs`).
//!
//! The hub keeps nothing: entries reach only the subscribers listening when they are logged,
//! and a subscriber that falls more than the channel's capacity behind loses the oldest
//! entries (the stream reports how many). The layer sits behind the same `LOG_LEVEL`/
//! `RUST_LOG` filter as stdout, so it sees exactly what the console sees.
//!
//! Every entry is cleaned before it is shared: ANSI escape sequences and control characters
//! (other than newlines and tabs) are removed, the message is capped at
//! [`MAX_MESSAGE_BYTES`], and structured fields whose names look like credentials
//! (`password`, `token`, `cookie`, …) are left out. Message text itself is not redacted, so
//! code must keep secrets out of log messages as it already does for stdout.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value, json};
use tokio::sync::{broadcast, watch};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use crate::db::UtcDateTime;

/// Entries a subscriber may fall behind before it starts losing the oldest.
pub const DEFAULT_CAPACITY: usize = 1024;
/// Largest message shared, in UTF-8 bytes (including the trailing ellipsis when cut).
pub const MAX_MESSAGE_BYTES: usize = 8000;
/// Largest structured field value shared, in UTF-8 bytes.
pub const MAX_FIELD_BYTES: usize = 1000;
/// Most structured fields shared per entry.
pub const MAX_FIELDS: usize = 32;

/// Field names containing any of these are never shared.
const SECRET_FIELD_PARTS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "cookie",
    "authorization",
    "api_key",
    "apikey",
    "credential",
    "private_key",
    "session",
];

/// The request a log line was written for, from the request span.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestContext {
    /// HTTP method.
    pub method: String,
    /// Path, without the query string.
    pub path: String,
    /// The `x-request-id`.
    pub request_id: String,
}

/// One cleaned log line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Increases by one per entry the hub publishes.
    pub id: u64,
    /// When it was logged.
    pub timestamp: UtcDateTime,
    /// `debug`, `info`, `warning`, or `error` (the `LOG_LEVEL` names).
    pub level: &'static str,
    /// The module that logged it.
    pub target: String,
    /// The message.
    pub message: String,
    /// Other structured fields, in logged order.
    pub fields: Vec<(String, String)>,
    /// The request being served, if any.
    pub request: Option<RequestContext>,
}

impl LogEntry {
    /// The JSON the stream sends.
    pub fn to_json(&self) -> Value {
        let fields: Map<String, Value> = self
            .fields
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect();
        json!({
            "id": self.id,
            "timestamp": self.timestamp,
            "level": self.level,
            "target": self.target,
            "message": self.message,
            "fields": fields,
            "request": self.request.as_ref().map(|request| json!({
                "method": request.method,
                "path": request.path,
                "request_id": request.request_id,
            })),
        })
    }
}

/// A raw event, before the hub cleans and numbers it.
#[derive(Clone, Debug)]
pub struct LogRecord {
    /// The event's level.
    pub level: Level,
    /// The module that logged it.
    pub target: String,
    /// The message.
    pub message: String,
    /// Other fields, in logged order.
    pub fields: Vec<(String, String)>,
    /// The request being served, if any.
    pub request: Option<RequestContext>,
}

/// Fans log entries out to live subscribers. Cheap to clone.
#[derive(Clone, Debug)]
pub struct LogHub(Arc<HubInner>);

#[derive(Debug)]
struct HubInner {
    sender: broadcast::Sender<Arc<LogEntry>>,
    next_id: AtomicU64,
    shutdown: watch::Sender<bool>,
}

impl Default for LogHub {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl LogHub {
    /// A hub whose subscribers may fall `capacity` entries behind.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        let (shutdown, _) = watch::channel(false);
        Self(Arc::new(HubInner {
            sender,
            next_id: AtomicU64::new(1),
            shutdown,
        }))
    }

    /// The `tracing` layer that publishes into this hub.
    pub fn layer(&self) -> LogLayer {
        LogLayer { hub: self.clone() }
    }

    /// Receives entries published from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<LogEntry>> {
        self.0.sender.subscribe()
    }

    /// Whether anyone is listening (events are not even formatted otherwise).
    pub fn has_subscribers(&self) -> bool {
        self.0.sender.receiver_count() > 0
    }

    /// Cleans, numbers, and sends `record` to the current subscribers.
    pub fn publish(&self, record: LogRecord) {
        if !self.has_subscribers() {
            return;
        }
        let entry = LogEntry {
            id: self.0.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp: UtcDateTime::now(),
            level: level_name(record.level),
            target: sanitize(&record.target, MAX_FIELD_BYTES),
            message: sanitize(&record.message, MAX_MESSAGE_BYTES),
            fields: record
                .fields
                .iter()
                .filter(|(name, _)| shareable_field(name))
                .take(MAX_FIELDS)
                .map(|(name, value)| {
                    (
                        sanitize(name, MAX_FIELD_BYTES),
                        sanitize(value, MAX_FIELD_BYTES),
                    )
                })
                .collect(),
            request: record.request.map(|request| RequestContext {
                method: sanitize(&request.method, MAX_FIELD_BYTES),
                path: sanitize(&request.path, MAX_FIELD_BYTES),
                request_id: sanitize(&request.request_id, MAX_FIELD_BYTES),
            }),
        };
        let _ = self.0.sender.send(Arc::new(entry));
    }

    /// Ends every live stream (on server shutdown, so graceful shutdown need not wait for
    /// them).
    pub fn shut_down(&self) {
        self.0.shutdown.send_replace(true);
    }

    /// Becomes `true` when the server shuts down.
    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.0.shutdown.subscribe()
    }
}

/// `LOG_LEVEL`'s name for a level; trace counts as debug.
pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::ERROR => "error",
        Level::WARN => "warning",
        Level::INFO => "info",
        _ => "debug",
    }
}

/// Whether a field may be shared: not `log.*` bookkeeping, not credential-like.
fn shareable_field(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !lower.starts_with("log.") && !SECRET_FIELD_PARTS.iter().any(|part| lower.contains(part))
}

/// Removes ANSI escape sequences and control characters except `\n` and `\t`, then caps the
/// text at `max_bytes` UTF-8 bytes, ending a cut text with `…`.
pub fn sanitize(input: &str, max_bytes: usize) -> String {
    const ELLIPSIS: char = '…';
    let mut out = String::with_capacity(input.len().min(max_bytes));
    let mut chars = input.chars().peekable();
    let mut cut = false;
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                // CSI: parameters and intermediates up to a final byte in `@`..=`~`.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ST (`ESC \`).
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            chars.next_if_eq(&'\\');
                            break;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if c.is_control() && c != '\n' && c != '\t' {
            continue;
        }
        if out.len() + c.len_utf8() > max_bytes {
            cut = true;
            break;
        }
        out.push(c);
    }
    if cut {
        while !out.is_empty() && out.len() + ELLIPSIS.len_utf8() > max_bytes {
            out.pop();
        }
        if out.len() + ELLIPSIS.len_utf8() <= max_bytes {
            out.push(ELLIPSIS);
        }
    }
    out
}

/// Publishes every event that passes the subscriber's filter into a [`LogHub`].
#[derive(Clone, Debug)]
pub struct LogLayer {
    hub: LogHub,
}

impl<S> Layer<S> for LogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().name() != "request" {
            return;
        }
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);
        let mut request = RequestContext::default();
        for (name, value) in visitor.fields {
            match name.as_str() {
                "method" => request.method = value,
                "path" => request.path = value,
                "request_id" => request.request_id = value,
                _ => {}
            }
        }
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(request);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if !self.hub.has_subscribers() {
            return;
        }
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let request = ctx.event_scope(event).and_then(|scope| {
            scope
                .into_iter()
                .find_map(|span| span.extensions().get::<RequestContext>().cloned())
        });
        let metadata = event.metadata();
        self.hub.publish(LogRecord {
            level: *metadata.level(),
            target: metadata.target().to_owned(),
            message: visitor.message.unwrap_or_default(),
            fields: visitor.fields,
            request,
        });
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl FieldVisitor {
    fn push(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else if self.fields.len() < MAX_FIELDS * 2 {
            self.fields.push((field.name().to_owned(), value));
        }
    }
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push(field, format!("{value:?}"));
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    fn record(message: &str) -> LogRecord {
        LogRecord {
            level: Level::INFO,
            target: "test".into(),
            message: message.into(),
            fields: Vec::new(),
            request: None,
        }
    }

    #[test]
    fn strips_ansi_and_control_characters() {
        assert_eq!(
            sanitize("\u{1b}[1;31mred\u{1b}[0m plain\r\u{7}\u{0}", 100),
            "red plain"
        );
        assert_eq!(
            sanitize("\u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{7}", 100),
            "link"
        );
        assert_eq!(
            sanitize("line one\n\tline two", 100),
            "line one\n\tline two"
        );
    }

    #[test]
    fn caps_messages_on_a_character_boundary() {
        let long = "é".repeat(MAX_MESSAGE_BYTES);
        let cut = sanitize(&long, MAX_MESSAGE_BYTES);
        assert!(cut.len() <= MAX_MESSAGE_BYTES);
        assert!(cut.ends_with('…'));
        assert_eq!(sanitize("short", MAX_MESSAGE_BYTES), "short");
        assert_eq!(sanitize("abcdef", 5), "ab…");
    }

    #[test]
    fn normalizes_levels() {
        assert_eq!(level_name(Level::TRACE), "debug");
        assert_eq!(level_name(Level::DEBUG), "debug");
        assert_eq!(level_name(Level::INFO), "info");
        assert_eq!(level_name(Level::WARN), "warning");
        assert_eq!(level_name(Level::ERROR), "error");
    }

    #[test]
    fn publishes_only_to_live_subscribers_and_numbers_entries() {
        let hub = LogHub::new(8);
        hub.publish(record("before anyone listened"));
        let mut receiver = hub.subscribe();
        hub.publish(record("first"));
        hub.publish(record("second"));
        let first = receiver.try_recv().unwrap();
        let second = receiver.try_recv().unwrap();
        assert_eq!(first.message, "first");
        assert_eq!(second.id, first.id + 1);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_slow_subscriber_learns_how_many_entries_it_lost() {
        let hub = LogHub::new(4);
        let mut receiver = hub.subscribe();
        for n in 0..10 {
            hub.publish(record(&format!("entry {n}")));
        }
        assert!(matches!(
            receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Lagged(6))
        ));
        assert_eq!(receiver.try_recv().unwrap().message, "entry 6");
    }

    #[test]
    fn the_layer_carries_fields_and_request_context_without_secrets() {
        let hub = LogHub::new(8);
        let mut receiver = hub.subscribe();
        let subscriber = tracing_subscriber::registry().with(hub.layer());
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "request",
                method = "POST",
                path = "/api/games",
                request_id = "abcdefghijklmnopqrstu"
            );
            let _entered = span.enter();
            tracing::warn!(
                game_id = 7,
                password = "hunter2",
                api_key = "tg_x",
                session_token = "s",
                "saving \u{1b}[32mgame\u{1b}[0m"
            );
        });
        tracing::subscriber::with_default(tracing_subscriber::registry().with(hub.layer()), || {
            tracing::error!("outside a request");
        });

        let entry = receiver.try_recv().unwrap();
        assert_eq!(entry.level, "warning");
        assert_eq!(entry.message, "saving game");
        assert_eq!(entry.fields, vec![("game_id".to_owned(), "7".to_owned())]);
        assert_eq!(
            entry.request,
            Some(RequestContext {
                method: "POST".into(),
                path: "/api/games".into(),
                request_id: "abcdefghijklmnopqrstu".into(),
            })
        );
        let json = entry.to_json();
        assert_eq!(json["request"]["path"], "/api/games");
        assert_eq!(json["fields"]["game_id"], "7");
        assert!(!json.to_string().contains("hunter2"));

        let outside = receiver.try_recv().unwrap();
        assert_eq!(outside.level, "error");
        assert_eq!(outside.request, None);
        assert_eq!(outside.to_json()["request"], Value::Null);
    }
}
