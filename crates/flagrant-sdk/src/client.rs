use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flagrant_types::FeatureResponse;

use crate::lru_cache::LruCache;
use crate::transport::{AsyncTransport, BlockingTransport};

/// Immutable snapshot of a resolved feature set, as returned by
/// [`FlagrantClient::get_features`] and [`AsyncFlagrantClient::get_features`].
///
/// Cloning is cheap - clones share the same backing allocation, which is what makes a cache
/// hit a refcount bump instead of a deep copy of every `FeatureResponse`. Derefs to
/// `[FeatureResponse]`, so it can be iterated, indexed, and sliced like a plain slice.
#[derive(Clone, Debug)]
pub struct Features(Arc<[FeatureResponse]>);

impl Deref for Features {
    type Target = [FeatureResponse];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<FeatureResponse>> for Features {
    fn from(features: Vec<FeatureResponse>) -> Self {
        Self(Arc::from(features))
    }
}

type FeaturesCache = Mutex<LruCache<(String, String), Features>>;

/// Pairs a synchronous transport with the one capability it's allowed to expose to external
/// consumers. No matter which transport backs it (HTTP blocking, or a future caching wrapper
/// around one), only feature resolution for an identity is reachable here. Pinned to a single
/// project for its lifetime - same reasoning as `Connection` pinning project+environment in
/// `flagrant-client` - but environment is taken per call, since one client/project commonly
/// spans multiple environments (dev/prod).
///
/// Optionally wraps resolved features in an LRU cache keyed by `(environment, identity)` - opt in
/// with [`Self::with_cache`].
pub struct FlagrantClient<T: BlockingTransport> {
    transport: T,
    project: String,
    cache: Option<FeaturesCache>,
}

impl<T: BlockingTransport> FlagrantClient<T> {
    pub fn new(transport: T, project: String) -> Self {
        Self {
            transport,
            project,
            cache: None,
        }
    }

    /// Enables caching resolved features per `(environment, identity)` pair, with LRU eviction
    /// bounded to `cache_size` entries and an optional `cache_ttl`.
    ///
    /// A hit within TTL only bumps recency. An entry past its TTL is reported as a miss but left
    /// in place untouched, and is used as a fallback if the transport call that follows fails
    /// (e.g. the connection to the Flagrant API died) - so callers still get a previously known
    /// value instead of an error.
    pub fn with_cache(mut self, cache_size: NonZeroUsize, cache_ttl: Option<Duration>) -> Self {
        self.cache = Some(Mutex::new(LruCache::new(cache_size, cache_ttl)));
        self
    }

    /// Returns a [`Features`] snapshot - a cache hit is a cheap refcount bump rather than a
    /// deep clone of every `FeatureResponse` - see [`Self::with_cache`].
    pub fn get_features(&self, environment: &str, identity: &str) -> anyhow::Result<Features> {
        let key = (environment.to_string(), identity.to_string());

        if let Some(features) = self
            .cache
            .as_ref()
            .and_then(|cache| cache.lock().unwrap().get(&key))
        {
            return Ok(features);
        }

        match self
            .transport
            .get_features(&self.project, environment, identity)
        {
            Ok(features) => {
                let features = Features::from(features);
                if let Some(cache) = &self.cache {
                    cache.lock().unwrap().set(key, features.clone());
                }
                Ok(features)
            }
            Err(err) => {
                if let Some(features) = self
                    .cache
                    .as_ref()
                    .and_then(|cache| cache.lock().unwrap().get_stale(&key))
                {
                    return Ok(features);
                }
                Err(err)
            }
        }
    }
}

/// Async counterpart of [`FlagrantClient`], for async-native transports (HTTP async, gRPC).
///
/// Optionally wraps resolved features in an LRU cache keyed by `(environment, identity)` - opt in
/// with [`Self::with_cache`].
pub struct AsyncFlagrantClient<T: AsyncTransport> {
    transport: T,
    project: String,
    cache: Option<FeaturesCache>,
}

impl<T: AsyncTransport> AsyncFlagrantClient<T> {
    pub fn new(transport: T, project: String) -> Self {
        Self {
            transport,
            project,
            cache: None,
        }
    }

    /// Same as [`FlagrantClient::with_cache`], but for an async-native transport.
    pub fn with_cache(mut self, cache_size: NonZeroUsize, cache_ttl: Option<Duration>) -> Self {
        self.cache = Some(Mutex::new(LruCache::new(cache_size, cache_ttl)));
        self
    }

