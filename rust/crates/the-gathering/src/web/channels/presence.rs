//! Who is at each table.
//!
//! Each entry belongs to an owner (a channel) and a key (a peer id). Every change sends the
//! topic's Socket.IO room its full roster as `presence` (one meta per key) while the lock is
//! held, so sockets receive rosters in the order they changed. Keys that leave are also
//! published to channels ([`Presence::leaves`]); a reveal ends when its target leaves.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use socketioxide::SocketIo;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
struct Tracked {
    owner: u64,
    meta: Value,
}

type Topic = BTreeMap<String, Vec<Tracked>>;

/// A key that is no longer present on a topic.
#[derive(Clone, Debug)]
pub struct Left {
    /// Topic.
    pub topic: String,
    /// The key that left.
    pub key: String,
}

#[derive(Debug)]
struct Inner {
    io: SocketIo,
    topics: Mutex<HashMap<String, Topic>>,
    leaves: broadcast::Sender<Left>,
}

/// Who is present on each topic. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Presence(Arc<Inner>);

fn roster(topic: Option<&Topic>) -> Vec<Value> {
    topic
        .map(|keys| {
            keys.values()
                .filter_map(|entries| entries.first().map(|entry| entry.meta.clone()))
                .collect()
        })
        .unwrap_or_default()
}

impl Presence {
    /// Presence sending rosters through `io`.
    pub fn new(io: SocketIo) -> Self {
        Self(Arc::new(Inner {
            io,
            topics: Mutex::new(HashMap::new()),
            leaves: broadcast::channel(256).0,
        }))
    }

    fn topics(&self) -> MutexGuard<'_, HashMap<String, Topic>> {
        self.0
            .topics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Sends the topic's room its roster. Called with the lock held.
    fn publish(&self, topics: &HashMap<String, Topic>, topic: &str) {
        let sockets = self.0.io.to(topic.to_owned()).sockets();
        if sockets.is_empty() {
            return;
        }
        let roster = roster(topics.get(topic));
        for socket in sockets {
            let _ = socket.emit("presence", &roster);
        }
    }

    /// Keys leaving any topic, from now on.
    pub fn leaves(&self) -> broadcast::Receiver<Left> {
        self.0.leaves.subscribe()
    }

    /// `owner` is present on `topic` under `key` with `meta`.
    pub fn track(&self, topic: &str, owner: u64, key: &str, meta: Value) {
        let mut topics = self.topics();
        topics
            .entry(topic.to_owned())
            .or_default()
            .entry(key.to_owned())
            .or_default()
            .push(Tracked { owner, meta });
        self.publish(&topics, topic);
    }

    /// Replaces `owner`'s meta under `key`. Untracked keys are ignored.
    pub fn update(&self, topic: &str, owner: u64, key: &str, meta: Value) {
        let mut topics = self.topics();
        let Some(tracked) = topics
            .get_mut(topic)
            .and_then(|keys| keys.get_mut(key))
            .and_then(|entries| entries.iter_mut().find(|entry| entry.owner == owner))
        else {
            return;
        };
        tracked.meta = meta;
        self.publish(&topics, topic);
    }

    /// Removes everything `owner` tracks on `topic`.
    pub fn untrack(&self, topic: &str, owner: u64) {
        let mut topics = self.topics();
        let Some(keys) = topics.get_mut(topic) else {
            return;
        };
        let before = keys.values().map(Vec::len).sum::<usize>();
        let mut left = Vec::new();
        keys.retain(|key, entries| {
            entries.retain(|entry| entry.owner != owner);
            if entries.is_empty() {
                left.push(key.clone());
            }
            !entries.is_empty()
        });
        let changed = keys.values().map(Vec::len).sum::<usize>() != before;
        if keys.is_empty() {
            topics.remove(topic);
        }
        if changed {
            self.publish(&topics, topic);
        }
        for key in left {
            let _ = self.0.leaves.send(Left {
                topic: topic.to_owned(),
                key,
            });
        }
    }

    /// One meta per present key, in key order.
    pub fn list(&self, topic: &str) -> Vec<Value> {
        roster(self.topics().get(topic))
    }

    /// Every meta on `topic`.
    pub fn metas(&self, topic: &str) -> Vec<Value> {
        self.topics()
            .get(topic)
            .map(|keys| {
                keys.values()
                    .flatten()
                    .map(|entry| entry.meta.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The meta present under `key`.
    pub fn get(&self, topic: &str, key: &str) -> Option<Value> {
        let topics = self.topics();
        Some(topics.get(topic)?.get(key)?.first()?.meta.clone())
    }

    /// Whether anyone is present under `key`.
    pub fn has_key(&self, topic: &str, key: &str) -> bool {
        self.topics()
            .get(topic)
            .is_some_and(|keys| keys.contains_key(key))
    }

    /// How many keys are present.
    pub fn count(&self, topic: &str) -> usize {
        self.topics().get(topic).map_or(0, BTreeMap::len)
    }
}
