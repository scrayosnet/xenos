use crate::cache::entry::Dated;
use crate::cache::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use async_trait::async_trait;
use lazy_static::lazy_static;
use metrics::MetricsEvent;
use prometheus::{register_histogram_vec, HistogramVec};
use std::fmt::Debug;
use tracing::warn;
use uuid::Uuid;

pub mod moka;
pub mod no;
#[cfg(feature = "redis")]
pub mod redis;

lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// cache requests (`request_type`). Use the [monitor_get] utility for ease of use.
    static ref CACHE_LEVEL_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache get request result age in seconds. It is intended to be used by all
    /// cache requests (`request_type`). Use the [monitor_get] utility for ease of use.
    static ref CACHE_LEVEL_AGE_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_age_duration_seconds",
        "The cache get request latencies in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    ///  cache requests (`request_type`). Use the [monitor_set] utility for ease of use.
    static ref CACHE_LEVEL_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
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

fn metrics_get_handler<T: Clone + Debug + Eq>(event: MetricsEvent<Option<Entry<T>>>) {
    let label = match event.result {
        None => "miss",
        Some(Dated { data: Some(_), .. }) => "filled",
        Some(Dated { data: None, .. }) => "empty",
    };
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    CACHE_LEVEL_GET_HISTOGRAM
        .with_label_values(&[request_type, label])
        .observe(event.time);

    if let Some(dated) = event.result {
        CACHE_LEVEL_AGE_HISTOGRAM
            .with_label_values(&[request_type])
            .observe(dated.current_age() as f64);
    }
}

fn metrics_set_handler<T: Clone + Debug + Eq>(event: MetricsEvent<T>) {
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    CACHE_LEVEL_SET_HISTOGRAM
        .with_label_values(&[request_type])
        .observe(event.time);
}
