//! A bounded in-memory cache with expiry.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

/// Default entry lifetime.
pub const TTL: Duration = Duration::from_mins(5);
/// Default size cap.
pub const MAX_ENTRIES: usize = 1_000;

struct Entry<V> {
    value: V,
    expires_at: Instant,
    inserted: u64,
}

/// Entries expire after `ttl`; at `max_entries` the oldest entry makes room.
pub struct TtlCache<K, V> {
    entries: Mutex<(HashMap<K, Entry<V>>, u64)>,
    ttl: Duration,
    max_entries: usize,
}

impl<K, V> std::fmt::Debug for TtlCache<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TtlCache")
            .field("ttl", &self.ttl)
            .field("max_entries", &self.max_entries)
            .finish_non_exhaustive()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    /// An empty cache.
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: Mutex::new((HashMap::new(), 0)),
            ttl,
            max_entries: max_entries.max(1),
        }
    }

    /// The live value for `key`; an expired one is dropped.
    pub fn fetch(&self, key: &K) -> Option<V> {
        let mut guard = self.entries.lock().ok()?;
        let now = Instant::now();
        match guard.0.get(key) {
            Some(entry) if entry.expires_at > now => Some(entry.value.clone()),
            Some(_) => {
                guard.0.remove(key);
                None
            }
            None => None,
        }
    }

    /// Stores `value`, sweeping expired entries and evicting the oldest when full.
    pub fn put(&self, key: K, value: V) {
        let Ok(mut guard) = self.entries.lock() else {
            return;
        };
        let now = Instant::now();
        guard.0.retain(|_, entry| entry.expires_at > now);
        if !guard.0.contains_key(&key) && guard.0.len() >= self.max_entries {
            let oldest = guard
                .0
                .iter()
                .min_by_key(|(_, entry)| entry.inserted)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                guard.0.remove(&oldest);
            }
        }
        guard.1 = guard.1.wrapping_add(1);
        let inserted = guard.1;
        guard.0.insert(
            key,
            Entry {
                value,
                expires_at: now + self.ttl,
                inserted,
            },
        );
    }

    /// Drops expired entries.
    pub fn sweep(&self) {
        if let Ok(mut guard) = self.entries.lock() {
            let now = Instant::now();
            guard.0.retain(|_, entry| entry.expires_at > now);
        }
    }

    /// Drops everything.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.0.clear();
        }
    }

    /// Entries held, expired or not.
    pub fn len(&self) -> usize {
        self.entries.lock().map_or(0, |guard| guard.0.len())
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every stored value (tests inspect what is cached).
    pub fn values(&self) -> Vec<V> {
        self.entries
            .lock()
            .map(|guard| guard.0.values().map(|entry| entry.value.clone()).collect())
            .unwrap_or_default()
    }

    /// Every stored key.
    pub fn keys(&self) -> Vec<K> {
        self.entries
            .lock()
            .map(|guard| guard.0.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sweep_removes_expired_entries_that_are_never_fetched_again() {
        let cache = TtlCache::new(Duration::ZERO, 10);
        cache.put("expired", 1);
        cache.sweep();
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn size_cap_evicts_an_old_entry() {
        let cache = TtlCache::new(TTL, 1);
        cache.put("first", 1);
        cache.put("second", 2);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.fetch(&"second"), Some(2));
    }
}
