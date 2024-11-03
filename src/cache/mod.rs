pub mod entry;
pub mod level;

use crate::cache::entry::{Cached, CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::CacheLevel;
use crate::settings;
use crate::settings::CacheEntry;
use lazy_static::lazy_static;
use metrics::MetricsEvent;
use prometheus::{register_histogram_vec, HistogramVec};
use std::fmt::Debug;
use tracing::warn;
use uuid::Uuid;

lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// cache requests (`request_type`). Use the [monitor_get] utility for ease of use.
    pub(crate) static ref CACHE_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["cache_variant", "request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache get request result age in seconds. It is intended to be used by all
    /// cache requests (`request_type`). Use the [monitor_get] utility for ease of use.
    pub(crate) static ref CACHE_AGE_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_age_duration_seconds",
        "The cache get request latencies in seconds.",
        &["cache_variant", "request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    ///  cache requests (`request_type`). Use the [monitor_set] utility for ease of use.
    pub(crate) static ref CACHE_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["cache_variant", "request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

fn metrics_get_handler<T: Clone + Debug + Eq>(event: MetricsEvent<Cached<T>>) {
    let cache_result = match event.result {
        Cached::Hit(_) => "hit",
        Cached::Expired(_) => "expired",
        Cached::Miss => "miss",
    };
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    let cache_variant = "cache";
    CACHE_GET_HISTOGRAM
        .with_label_values(&[cache_variant, request_type, cache_result])
        .observe(event.time);

    match event.result {
        Cached::Hit(entry) | Cached::Expired(entry) => {
            CACHE_AGE_HISTOGRAM
                .with_label_values(&[cache_variant, request_type])
                .observe(entry.current_age() as f64);
        }
        _ => {}
    };
}

fn metrics_set_handler<T: Clone + Debug + Eq>(event: MetricsEvent<Entry<T>>) {
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    let cache_variant = "cache";
    CACHE_SET_HISTOGRAM
        .with_label_values(&[cache_variant, request_type])
        .observe(event.time);
}

/// A [Cache] is a thread-safe multi-level cache. [Levels](CacheLevel) are added to the end of the stack.
/// That means that the last added level is the lowest level. In general, the lower level caches should be
/// remote/persistent caches while the upper level caches should be fast in-memory caches. Also,
/// upper level caches should be subsets of lower level caches.
///
/// - **Get operations** find the first [CacheLevel] that contains a some [Entry].
///   When a [Hit] is found, all previous levels are updated with that [Entry]. Otherwise, it uses the
///   last found [Expired] entry. If no [Entry] could be found. Nothing is updated.
/// - **Set operations** update all levels, starting with the lowest level.
///
/// ```rs
/// let cache = Cache::new(...)
///   // add cache level 1
///   .add_level(true, || async { ... }).await?
///   // skip cache level 2 (disabled)
///   .add_level(false, || async { ... }).await?
///   // add cache level 3 (added as cache level 2)
///   .add_level(true, || async { ... }).await?;
/// ```
pub struct Cache<L, R>
where
    L: CacheLevel,
    R: CacheLevel,
{
    expiry: settings::CacheEntries<CacheEntry>,
    local_cache: L,
    remote_cache: R,
}

impl<L, R> Cache<L, R>
where
    L: CacheLevel,
    R: CacheLevel,
{
    /// Creates a new [Cache] with no inner caches.
    pub fn new(
        expiry: settings::CacheEntries<CacheEntry>,
        local_cache: L,
        remote_cache: R,
    ) -> Self {
        Cache {
            expiry,
            local_cache,
            remote_cache,
        }
    }

    /// Gets some [UuidData] from the [Cache] for a case-insensitive username.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(request_type = "uuid"),
        handler = metrics_get_handler,
    )]
    pub async fn get_uuid(&self, key: &str) -> Cached<UuidData> {
        let local = self.local_cache.get_uuid(key).await;
        if let Some(entry) = &local {
            if !entry.is_expired(&self.expiry.uuid) {
                return Cached::with_expiry(local, &self.expiry.uuid);
            }
        }

        let remote = self.remote_cache.get_uuid(key).await;
        match &remote {
            None => {
                // if remote cache has no value, use local result
                Cached::with_expiry(local, &self.expiry.uuid)
            }
            Some(entry) => {
                // if remote cache has a value, sync with local cache
                self.local_cache.set_uuid(key, entry.clone()).await;
                Cached::with_expiry(remote, &self.expiry.uuid)
            }
        }
    }

    /// Sets some optional [UuidData] to the [Cache] for a case-insensitive username.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(request_type = "uuid"),
        handler = metrics_set_handler,
    )]
    pub async fn set_uuid(&self, key: &str, data: Option<UuidData>) -> Entry<UuidData> {
        let entry = Entry::from(data);
        self.local_cache.set_uuid(key, entry.clone()).await;
        self.remote_cache.set_uuid(key, entry.clone()).await;
        entry
    }

    /// Gets some [ProfileData] from the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(request_type = "profile"),
        handler = metrics_get_handler,
    )]
    pub async fn get_profile(&self, uuid: &Uuid) -> Cached<ProfileData> {
        let local = self.local_cache.get_profile(uuid).await;
        if let Some(entry) = &local {
            if !entry.is_expired(&self.expiry.profile) {
                return Cached::with_expiry(local, &self.expiry.profile);
            }
        }

        let remote = self.remote_cache.get_profile(uuid).await;
        match &remote {
            None => {
                // if remote cache has no value, use local result
                Cached::with_expiry(local, &self.expiry.profile)
            }
            Some(entry) => {
                // if remote cache has a value, sync with local cache
                self.local_cache.set_profile(uuid, entry.clone()).await;
                Cached::with_expiry(remote, &self.expiry.profile)
            }
        }
    }

    /// Sets some optional [ProfileData] to the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(request_type = "profile"),
        handler = metrics_set_handler,
    )]
    pub async fn set_profile(&self, key: &Uuid, data: Option<ProfileData>) -> Entry<ProfileData> {
        let entry = Entry::from(data);
        self.local_cache.set_profile(key, entry.clone()).await;
        self.remote_cache.set_profile(key, entry.clone()).await;
        entry
    }

    /// Gets some [SkinData] from the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(request_type = "skin"),
        handler = metrics_get_handler,
    )]
    pub async fn get_skin(&self, uuid: &Uuid) -> Cached<SkinData> {
        let local = self.local_cache.get_skin(uuid).await;
        if let Some(entry) = &local {
            if !entry.is_expired(&self.expiry.skin) {
                return Cached::with_expiry(local, &self.expiry.skin);
            }
        }

        let remote = self.remote_cache.get_skin(uuid).await;
        match &remote {
            None => {
                // if remote cache has no value, use local result
                Cached::with_expiry(local, &self.expiry.skin)
            }
            Some(entry) => {
                // if remote cache has a value, sync with local cache
                self.local_cache.set_skin(uuid, entry.clone()).await;
                Cached::with_expiry(remote, &self.expiry.skin)
            }
        }
    }

    /// Sets some optional [SkinData] to the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(request_type = "profile"),
        handler = metrics_set_handler,
    )]
    pub async fn set_skin(&self, key: &Uuid, data: Option<SkinData>) -> Entry<SkinData> {
        let entry = Entry::from(data);
        self.local_cache.set_skin(key, entry.clone()).await;
        self.remote_cache.set_skin(key, entry.clone()).await;
        entry
    }

    /// Gets some [CapeData] from the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(request_type = "cape"),
        handler = metrics_get_handler,
    )]
    pub async fn get_cape(&self, uuid: &Uuid) -> Cached<CapeData> {
        let local = self.local_cache.get_cape(uuid).await;
        if let Some(entry) = &local {
            if !entry.is_expired(&self.expiry.cape) {
                return Cached::with_expiry(local, &self.expiry.cape);
            }
        }

        let remote = self.remote_cache.get_cape(uuid).await;
        match &remote {
            None => {
                // if remote cache has no value, use local result
                Cached::with_expiry(local, &self.expiry.cape)
            }
            Some(entry) => {
                // if remote cache has a value, sync with local cache
                self.local_cache.set_cape(uuid, entry.clone()).await;
                Cached::with_expiry(remote, &self.expiry.cape)
            }
        }
    }

    /// Sets some optional [CapeData] to the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(request_type = "cape"),
        handler = metrics_set_handler,
    )]
    pub async fn set_cape(&self, key: &Uuid, data: Option<CapeData>) -> Entry<CapeData> {
        let entry = Entry::from(data);
        self.local_cache.set_cape(key, entry.clone()).await;
        self.remote_cache.set_cape(key, entry.clone()).await;
        entry
    }

    /// Gets some [HeadData] from the [Cache] for a profile [Uuid] with or without its overlay.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(request_type = "head"),
        handler = metrics_get_handler,
    )]
    pub async fn get_head(&self, uuid: &(Uuid, bool)) -> Cached<HeadData> {
        let local = self.local_cache.get_head(uuid).await;
        if let Some(entry) = &local {
            if !entry.is_expired(&self.expiry.head) {
                return Cached::with_expiry(local, &self.expiry.head);
            }
        }

        let remote = self.remote_cache.get_head(uuid).await;
        match &remote {
            None => {
                // if remote cache has no value, use local result
                Cached::with_expiry(local, &self.expiry.head)
            }
            Some(entry) => {
                // if remote cache has a value, sync with local cache
                self.local_cache.set_head(uuid, entry.clone()).await;
                Cached::with_expiry(remote, &self.expiry.head)
            }
        }
    }

    /// Sets some optional [HeadData] to the [Cache] for a profile [Uuid] with or without its overlay.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(request_type = "head"),
        handler = metrics_set_handler,
    )]
    pub async fn set_head(&self, key: &(Uuid, bool), data: Option<HeadData>) -> Entry<HeadData> {
        let entry = Entry::from(data);
        self.local_cache.set_head(key, entry.clone()).await;
        self.remote_cache.set_head(key, entry.clone()).await;
        entry
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::level::moka::MokaCache;
    use crate::settings::{CacheEntries, MokaCacheEntry};
    use std::time::Duration;
    use uuid::uuid;
    use Cached::*;

    fn new_moka_settings() -> settings::MokaCache {
        let entry = MokaCacheEntry {
            cap: 10,
            ttl: Duration::from_secs(100),
            ttl_empty: Duration::from_secs(100),
            tti: Duration::from_secs(100),
            tti_empty: Duration::from_secs(100),
        };
        settings::MokaCache {
            entries: CacheEntries {
                uuid: entry.clone(),
                profile: entry.clone(),
                skin: entry.clone(),
                cape: entry.clone(),
                head: entry.clone(),
            },
        }
    }

    fn new_expiry(dur: Duration) -> CacheEntries<CacheEntry> {
        let expiry = CacheEntry {
            exp: dur,
            exp_empty: dur,
        };
        CacheEntries {
            uuid: expiry.clone(),
            profile: expiry.clone(),
            skin: expiry.clone(),
            cape: expiry.clone(),
            head: expiry.clone(),
        }
    }

    /// Creates a new cache with two levels.
    async fn new_cache_2l(dur: Duration) -> Cache<MokaCache, MokaCache> {
        Cache::new(
            new_expiry(dur),
            MokaCache::new(new_moka_settings()),
            MokaCache::new(new_moka_settings()),
        )
    }

    #[tokio::test]
    async fn set_some() {
        // given
        let cache = new_cache_2l(Duration::from_secs(10)).await;
        let data = UuidData {
            username: "Hydrofin".to_string(),
            uuid: uuid!("09879557e47945a9b434a56377674627"),
        };

        // when
        cache.set_uuid("hydrofin", Some(data.clone())).await;

        // then
        let cached1 = cache.local_cache.get_uuid("hydrofin").await;
        let cached2 = cache.remote_cache.get_uuid("hydrofin").await;

        assert!(matches!(cached1, Some(entry) if entry.data == Some(data.clone())));
        assert!(matches!(cached2, Some(entry) if entry.data == Some(data.clone())));
    }

    #[tokio::test]
    async fn set_none() {
        // given
        let cache = new_cache_2l(Duration::from_secs(10)).await;

        // when
        cache.set_uuid("hydrofin", None).await;

        // then
        let cached1 = cache.local_cache.get_uuid("hydrofin").await;
        let cached2 = cache.remote_cache.get_uuid("hydrofin").await;

        assert!(matches!(cached1, Some(entry) if entry.data.is_none()));
        assert!(matches!(cached2, Some(entry) if entry.data.is_none()));
    }

    #[tokio::test]
    async fn get_hit() {
        // given
        let cache = new_cache_2l(Duration::from_secs(10)).await;
        cache.set_uuid("hydrofin", None).await;

        // when
        let cached = cache.get_uuid("hydrofin").await;

        // then
        assert!(matches!(cached, Hit(entry) if entry.data.is_none()));
    }

    #[tokio::test]
    async fn get_expired() {
        // given
        let cache = new_cache_2l(Duration::from_secs(0)).await;
        cache.set_uuid("hydrofin", None).await;

        // when
        let cached = cache.get_uuid("hydrofin").await;

        // then
        assert!(matches!(cached, Expired(entry) if entry.data.is_none()));
    }

    #[tokio::test]
    async fn get_miss() {
        // given
        let cache = new_cache_2l(Duration::from_secs(10)).await;

        // when
        let cached = cache.get_uuid("hydrofin").await;

        // then
        assert!(matches!(cached, Miss));
    }
}
