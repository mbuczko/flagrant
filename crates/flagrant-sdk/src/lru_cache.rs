use std::hash::Hash;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache as Lru;

/// A size-bounded cache with LRU eviction and an optional per-entry TTL.
pub(crate) struct LruCache<K, V> {
    ttl: Option<Duration>,
    data: Lru<K, (Instant, V)>,
}

impl<K: Hash + Eq, V: Clone> LruCache<K, V> {
    pub fn new(max_size: NonZeroUsize, ttl: Option<Duration>) -> Self {
        Self {
            ttl,
            data: Lru::new(max_size),
        }
    }

    /// Returns the cached value, or `None` if missing or expired.
    ///
    /// A hit only bumps recency (moves the key to the front of the
    /// underlying LRU list) when it's within TTL; expired entries are
    /// reported as missing but left in place, untouched, so [`Self::get_stale`]
    /// can still recover them for failure fallback.
    pub fn get(&mut self, key: &K) -> Option<V> {
        let (timestamp, _) = self.data.peek(key)?;
        if self.is_expired(*timestamp) {
            return None;
        }
        self.data.get(key).map(|(_, value)| value.clone())
    }

    /// Returns the cached value even if its TTL has expired, without
    /// evicting or promoting it. Used as a failure fallback when a fresh
    /// fetch fails (e.g. the connection to the API died), so callers can
    /// still serve a previously known value instead of nothing.
    pub fn get_stale(&mut self, key: &K) -> Option<V> {
        self.data.peek(key).map(|(_, value)| value.clone())
    }

    pub fn set(&mut self, key: K, value: V) {
        self.data.put(key, (Instant::now(), value));
    }

    fn is_expired(&self, timestamp: Instant) -> bool {
        self.ttl.is_some_and(|ttl| timestamp.elapsed() > ttl)
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;

    use super::*;

    fn cache<V: Clone>(max_size: usize, ttl: Option<Duration>) -> LruCache<&'static str, V> {
        LruCache::new(NonZeroUsize::new(max_size).unwrap(), ttl)
    }

    #[test]
    fn hit_within_ttl_returns_value() {
        let mut cache = cache(2, Some(Duration::from_secs(60)));
        cache.set("a", 1);

        assert_eq!(cache.get(&"a"), Some(1));
    }

    #[test]
    fn miss_for_unknown_key() {
        let mut cache = cache::<i32>(2, None);
        assert_eq!(cache.get(&"missing"), None);
        assert_eq!(cache.get_stale(&"missing"), None);
    }

    #[test]
    fn expired_entry_is_a_miss_but_stays_available_as_stale() {
        let mut cache = cache(2, Some(Duration::from_millis(1)));
        cache.set("a", 1);
        sleep(Duration::from_millis(20));

        assert_eq!(
            cache.get(&"a"),
            None,
            "expired entry must be reported as missing"
        );
        assert_eq!(
            cache.get_stale(&"a"),
            Some(1),
            "expired entry must still be recoverable as a stale fallback"
        );
    }

    #[test]
    fn expired_get_does_not_promote_the_entry() {
        let mut cache = cache(2, Some(Duration::from_millis(10)));
        cache.set("a", 1); // oldest
        sleep(Duration::from_millis(20)); // "a" is now expired
        cache.set("b", 2); // freshly inserted, most recently used

        // a hit on an expired entry must be reported as a miss without bumping its
        // recency - if it wrongly promoted "a" ahead of "b", the eviction below would
        // take out "b" instead.
        assert_eq!(cache.get(&"a"), None);

        cache.set("c", 3); // capacity 2: evicts the least recently used entry

        assert_eq!(cache.get_stale(&"a"), None, "\"a\" should have been evicted");
        assert_eq!(cache.get_stale(&"b"), Some(2), "\"b\" should have survived eviction");
        assert_eq!(cache.get_stale(&"c"), Some(3));
    }

    #[test]
    fn hit_bumps_recency_so_it_survives_eviction() {
        let mut cache = cache(2, None);
        cache.set("a", 1);
        cache.set("b", 2);

        // touch "a" so "b" becomes the least recently used entry
        assert_eq!(cache.get(&"a"), Some(1));

        cache.set("c", 3);

        assert_eq!(
            cache.get(&"b"),
            None,
            "least recently used entry should be evicted"
        );
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn set_evicts_least_recently_used_beyond_capacity() {
        let mut cache = cache(2, None);
        cache.set("a", 1);
        cache.set("b", 2);
        cache.set("c", 3);

        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }
}
