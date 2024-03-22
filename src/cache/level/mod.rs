use crate::cache::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use async_trait::async_trait;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use std::fmt::Debug;
use std::future::Future;
use std::time::Instant;
use uuid::Uuid;

pub mod moka;
pub mod redis;

lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [crate::cache::monitor::monitor_get]
    /// utility for ease of use.
    pub static ref CACHE_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["cache_variant", "request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [crate::cache::monitor::monitor_set]
    /// utility for ease of use.
    pub static ref CACHE_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_level_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["cache_variant", "request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

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

#[async_trait]
pub trait CacheLevel: Debug + Send + Sync {
    async fn get_uuid(&self, username: &str) -> Option<Entry<UuidData>>;
    async fn set_uuid(&self, username: String, entry: Entry<UuidData>);
    async fn get_profile(&self, uuid: &Uuid) -> Option<Entry<ProfileData>>;
    async fn set_profile(&self, uuid: Uuid, entry: Entry<ProfileData>);
    async fn get_skin(&self, uuid: &Uuid) -> Option<Entry<SkinData>>;
    async fn set_skin(&self, uuid: Uuid, entry: Entry<SkinData>);
    async fn get_cape(&self, uuid: &Uuid) -> Option<Entry<CapeData>>;
    async fn set_cape(&self, uuid: Uuid, entry: Entry<CapeData>);
    async fn get_head(&self, uuid: &Uuid, overlay: bool) -> Option<Entry<HeadData>>;
    async fn set_head(&self, uuid: Uuid, overlay: bool, entry: Entry<HeadData>);
}
