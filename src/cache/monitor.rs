use crate::cache::Cached;
use crate::cache::Cached::{Expired, Hit, Miss};
use crate::error::XenosError;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use std::future::Future;
use std::time::Instant;

// TODO update buckets
lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [monitor_cache_get]
    /// utility for ease of use.
    pub static ref CACHE_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["cache_variant", "request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [monitor_cache_set]
    /// utility for ease of use.
    pub static ref CACHE_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["cache_variant", "request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

/// Monitors the inner function for getting a [cache](XenosCache) entry.
pub async fn monitor_cache_set<F, Fut>(
    cache_variant: &str,
    request_type: &str,
    f: F,
) -> Result<(), XenosError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), XenosError>>,
{
    let start = Instant::now();
    let result = f().await;
    match &result {
        Ok(_) => {
            CACHE_SET_HISTOGRAM
                .with_label_values(&[cache_variant, request_type, "ok"])
                .observe(start.elapsed().as_secs_f64());
        }
        Err(_) => {
            CACHE_SET_HISTOGRAM
                .with_label_values(&[cache_variant, request_type, "err"])
                .observe(start.elapsed().as_secs_f64());
        }
    };
    result
}

/// Monitors the inner function for setting a [cache](XenosCache) entry.
pub async fn monitor_cache_get<F, Fut, E>(
    cache_variant: &str,
    request_type: &str,
    f: F,
) -> Result<Cached<E>, XenosError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Cached<E>, XenosError>>,
{
    let start = Instant::now();
    let result = f().await;
    let cache_result = match &result {
        Ok(Expired(_)) => "expired",
        Ok(Hit(_)) => "hit",
        Ok(Miss) => "miss",
        Err(_) => "error",
    };
    CACHE_GET_HISTOGRAM
        .with_label_values(&[cache_variant, request_type, cache_result])
        .observe(start.elapsed().as_secs_f64());
    result
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::CacheEntry;
    use std::assert_matches::assert_matches;

    type VoidEntry = CacheEntry<()>;

    #[tokio::test]
    async fn get_err() {
        // given

        // when
        let monitored: Result<Cached<VoidEntry>, XenosError> =
            monitor_cache_get("", "", || async { Err(XenosError::NotFound) }).await;

        // then
        assert_matches!(monitored, Err(XenosError::NotFound));
    }

    #[tokio::test]
    async fn get_expired() {
        // given
        let entry = Expired(VoidEntry::new_empty());

        // when
        let monitored = monitor_cache_get("test", "test", || async { Ok(entry.clone()) }).await;

        // then
        assert_matches!(monitored, Ok(e) if e == entry);
    }

    #[tokio::test]
    async fn get_hit() {
        // given
        let entry = Hit(VoidEntry::new_empty());

        // when
        let monitored = monitor_cache_get("test", "test", || async { Ok(entry.clone()) }).await;

        // then
        assert_matches!(monitored, Ok(e) if e == entry);
    }

    #[tokio::test]
    async fn get_miss() {
        // given
        let entry: Cached<VoidEntry> = Miss;

        // when
        let monitored = monitor_cache_get("test", "test", || async { Ok(entry.clone()) }).await;

        // then
        assert_matches!(monitored, Ok(e) if e == entry);
    }

    #[tokio::test]
    async fn set_err() {
        // given

        // when
        let monitored =
            monitor_cache_set("test", "test", || async { Err(XenosError::NotFound) }).await;

        // then
        assert_matches!(monitored, Err(XenosError::NotFound));
    }
}