    /// Returns a [`Features`] snapshot - a cache hit is a cheap refcount bump rather than a
    /// deep clone of every `FeatureResponse` - see [`Self::with_cache`].
    pub async fn get_features(
        &self,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Features> {
        let key = (environment.to_string(), identity.to_string());

        if let Some(features) = self
            .cache
            .as_ref()
            .and_then(|cache| cache.lock().unwrap().get(&key))
        {
            return Ok(features);
        }

        match self
            .transport
            .get_features(&self.project, environment, identity)
            .await
        {
            Ok(features) => {
                let features = Features::from(features);
                if let Some(cache) = &self.cache {
                    cache.lock().unwrap().set(key, features.clone());
                }
                Ok(features)
            }
            Err(err) => {
                if let Some(features) = self
                    .cache
                    .as_ref()
                    .and_then(|cache| cache.lock().unwrap().get_stale(&key))
                {
                    return Ok(features);
                }
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::sleep;

    use flagrant_types::{TagList, VariantValue};

    use super::*;

    /// Transport that succeeds on the first `fail_from_call - 1` calls, then fails
    /// on every call onwards, so tests can assert exactly how many times it was hit.
    struct FlakyTransport {
        calls: AtomicUsize,
        fail_from_call: usize,
    }

    impl FlakyTransport {
        fn new(fail_from_call: usize) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail_from_call,
            }
        }

        fn feature(id: i32) -> FeatureResponse {
            FeatureResponse {
                feature_id: id,
                name: "flag".into(),
                description: String::new(),
                tags: TagList(vec![]),
                value: VariantValue::Text(id.to_string()),
                is_enabled: true,
                is_srv: false,
            }
        }
    }

    impl FlakyTransport {
        fn next_response(&self) -> anyhow::Result<Vec<FeatureResponse>> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call >= self.fail_from_call {
                anyhow::bail!("connection to flagrant API died");
            }
            Ok(vec![Self::feature(call as i32)])
        }
    }

    impl BlockingTransport for FlakyTransport {
        fn get_features(
            &self,
            _project: &str,
            _environment: &str,
            _identity: &str,
        ) -> anyhow::Result<Vec<FeatureResponse>> {
            self.next_response()
        }
    }

    impl AsyncTransport for FlakyTransport {
        async fn get_features(
            &self,
            _project: &str,
            _environment: &str,
            _identity: &str,
        ) -> anyhow::Result<Vec<FeatureResponse>> {
            self.next_response()
        }
    }

    #[test]
    fn without_cache_every_call_hits_the_transport() {
        let client = FlagrantClient::new(FlakyTransport::new(usize::MAX), "proj".to_string());

        let first = client.get_features("prod", "user-1").unwrap();
        let second = client.get_features("prod", "user-1").unwrap();

        assert_eq!(first[0].feature_id, 1);
        assert_eq!(second[0].feature_id, 2, "second call must not be cached");
    }

    #[test]
    fn cache_hit_within_ttl_skips_the_transport() {
        let client = FlagrantClient::new(FlakyTransport::new(usize::MAX), "proj".to_string())
            .with_cache(NonZeroUsize::new(4).unwrap(), Some(Duration::from_secs(60)));

        let first = client.get_features("prod", "user-1").unwrap();
        let second = client.get_features("prod", "user-1").unwrap();

        assert_eq!(first[0].feature_id, second[0].feature_id);
    }

    #[test]
    fn expired_entry_is_refetched_and_replaced() {
        let client = FlagrantClient::new(FlakyTransport::new(usize::MAX), "proj".to_string())
            .with_cache(
                NonZeroUsize::new(4).unwrap(),
                Some(Duration::from_millis(1)),
            );

        let first = client.get_features("prod", "user-1").unwrap();
        sleep(Duration::from_millis(20));
        let second = client.get_features("prod", "user-1").unwrap();

        assert_ne!(
            first[0].feature_id, second[0].feature_id,
            "expired entry must be refetched, not reused"
        );
    }

    #[test]
    fn stale_entry_is_served_when_transport_fails() {
        // succeeds on the first call, fails on the second
        let client = FlagrantClient::new(FlakyTransport::new(2), "proj".to_string()).with_cache(
            NonZeroUsize::new(4).unwrap(),
            Some(Duration::from_millis(1)),
        );

        let fresh = client.get_features("prod", "user-1").unwrap();
        sleep(Duration::from_millis(20));
        let fallback = client.get_features("prod", "user-1").unwrap();

        assert_eq!(
            fresh[0].feature_id, fallback[0].feature_id,
            "a failed transport call must fall back to the stale cached value"
        );
    }

    #[test]
    fn error_propagates_when_no_stale_entry_is_available() {
        let client = FlagrantClient::new(FlakyTransport::new(1), "proj".to_string())
            .with_cache(NonZeroUsize::new(4).unwrap(), None);

        assert!(client.get_features("prod", "user-1").is_err());
    }
}
