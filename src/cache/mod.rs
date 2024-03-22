//! The cache module provides multiple [cache](XenosCache) implementations for the xenos service.

pub mod entry;
pub mod level;

use crate::cache::entry::Cached::{Expired, Hit, Miss};
use crate::cache::entry::{Cached, CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::CacheLevel;
use crate::error::XenosError;
use crate::settings;
use crate::settings::Expiry;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use std::fmt::Debug;
use std::future::Future;
use std::time::Instant;
use uuid::Uuid;

// TODO update buckets
lazy_static! {
    /// A histogram for the cache get request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [monitor_get]
    /// utility for ease of use.
    static ref CACHE_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_get_duration_seconds",
        "The cache get request latencies in seconds.",
        &["request_type", "cache_result"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();

    /// A histogram for the cache set request latencies in seconds. It is intended to be used by all
    /// caches (`cache_variant`) and cache requests (`request_type`). Use the [monitor_set]
    /// utility for ease of use.
    static ref CACHE_SET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_cache_set_duration_seconds",
        "The cache set request latencies in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

async fn monitor_set<F, Fut, D>(request_type: &str, f: F) -> Entry<D>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Entry<D>>,
    D: Clone + Debug + Eq + PartialEq,
{
    let start = Instant::now();
    let result = f().await;
    // TODO monitor more?
    CACHE_SET_HISTOGRAM
        .with_label_values(&[request_type])
        .observe(start.elapsed().as_secs_f64());
    result
}

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
    // TODO monitor more?
    CACHE_GET_HISTOGRAM
        .with_label_values(&[request_type, cache_result])
        .observe(start.elapsed().as_secs_f64());
    result
}

pub struct Cache {
    expiry: settings::CacheEntries<Expiry>,
    levels: Vec<Box<dyn CacheLevel>>,
}

impl Cache {
    /// Creates a new [Cache] with no inner caches.
    pub fn new(expiry: settings::CacheEntries<Expiry>) -> Self {
        Cache {
            expiry,
            levels: vec![],
        }
    }

    /// Pushes an optional cache to the end of the inner caches (the last layer).
    pub async fn add_level<F, Fut>(mut self, enabled: bool, f: F) -> Result<Self, XenosError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Box<dyn CacheLevel>, XenosError>>,
    {
        if enabled {
            self.levels.push(f().await?);
        }
        Ok(self)
    }

    async fn get<'a, D, G, GF, S, SF>(&'a self, expiry: &Expiry, getter: G, setter: S) -> Cached<D>
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

    #[tracing::instrument(skip(self))]
    pub async fn set_uuid(&self, username: &str, data: Option<UuidData>) -> Entry<UuidData> {
        monitor_set("uuid", || {
            self.set(data, |level, entry| {
                level.set_uuid(username.to_string(), entry)
            })
        })
        .await
    }

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

    #[tracing::instrument(skip(self))]
    pub async fn set_profile(&self, uuid: Uuid, data: Option<ProfileData>) -> Entry<ProfileData> {
        monitor_set("profile", || {
            self.set(data, |level, entry| level.set_profile(uuid, entry))
        })
        .await
    }

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

    #[tracing::instrument(skip(self))]
    pub async fn set_skin(&self, uuid: Uuid, data: Option<SkinData>) -> Entry<SkinData> {
        monitor_set("skin", || {
            self.set(data, |level, entry| level.set_skin(uuid, entry))
        })
        .await
    }

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

    #[tracing::instrument(skip(self))]
    pub async fn set_cape(&self, uuid: Uuid, data: Option<CapeData>) -> Entry<CapeData> {
        monitor_set("cape", || {
            self.set(data, |level, entry| level.set_cape(uuid, entry))
        })
        .await
    }

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
