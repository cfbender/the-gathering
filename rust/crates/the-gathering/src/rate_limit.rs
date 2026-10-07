//! In-memory fixed-window rate limiter (Hammer's ETS `fix_window` algorithm) and the
//! token buckets webcam table channels spend per event.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::{BucketLimit, WindowLimit};

/// The outcome of a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Allowed; the count in this window.
    Allow(u64),
    /// Denied; milliseconds until the window resets.
    Deny(u64),
}

/// Counters keyed by bucket name and window.
#[derive(Debug, Default)]
pub struct RateLimiter {
    counters: Mutex<HashMap<(String, u64), (u64, u64)>>,
}

fn now_ms() -> u64 {
    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())).unwrap_or(u64::MAX)
}

impl RateLimiter {
    /// An empty limiter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts one hit for `key` against `limit`.
    pub fn hit(&self, key: &str, limit: WindowLimit) -> Decision {
        self.hit_at(key, limit, now_ms())
    }

    fn hit_at(&self, key: &str, limit: WindowLimit, now: u64) -> Decision {
        let scale = u64::try_from(limit.scale.as_millis()).unwrap_or(u64::MAX).max(1);
        let window = now / scale;
        let expires_at = window.saturating_add(1).saturating_mul(scale);
        let Ok(mut counters) = self.counters.lock() else {
            return Decision::Allow(1);
        };
        if counters.len() > 10_000 {
            counters.retain(|_, (_, expires)| *expires > now);
        }
        let entry = counters.entry((key.to_owned(), window)).or_insert((0, expires_at));
        entry.0 = entry.0.saturating_add(1);
        if entry.0 <= limit.limit {
            Decision::Allow(entry.0)
        } else {
            Decision::Deny(expires_at.saturating_sub(now))
        }
    }

    /// Forgets every counter (tests).
    pub fn reset(&self) {
        if let Ok(mut counters) = self.counters.lock() {
            counters.clear();
        }
    }
}

/// A token bucket owned by one channel connection (`TheGatheringWeb.ChannelRateLimit`).
#[derive(Clone, Copy, Debug)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_ms: f64,
    tokens: f64,
    at: Instant,
}

impl TokenBucket {
    /// A full bucket.
    pub fn new(limit: BucketLimit) -> Self {
        Self {
            capacity: limit.capacity,
            refill_per_ms: limit.refill_per_second / 1000.0,
            tokens: limit.capacity,
            at: Instant::now(),
        }
    }

    /// Spends one token, refilling for the time since the last spend.
    pub fn take(&mut self) -> bool {
        self.take_at(Instant::now())
    }

    /// Tokens left after the last spend.
    pub fn tokens(&self) -> f64 {
        self.tokens
    }

    /// When the bucket last changed (the reference for [`TokenBucket::take_at`]).
    pub fn at(&self) -> Instant {
        self.at
    }

    /// [`TokenBucket::take`] at a given instant.
    pub fn take_at(&mut self, now: Instant) -> bool {
        let elapsed_ms = now.saturating_duration_since(self.at).as_secs_f64() * 1000.0;
        let tokens = (self.tokens + elapsed_ms * self.refill_per_ms).min(self.capacity);
        if tokens >= 1.0 {
            self.tokens = tokens - 1.0;
            self.at = now;
            true
        } else {
            false
        }
    }
}

/// Seconds to wait, rounded up, at least one (`retry-after`).
pub fn retry_after_seconds(ms: u64) -> u64 {
    ms.div_ceil(1000).max(1)
}

/// A minute.
pub const MINUTE: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_after_the_limit_until_the_window_resets() {
        let limiter = RateLimiter::new();
        let limit = WindowLimit { limit: 2, scale: Duration::from_secs(60) };
        assert_eq!(limiter.hit_at("k", limit, 120_000), Decision::Allow(1));
        assert_eq!(limiter.hit_at("k", limit, 120_001), Decision::Allow(2));
        assert_eq!(limiter.hit_at("k", limit, 150_000), Decision::Deny(30_000));
        assert_eq!(limiter.hit_at("k", limit, 180_000), Decision::Allow(1));
    }

    #[test]
    fn buckets_refill() {
        let mut bucket = TokenBucket::new(BucketLimit { capacity: 1.0, refill_per_second: 10.0 });
        let start = bucket.at;
        assert!(bucket.take_at(start));
        assert!(!bucket.take_at(start));
        assert!(bucket.take_at(start + Duration::from_millis(100)));
    }
}
