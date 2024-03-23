//! The cache module provides multiple [cache](XenosCache) implementations for the xenos service.

pub mod entry;
pub mod level;

use crate::cache::entry::Cached::{Expired, Hit, Miss};
use crate::cache::entry::{Cached, CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::CacheLevel;
use crate::settings;
use crate::settings::CacheEntry;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

// TODO update buckets
lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// cache requests (`request_type`). Use the [monitor_get] utility for ease of use.
    static ref CACHE_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    ///  cache requests (`request_type`). Use the [monitor_set] utility for ease of use.
    static ref CACHE_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

/// Monitors a cache get operation, tracking its runtime and response.
async fn monitor_get<F, Fut, D>(request_type: &str, f: F) -> Cached<D>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Cached<D>>,
    D: Clone + Debug + Eq + PartialEq,
{
    let start = Instant::now();
    let result = f().await;
    let cache_result = match &result {
        Hit(_) => "hit",
        Expired(_) => "expired",
        Miss => "miss",
    };
    CACHE_GET_HISTOGRAM
        .with_label_values(&[request_type, cache_result])
        .observe(start.elapsed().as_secs_f64());
    result
}

/// Monitors a cache set operation, tracking its runtime and response.
async fn monitor_set<F, Fut, D>(request_type: &str, f: F) -> Entry<D>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Entry<D>>,
    D: Clone + Debug + Eq + PartialEq,
{
    let start = Instant::now();
    let result = f().await;
    CACHE_SET_HISTOGRAM
        .with_label_values(&[request_type])
        .observe(start.elapsed().as_secs_f64());
    result
}

/// A [Cache] is a thread-safe multi-level cache. [Levels](CacheLevel) are added to the end of the stack.
/// That means that the last added level is the lowest level. In general, the lower level caches should be
/// remote/persistent caches while the upper level caches should be fast in-memory caches. Also,
/// upper level caches should be subsets of lower level caches.
///
/// - **Get operations** find the first [CacheLevel] that contains a some [Entry].
/// When a [Hit] is found, all previous levels are updated with that [Entry]. Otherwise, it uses the
/// last found [Expired] entry. If no [Entry] could be found. Nothing is updated.
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
pub struct Cache {
    expiry: settings::CacheEntries<CacheEntry>,
    levels: Vec<Arc<dyn CacheLevel>>,
}

impl Cache {
    /// Creates a new [Cache] with no inner caches.
    pub fn new(expiry: settings::CacheEntries<CacheEntry>) -> Self {
        Cache {
            expiry,
            levels: vec![],
        }
    }

    /// Pushes an optional cache level to the end of the cache levels (as the lowest level).
    pub async fn add_level<F, Fut, E>(mut self, enabled: bool, f: F) -> Result<Self, E>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Arc<dyn CacheLevel>, E>>,
    {
        if enabled {
            self.levels.push(f().await?);
        }
        Ok(self)
    }

    /// Utility for getting an [Entry] from the cache levels. Also updates cache levels appropriately.
    async fn get<'a, D, G, GF, S, SF>(
        &'a self,
        expiry: &CacheEntry,
        getter: G,
        setter: S,
    ) -> Cached<D>
    where
        G: Fn(&'a dyn CacheLevel) -> GF,
        GF: Future<Output = Option<Entry<D>>>,
        S: Fn(&'a dyn CacheLevel, Entry<D>) -> SF,
        SF: Future<Output = ()>,
        D: Clone + Debug + Eq + PartialEq,
    {
        let mut depth = 0;
        let mut result = Miss;

        // try to find cache hit falling back to expired, noting its depth
        for i in 0..self.levels.len() {
            // get entry from cache
            let opt = getter(self.levels[depth].as_ref()).await;
            let cached = Cached::with_expiry(opt, expiry);
            match cached {
                Hit(entry) => {
                    result = Hit(entry);
                    depth = i;
                    break;
                }
                Expired(entry) => {
                    result = Expired(entry);
                }
                Miss => {}
            };
        }

        // update upper caches, ensuring the consistency invariant
        match &result {
            Hit(entry) | Expired(entry) => {
                for i in (0..depth).rev() {
                    setter(self.levels[i].as_ref(), Entry::clone(entry)).await;
                }
            }
            _ => {}
        };
        result
    }

    /// Utility for settings an [Entry] to all cache levels.
    async fn set<'a, D, S, SF>(&'a self, data: Option<D>, setter: S) -> Entry<D>
    where
        S: Fn(&'a dyn CacheLevel, Entry<D>) -> SF,
        SF: Future<Output = ()>,
        D: Clone + Debug + Eq + PartialEq,
    {
        // fix entry timestamp to be the same for all caches
        let entry = Entry::from(data);

        // update lower caches first, ensuring the consistency invariant
        for cache in self.levels.iter().rev() {
            setter(cache.as_ref(), Entry::clone(&entry)).await;
        }
        entry
    }

    /// Gets some [UuidData] from the [Cache] for a case-insensitive username.
    #[tracing::instrument(skip(self))]
    pub async fn get_uuid(&self, username: &str) -> Cached<UuidData> {
        monitor_get("uuid", || {
            self.get(
                &self.expiry.uuid,
                |level| level.get_uuid(username),
                |level, entry| level.set_uuid(username.to_string(), entry),
            )
        })
        .await
    }

    /// Sets some optional [UuidData] to the [Cache] for a case-insensitive username.
    #[tracing::instrument(skip(self))]
    pub async fn set_uuid(&self, username: &str, data: Option<UuidData>) -> Entry<UuidData> {
        monitor_set("uuid", || {
            self.set(data, |level, entry| {
                level.set_uuid(username.to_string(), entry)
            })
        })
        .await
    }

    /// Gets some [ProfileData] from the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    pub async fn get_profile(&self, uuid: &Uuid) -> Cached<ProfileData> {
        monitor_get("profile", || {
            self.get(
                &self.expiry.profile,
                |level| level.get_profile(uuid),
                |level, entry| level.set_profile(*uuid, entry),
            )
        })
        .await
    }

    /// Sets some optional [ProfileData] to the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    pub async fn set_profile(&self, uuid: Uuid, data: Option<ProfileData>) -> Entry<ProfileData> {
        monitor_set("profile", || {
            self.set(data, |level, entry| level.set_profile(uuid, entry))
        })
        .await
    }

    /// Gets some [SkinData] from the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    pub async fn get_skin(&self, uuid: &Uuid) -> Cached<SkinData> {
        monitor_get("skin", || {
            self.get(
                &self.expiry.skin,
                |level| level.get_skin(uuid),
                |level, entry| level.set_skin(*uuid, entry),
            )
        })
        .await
    }

    /// Sets some optional [SkinData] to the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    pub async fn set_skin(&self, uuid: Uuid, data: Option<SkinData>) -> Entry<SkinData> {
        monitor_set("skin", || {
            self.set(data, |level, entry| level.set_skin(uuid, entry))
        })
        .await
    }

    /// Gets some [CapeData] from the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    pub async fn get_cape(&self, uuid: &Uuid) -> Cached<CapeData> {
        monitor_get("cape", || {
            self.get(
                &self.expiry.cape,
                |level| level.get_cape(uuid),
                |level, entry| level.set_cape(*uuid, entry),
            )
        })
        .await
    }

    /// Sets some optional [CapeData] to the [Cache] for a profile [Uuid].
    #[tracing::instrument(skip(self))]
    pub async fn set_cape(&self, uuid: Uuid, data: Option<CapeData>) -> Entry<CapeData> {
        monitor_set("cape", || {
            self.set(data, |level, entry| level.set_cape(uuid, entry))
        })
        .await
    }

    /// Gets some [HeadData] from the [Cache] for a profile [Uuid] with or without its overlay.
    #[tracing::instrument(skip(self))]
    pub async fn get_head(&self, uuid: &Uuid, overlay: bool) -> Cached<HeadData> {
        monitor_get("head", || {
            self.get(
                &self.expiry.head,
                |level| level.get_head(uuid, overlay),
                |level, entry| level.set_head(*uuid, overlay, entry),
            )
        })
        .await
    }

    /// Sets some optional [HeadData] to the [Cache] for a profile [Uuid] with or without its overlay.
    #[tracing::instrument(skip(self))]
    pub async fn set_head(
        &self,
        uuid: Uuid,
        overlay: bool,
        data: Option<HeadData>,
    ) -> Entry<HeadData> {
        monitor_set("head", || {
            self.set(data, |level, entry| level.set_head(uuid, overlay, entry))
        })
        .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::level::moka::MokaCache;
    use crate::settings::{CacheEntries, MokaCacheEntry};
    use std::sync::Arc;
    use std::time::Duration;
    use uuid::uuid;

    fn new_moka_settings() -> settings::MokaCache {
        let entry = MokaCacheEntry {
            cap: 10,
            ttl: Duration::from_secs(100),
            ttl_na: Duration::from_secs(100),
            tti: Duration::from_secs(100),
            tti_na: Duration::from_secs(100),
        };
        settings::MokaCache {
            enabled: false,
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
            exp_na: dur,
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
    async fn new_cache_2l(dur: Duration) -> Cache {
        let new_moka = || async {
            let l1: Arc<dyn CacheLevel> = Arc::new(MokaCache::new(new_moka_settings()));
            Ok::<_, ()>(l1)
        };

        Cache::new(new_expiry(dur))
            .add_level(true, new_moka)
            .await
            .unwrap()
            .add_level(true, new_moka)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn new_cache() {
        // given
        let l1: Arc<dyn CacheLevel> = Arc::new(MokaCache::new(new_moka_settings()));
        let l2: Arc<dyn CacheLevel> = Arc::new(MokaCache::new(new_moka_settings()));

        // when
        let cache = Cache::new(new_expiry(Duration::from_secs(10)))
            .add_level(false, || async { Err(()) })
            .await
            .expect("factory 1 should not be called")
            .add_level(true, || async { Ok::<_, ()>(Arc::clone(&l1)) })
            .await
            .expect("factory 2 should not fail")
            .add_level(false, || async { Err(()) })
            .await
            .expect("factory 3 should not be called")
            .add_level(true, || async { Ok::<_, ()>(Arc::clone(&l2)) })
            .await
            .expect("factory 4 should not fail");

        // then
        assert_eq!(2, cache.levels.len());
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
        let cached1 = cache.levels[0].get_uuid("hydrofin").await;
        let cached2 = cache.levels[1].get_uuid("hydrofin").await;

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
        let cached1 = cache.levels[0].get_uuid("hydrofin").await;
        let cached2 = cache.levels[1].get_uuid("hydrofin").await;

        assert!(matches!(cached1, Some(entry) if entry.data == None));
        assert!(matches!(cached2, Some(entry) if entry.data == None));
    }

    #[tokio::test]
    async fn get_hit() {
        // given
        let cache = new_cache_2l(Duration::from_secs(10)).await;
        cache.set_uuid("hydrofin", None).await;

        // when
        let cached = cache.get_uuid("hydrofin").await;

        // then
        assert!(matches!(cached, Hit(entry) if entry.data == None));
    }

    #[tokio::test]
    async fn get_expired() {
        // given
        let cache = new_cache_2l(Duration::from_secs(0)).await;
        cache.set_uuid("hydrofin", None).await;

        // when
        let cached = cache.get_uuid("hydrofin").await;

        // then
        assert!(matches!(cached, Expired(entry) if entry.data == None));
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
