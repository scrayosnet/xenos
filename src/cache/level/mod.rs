use crate::cache::entry::Dated;
use crate::cache::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::metrics::{
    CacheAgeLabels, CacheGetLabels, CacheSetLabels, CACHE_AGE, CACHE_GET, CACHE_SET,
};
use metrics::MetricsEvent;
use std::fmt::Debug;
use tracing::warn;
use uuid::Uuid;

pub mod moka;
pub mod no;
#[cfg(feature = "redis")]
pub mod redis;

fn metrics_get_handler<T: Clone + Debug + Eq>(event: MetricsEvent<Option<Entry<T>>>) {
    let cache_result = match event.result {
        None => "miss",
        Some(Dated { data: Some(_), .. }) => "filled",
        Some(Dated { data: None, .. }) => "empty",
    };
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    let Some(cache_variant) = event.labels.get("cache_variant") else {
        warn!("Failed to retrieve label 'cache_variant' for metric!");
        return;
    };
    CACHE_GET
        .get_or_create(&CacheGetLabels {
            cache_variant,
            request_type,
            cache_result,
        })
        .observe(event.time);

    if let Some(entry) = event.result {
        CACHE_AGE
            .get_or_create(&CacheAgeLabels {
                cache_variant,
                request_type,
            })
            .observe(entry.current_age() as f64);
    }
}

fn metrics_set_handler<T: Clone + Debug + Eq>(event: MetricsEvent<T>) {
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    let Some(cache_variant) = event.labels.get("cache_variant") else {
        warn!("Failed to retrieve label 'cache_variant' for metric!");
        return;
    };
    CACHE_SET
        .get_or_create(&CacheSetLabels {
            cache_variant,
            request_type,
        })
        .observe(event.time);
}

/// A [CacheLevel] is a thread-safe cache level of a multi-level cache.
///
/// ```rs
/// let cached: Option<Entry<UuidData>> = cache_level.get_uuid("HydOFin");
/// match cached {
///   // cache contains entry for username
///   // the entry itself might be expired or contain no data
///   Some(entry) => { ... }
///   // cache contains NO entry for username
///   None => { ... }
/// }
/// ```

#[trait_variant::make(CacheLevel: Send)]
pub trait LocalCacheLevel {
    /// Gets some [UuidData] from the [CacheLevel] for a case-insensitive username.
    async fn get_uuid(&self, key: &str) -> Option<Entry<UuidData>>;

    /// Sets some optional [UuidData] to the [CacheLevel] for a case-insensitive username.
    async fn set_uuid(&self, key: &str, entry: Entry<UuidData>);

    /// Gets some [ProfileData] from the [CacheLevel] for a profile [Uuid].
    async fn get_profile(&self, key: &Uuid) -> Option<Entry<ProfileData>>;

    /// Sets some optional [ProfileData] to the [CacheLevel] for a profile [Uuid].
    async fn set_profile(&self, key: &Uuid, entry: Entry<ProfileData>);

    /// Gets some [SkinData] from the [CacheLevel] for a profile [Uuid].
    async fn get_skin(&self, key: &Uuid) -> Option<Entry<SkinData>>;

    /// Sets some optional [SkinData] to the [CacheLevel] for a profile [Uuid].
    async fn set_skin(&self, key: &Uuid, entry: Entry<SkinData>);

    /// Gets some [CapeData] from the [CacheLevel] for a profile [Uuid].
    async fn get_cape(&self, key: &Uuid) -> Option<Entry<CapeData>>;

    /// Sets some optional [CapeData] to the [CacheLevel] for a profile [Uuid].
    async fn set_cape(&self, key: &Uuid, entry: Entry<CapeData>);

    /// Gets some [HeadData] from the [CacheLevel] for a profile [Uuid] with or without its overlay.
    async fn get_head(&self, key: &(Uuid, bool)) -> Option<Entry<HeadData>>;

    /// Sets some optional [HeadData] to the [CacheLevel] for a profile [Uuid] with or without its overlay.
    async fn set_head(&self, key: &(Uuid, bool), entry: Entry<HeadData>);
}
