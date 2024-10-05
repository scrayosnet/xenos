use crate::cache::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use async_trait::async_trait;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use std::fmt::Debug;
use std::future::Future;
use std::time::Instant;
use uuid::Uuid;

pub mod moka;
pub mod no;
#[cfg(feature = "redis")]
pub mod redis;

lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [monitor_get]
    /// utility for ease of use.
    pub static ref CACHE_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["cache_variant", "request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [monitor_set]
    /// utility for ease of use.
    pub static ref CACHE_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["cache_variant", "request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

// TODO do with macro
/// Monitors a cache get operation, tracking its runtime and response.
async fn monitor_set<F, Fut>(cache_variant: &str, request_type: &str, f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let start = Instant::now();
    let result = f().await;
    CACHE_SET_HISTOGRAM
        .with_label_values(&[cache_variant, request_type])
        .observe(start.elapsed().as_secs_f64());
    result
}

// TODO do with macro
/// Monitors a cache set operation, tracking its runtime and response.
async fn monitor_get<F, Fut, D>(cache_variant: &str, request_type: &str, f: F) -> Option<Entry<D>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Option<Entry<D>>>,
    D: Clone + Debug + Eq + PartialEq,
{
    let start = Instant::now();
    let result = f().await;
    let cache_result = match &result {
        Some(entry) if entry.data.is_some() => "found",
        Some(_) => "not_found",
        None => "miss",
    };
    CACHE_GET_HISTOGRAM
        .with_label_values(&[cache_variant, request_type, cache_result])
        .observe(start.elapsed().as_secs_f64());
    result
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
#[async_trait]
pub trait CacheLevel: Debug + Send + Sync {
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
