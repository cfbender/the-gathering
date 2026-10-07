//! Single-node `Phoenix.PubSub` with channel fastlaning.
//!
//! A channel subscribes with its socket's outbound queue: broadcasts go straight to the
//! client (encoded once per broadcast), as Phoenix's fastlane does, except intercepted
//! events (`presence_diff`), which go to the channel task for per-subscriber handling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use tokio::sync::mpsc;

use super::protocol::{Outbound, encode};

/// A message published to a topic.
#[derive(Clone, Debug)]
pub struct Broadcast {
    /// Topic.
    pub topic: String,
    /// Event name.
    pub event: String,
    /// Payload.
    pub payload: Arc<Value>,
}

/// Where a subscriber's messages go.
#[derive(Clone, Debug)]
pub enum Sink {
    /// A joined channel.
    Channel {
        /// The socket's outbound frames.
        outbound: mpsc::UnboundedSender<Outbound>,
        /// The channel task, for intercepted events.
        intercept: mpsc::UnboundedSender<Broadcast>,
        /// Events handled by the channel instead of fastlaned.
        intercepts: &'static [&'static str],
    },
    /// A plain listener (`Endpoint.subscribe/1`).
    Listener(mpsc::UnboundedSender<Broadcast>),
}

impl Sink {
    fn closed(&self) -> bool {
        match self {
            Self::Channel { outbound, intercept, .. } => outbound.is_closed() || intercept.is_closed(),
            Self::Listener(sender) => sender.is_closed(),
        }
    }
}

#[derive(Debug, Default)]
struct Inner {
    topics: Mutex<HashMap<String, Vec<(u64, Sink)>>>,
    next_id: AtomicU64,
}

/// Topic subscriptions. Cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct PubSub(Arc<Inner>);

impl PubSub {
    /// No subscribers.
    pub fn new() -> Self {
        Self::default()
    }

    fn topics(&self) -> MutexGuard<'_, HashMap<String, Vec<(u64, Sink)>>> {
        self.0.topics.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Subscribes `sink` to `topic`; returns the subscription id.
    pub fn subscribe(&self, topic: &str, sink: Sink) -> u64 {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        self.topics().entry(topic.to_owned()).or_default().push((id, sink));
        id
    }

    /// A listener receiving every broadcast to `topic`.
    pub fn listen(&self, topic: &str) -> mpsc::UnboundedReceiver<Broadcast> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.subscribe(topic, Sink::Listener(sender));
        receiver
    }

    /// Removes a subscription.
    pub fn unsubscribe(&self, topic: &str, id: u64) {
        let mut topics = self.topics();
        if let Some(subscribers) = topics.get_mut(topic) {
            subscribers.retain(|(subscriber, _)| *subscriber != id);
            if subscribers.is_empty() {
                topics.remove(topic);
            }
        }
    }

    /// `Endpoint.broadcast!/3`: delivers to every subscriber of `topic`, in order.
    pub fn broadcast(&self, topic: &str, event: &str, payload: Value) {
        let payload = Arc::new(payload);
        let mut topics = self.topics();
        let Some(subscribers) = topics.get_mut(topic) else {
            return;
        };
        subscribers.retain(|(_, sink)| !sink.closed());
        let message = Broadcast { topic: topic.to_owned(), event: event.to_owned(), payload: Arc::clone(&payload) };
        let mut frame: Option<axum::extract::ws::Utf8Bytes> = None;
        for (_, sink) in subscribers.iter() {
            match sink {
                Sink::Channel { intercept, intercepts, .. } if intercepts.contains(&event) => {
                    let _ = intercept.send(message.clone());
                }
                Sink::Channel { outbound, .. } => {
                    let text = frame.get_or_insert_with(|| encode(None, None, topic, event, &payload).into());
                    let _ = outbound.send(Outbound::Text(text.clone()));
                }
                Sink::Listener(sender) => {
                    let _ = sender.send(message.clone());
                }
            }
        }
        if subscribers.is_empty() {
            topics.remove(topic);
        }
    }
}
