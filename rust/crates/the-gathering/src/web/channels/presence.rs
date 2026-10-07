//! Single-node presence tracking, compatible with the `phoenix` JS client's `Presence`.
//!
//! Each tracked entry belongs to an owner (a channel task) and a key (a peer id). Every change
//! broadcasts `presence_diff` (`{joins, leaves}` keyed like `presence_state`, metas carrying
//! `phx_ref` and, after an update, `phx_ref_prev`), which `phoenix.js`'s `Presence` merges.
//! Diffs are published while the state lock is held, so subscribers see them in the order
//! the state changed.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Map, Value, json};

use super::pubsub::PubSub;
use crate::crypto;

#[derive(Clone, Debug)]
struct Tracked {
    owner: u64,
    meta: Value,
}

type Topic = BTreeMap<String, Vec<Tracked>>;

#[derive(Debug)]
struct Inner {
    pubsub: PubSub,
    topics: Mutex<HashMap<String, Topic>>,
    prefix: String,
    counter: AtomicU64,
}

/// Who is present on each topic. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Presence(Arc<Inner>);

fn with_refs(meta: &Value, phx_ref: &str, previous: Option<&str>) -> Value {
    let mut object = match meta {
        Value::Object(object) => object.clone(),
        _ => Map::new(),
    };
    object.remove("phx_ref_prev");
    object.insert("phx_ref".into(), json!(phx_ref));
    if let Some(previous) = previous {
        object.insert("phx_ref_prev".into(), json!(previous));
    }
    Value::Object(object)
}

fn phx_ref_of(meta: &Value) -> Option<&str> {
    meta.get("phx_ref").and_then(Value::as_str)
}

fn grouped(entries: Vec<(String, Value)>) -> Value {
    let mut map = Map::new();
    for (key, meta) in entries {
        let slot = map.entry(key).or_insert_with(|| json!({ "metas": [] }));
        if let Some(Value::Array(metas)) = slot.get_mut("metas") {
            metas.push(meta);
        }
    }
    Value::Object(map)
}

impl Presence {
    /// Presence publishing diffs on `pubsub`.
    pub fn new(pubsub: PubSub) -> Self {
        Self(Arc::new(Inner {
            pubsub,
            topics: Mutex::new(HashMap::new()),
            prefix: crypto::url_encode64_unpadded(&crypto::random_bytes::<6>()),
            counter: AtomicU64::new(0),
        }))
    }

    fn topics(&self) -> MutexGuard<'_, HashMap<String, Topic>> {
        self.0
            .topics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn next_ref(&self) -> String {
        format!(
            "{}{}",
            self.0.prefix,
            self.0.counter.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn publish(&self, topic: &str, joins: Vec<(String, Value)>, leaves: Vec<(String, Value)>) {
        if joins.is_empty() && leaves.is_empty() {
            return;
        }
        self.0.pubsub.broadcast(
            topic,
            "presence_diff",
            json!({ "joins": grouped(joins), "leaves": grouped(leaves) }),
        );
    }

    /// `Presence.track/4`: `owner` is present on `topic` under `key` with `meta`.
    pub fn track(&self, topic: &str, owner: u64, key: &str, meta: &Value) {
        let mut topics = self.topics();
        let meta = with_refs(meta, &self.next_ref(), None);
        topics
            .entry(topic.to_owned())
            .or_default()
            .entry(key.to_owned())
            .or_default()
            .push(Tracked {
                owner,
                meta: meta.clone(),
            });
        self.publish(topic, vec![(key.to_owned(), meta)], Vec::new());
    }

    /// `Presence.update/4`: replaces `owner`'s meta under `key`. Untracked keys are ignored.
    pub fn update(&self, topic: &str, owner: u64, key: &str, meta: &Value) {
        let mut topics = self.topics();
        let Some(tracked) = topics
            .get_mut(topic)
            .and_then(|keys| keys.get_mut(key))
            .and_then(|entries| entries.iter_mut().find(|entry| entry.owner == owner))
        else {
            return;
        };
        let previous = tracked.meta.clone();
        let next = with_refs(meta, &self.next_ref(), phx_ref_of(&previous));
        tracked.meta = next.clone();
        self.publish(
            topic,
            vec![(key.to_owned(), next)],
            vec![(key.to_owned(), previous)],
        );
    }

    /// Removes everything `owner` tracks on `topic`.
    pub fn untrack(&self, topic: &str, owner: u64) {
        let mut topics = self.topics();
        let Some(keys) = topics.get_mut(topic) else {
            return;
        };
        let mut leaves = Vec::new();
        for (key, entries) in keys.iter_mut() {
            entries.retain(|entry| {
                let keep = entry.owner != owner;
                if !keep {
                    leaves.push((key.clone(), entry.meta.clone()));
                }
                keep
            });
        }
        keys.retain(|_, entries| !entries.is_empty());
        if keys.is_empty() {
            topics.remove(topic);
        }
        self.publish(topic, Vec::new(), leaves);
    }

    /// `Presence.list/1`: `{key => {metas: [...]}}`.
    pub fn list(&self, topic: &str) -> Value {
        let topics = self.topics();
        let entries = topics
            .get(topic)
            .map(|keys| {
                keys.iter()
                    .flat_map(|(key, entries)| {
                        entries
                            .iter()
                            .map(|entry| (key.clone(), entry.meta.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        grouped(entries)
    }

    /// Every meta on `topic`.
    pub fn metas(&self, topic: &str) -> Vec<Value> {
        let topics = self.topics();
        topics
            .get(topic)
            .map(|keys| {
                keys.values()
                    .flatten()
                    .map(|entry| entry.meta.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `Presence.get_by_key/2`.
    pub fn get_by_key(&self, topic: &str, key: &str) -> Option<Value> {
        let topics = self.topics();
        let entries = topics.get(topic)?.get(key)?;
        Some(json!({ "metas": entries.iter().map(|entry| entry.meta.clone()).collect::<Vec<_>>() }))
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
