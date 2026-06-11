//! A minimal time-to-live cache for a single value.
//!
//! Ollama lets users add and remove models from the daemon at runtime, so the
//! [`OllamaProvider`](super::OllamaProvider) cannot hold a static model list and
//! must re-query the daemon. Querying on every call would be wasteful, so the
//! provider keeps the last response in a [`TtlCache`]: a lookup returns
//! [`Cached::Hit`] while the value is fresh and [`Cached::Miss`] once it expires
//! (or before anything has been stored), at which point the provider refetches.
//!
//! The cache is intentionally tiny — one slot, no eviction policy, no
//! background refresh — which is why it is hand-rolled rather than pulled from a
//! general-purpose caching crate.

use std::sync::RwLock;
use std::time::{Duration, Instant};

/// The outcome of a [`TtlCache`] lookup.
///
/// [`Hit`](Cached::Hit) carries a clone of the still-fresh value;
/// [`Miss`](Cached::Miss) means nothing is cached or the stored value has
/// outlived its time-to-live and should be refetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cached<T> {
    /// The cached value is still within its time-to-live.
    Hit(T),
    /// Nothing is cached, or the cached value has expired.
    Miss,
}

/// A stored value together with the instant it was written.
#[derive(Debug)]
struct Entry<T> {
    /// The cached value.
    value: T,
    /// When [`value`](Entry::value) was stored, used to evaluate expiry.
    stored_at: Instant,
}

/// A single-slot cache that hands back its value until a time-to-live elapses.
///
/// Lookups go through [`get`](TtlCache::get) and return [`Cached`]; writes go
/// through [`set`](TtlCache::set). The cache is internally synchronized, so it
/// can be shared behind an `Arc` and queried through a shared reference. Reads
/// take only a read lock and never mutate, so an expired entry simply reports
/// [`Cached::Miss`] until the next [`set`](TtlCache::set) overwrites it.
#[derive(Debug)]
pub struct TtlCache<T> {
    /// How long a stored value stays fresh after being written.
    ttl: Duration,
    /// The stored value, if any. `None` until the first write.
    entry: RwLock<Option<Entry<T>>>,
}

impl<T> TtlCache<T> {
    /// Creates an empty cache whose values stay fresh for `ttl`.
    ///
    /// A `ttl` of [`Duration::ZERO`] makes every stored value expire
    /// immediately, so lookups always report [`Cached::Miss`].
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entry: RwLock::new(None),
        }
    }

    /// Looks up the cached value as of now.
    ///
    /// Returns [`Cached::Hit`] with a clone of the value while it is fresh, and
    /// [`Cached::Miss`] once it has expired or if nothing has been stored.
    pub fn get(&self) -> Cached<T>
    where
        T: Clone,
    {
        self.lookup(Instant::now())
    }

    /// Stores `value`, resetting its time-to-live from now.
    ///
    /// Any previously cached value is overwritten.
    pub fn set(&self, value: T) {
        self.store(value, Instant::now());
    }

    /// Looks up the cached value as of `now`.
    ///
    /// Split out from [`get`](TtlCache::get) so expiry can be exercised at exact
    /// instants in tests without sleeping.
    fn lookup(&self, now: Instant) -> Cached<T>
    where
        T: Clone,
    {
        let guard = self.entry.read().expect("TtlCache lock poisoned");
        match guard.as_ref() {
            // `saturating_duration_since` guards against a `now` that predates
            // the write (clock quirks across platforms), treating it as zero
            // elapsed rather than panicking.
            Some(entry) if now.saturating_duration_since(entry.stored_at) < self.ttl => {
                Cached::Hit(entry.value.clone())
            }
            _ => Cached::Miss,
        }
    }

    /// Stores `value` as if written at `at`.
    ///
    /// Split out from [`set`](TtlCache::set) so writes can be pinned to a known
    /// instant in tests.
    fn store(&self, value: T, at: Instant) {
        *self.entry.write().expect("TtlCache lock poisoned") = Some(Entry {
            value,
            stored_at: at,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const TTL: Duration = Duration::from_millis(100);

    #[test]
    fn should_miss_when_empty() {
        let cache: TtlCache<u32> = TtlCache::new(TTL);
        assert_eq!(cache.get(), Cached::Miss);
    }

    #[test]
    fn should_hit_after_set() {
        let cache = TtlCache::new(TTL);
        cache.set(42);
        assert_eq!(cache.get(), Cached::Hit(42));
    }

    #[test]
    fn should_hit_before_ttl_elapses() {
        let cache = TtlCache::new(TTL);
        let base = Instant::now();
        cache.store(7, base);

        // One nanosecond before the deadline the value is still fresh.
        let just_before = base + TTL - Duration::from_nanos(1);
        assert_eq!(cache.lookup(just_before), Cached::Hit(7));
    }

    #[test]
    fn should_miss_at_and_after_ttl() {
        let cache = TtlCache::new(TTL);
        let base = Instant::now();
        cache.store(7, base);

        // Expiry is exclusive: exactly at the deadline the value is stale.
        assert_eq!(cache.lookup(base + TTL), Cached::Miss);
        assert_eq!(
            cache.lookup(base + TTL + Duration::from_secs(1)),
            Cached::Miss
        );
    }

    #[test]
    fn should_always_miss_with_zero_ttl() {
        let cache = TtlCache::new(Duration::ZERO);
        let base = Instant::now();
        cache.store(1, base);
        assert_eq!(cache.lookup(base), Cached::Miss);
    }

    #[test]
    fn should_overwrite_value_and_reset_ttl() {
        let cache = TtlCache::new(TTL);
        let base = Instant::now();
        cache.store(1, base);

        // A later write supersedes the old value and restarts the clock, so a
        // lookup that would have expired the first value still hits the second.
        let later = base + TTL - Duration::from_millis(1);
        cache.store(2, later);
        assert_eq!(
            cache.lookup(later + TTL - Duration::from_millis(1)),
            Cached::Hit(2)
        );
    }

    #[test]
    fn should_cache_non_copy_values() {
        let cache = TtlCache::new(TTL);
        cache.set(vec!["llama3".to_string(), "qwen".to_string()]);
        assert_eq!(
            cache.get(),
            Cached::Hit(vec!["llama3".to_string(), "qwen".to_string()])
        );
    }

    #[test]
    fn should_share_state_across_clones_of_handle() {
        // A shared `Arc` handle sees writes made through any other handle.
        let cache = Arc::new(TtlCache::new(TTL));
        let writer = Arc::clone(&cache);
        writer.set(99);
        assert_eq!(cache.get(), Cached::Hit(99));
    }
}
