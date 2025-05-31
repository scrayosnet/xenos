use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::Registry;
use std::sync::{Arc, LazyLock};

pub(crate) type HistogramFamily<T> = Family<T, Histogram, fn() -> Histogram>;

/// The application metrics registry.
pub(crate) static REGISTRY: LazyLock<Arc<Registry>> = LazyLock::new(build_registry);

/// A counter for the number of requests.
pub(crate) static REQUEST: LazyLock<Family<RequestsLabels, Counter>> =
    LazyLock::new(Family::<RequestsLabels, Counter>::default);

/// A histogram for the age in seconds of cache results.
pub(crate) static PROFILE_REQ_AGE: LazyLock<HistogramFamily<ProfileAgeLabels>> =
    LazyLock::new(|| {
        HistogramFamily::<ProfileAgeLabels>::new_with_constructor(|| {
            Histogram::new([5.0, 10.0, 60.0, 600.0, 3600.0, 86400.0, 604800.0, 2419200.0])
        })
    });

/// A histogram for the latency in seconds of cache results.
pub(crate) static PROFILE_REQ_LAT: LazyLock<HistogramFamily<ProfileLatLabels>> =
    LazyLock::new(|| {
        HistogramFamily::<ProfileLatLabels>::new_with_constructor(|| {
            Histogram::new([
                0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.175, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0,
            ])
        })
    });

/// A histogram for the mojang request status and request latencies in seconds.
pub(crate) static MOJANG_REQ_LAT: LazyLock<HistogramFamily<MojangLatLabels>> =
    LazyLock::new(|| {
        HistogramFamily::<MojangLatLabels>::new_with_constructor(|| {
            Histogram::new([0.05, 0.1, 0.175, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0])
        })
    });

/// A counter for the mojang request status.
pub(crate) static MOJANG_REQ: LazyLock<Family<MojangReqLabels, Counter>> =
    LazyLock::new(Family::<MojangReqLabels, Counter>::default);

/// A histogram for the cache get-request latencies in seconds.
pub(crate) static CACHE_GET: LazyLock<HistogramFamily<CacheGetLabels>> = LazyLock::new(|| {
    HistogramFamily::<CacheGetLabels>::new_with_constructor(|| {
        Histogram::new([
            0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.175, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0,
        ])
    })
});

/// A histogram for the cache get-request age in seconds.
pub(crate) static CACHE_AGE: LazyLock<HistogramFamily<CacheAgeLabels>> = LazyLock::new(|| {
    HistogramFamily::<CacheAgeLabels>::new_with_constructor(|| {
        Histogram::new([5.0, 10.0, 60.0, 600.0, 3600.0, 86400.0, 604800.0, 2419200.0])
    })
});

/// A histogram for the cache set-request latency in seconds.
pub(crate) static CACHE_SET: LazyLock<HistogramFamily<CacheSetLabels>> = LazyLock::new(|| {
    HistogramFamily::<CacheSetLabels>::new_with_constructor(|| {
        Histogram::new([
            0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.175, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0,
        ])
    })
});

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RequestsLabels {
    pub request_type: &'static str,
    pub handler: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ProfileAgeLabels {
    pub request_type: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ProfileLatLabels {
    pub request_type: &'static str,
    pub status: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MojangLatLabels {
    pub request_type: &'static str,
    pub status: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MojangReqLabels {
    pub request_type: &'static str,
    pub status: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct CacheGetLabels {
    pub cache_variant: &'static str,
    pub request_type: &'static str,
    pub cache_result: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct CacheAgeLabels {
    pub cache_variant: &'static str,
    pub request_type: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct CacheSetLabels {
    pub cache_variant: &'static str,
    pub request_type: &'static str,
}

fn build_registry() -> Arc<Registry> {
    let mut registry = Registry::with_prefix("xenos");

    registry.register(
        "requests",
        "The total number of (external) requests to the server.",
        REQUEST.clone(),
    );

    registry.register(
        "profile_age_seconds",
        "The profile response age in seconds.",
        PROFILE_REQ_AGE.clone(),
    );

    registry.register(
        "profile_latency_seconds",
        "The profile response latency in seconds.",
        PROFILE_REQ_LAT.clone(),
    );

    registry.register(
        "mojang_request_duration_seconds",
        "The mojang request latencies in seconds.",
        MOJANG_REQ_LAT.clone(),
    );

    registry.register(
        "mojang_request_status",
        "The mojang request status.",
        MOJANG_REQ.clone(),
    );

    registry.register(
        "cache_get_duration_seconds",
        "The cache get request latencies in seconds.",
        CACHE_GET.clone(),
    );

    registry.register(
        "cache_age_duration_seconds",
        "The cache get response age in seconds.",
        CACHE_AGE.clone(),
    );

    registry.register(
        "cache_set_duration_seconds",
        "The cache set request latencies in seconds.",
        CACHE_SET.clone(),
    );

    Arc::new(registry)
}
