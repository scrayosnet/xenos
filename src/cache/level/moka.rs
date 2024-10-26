use crate::cache::entry::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::{metrics_get_handler, metrics_set_handler, CacheLevel};
use crate::settings;
use moka::future::Cache;
use uuid::Uuid;

/// [Moka Cache](MokaCache) is a [CacheLevel] implementation using moka. It is a thread-safe,
/// futures-aware concurrent in-memory cache. The cache has a configurable maximum capacity and additional
/// expiration (delete) policies with time-to-live and time-to-idle.
#[derive(Debug)]
pub struct MokaCache {
    #[allow(dead_code)] // will be used in the future for per-element ttl/tti
    settings: settings::MokaCache,
    // caches
    uuids: Cache<String, Entry<UuidData>>,
    profiles: Cache<Uuid, Entry<ProfileData>>,
    skins: Cache<Uuid, Entry<SkinData>>,
    capes: Cache<Uuid, Entry<CapeData>>,
    heads: Cache<(Uuid, bool), Entry<HeadData>>,
}

impl MokaCache {
    pub fn new(settings: settings::MokaCache) -> Self {
        Self {
            settings: settings.clone(),
            uuids: Cache::builder()
                .max_capacity(settings.entries.uuid.cap)
                .time_to_live(settings.entries.uuid.ttl)
                .time_to_idle(settings.entries.uuid.tti)
                .build(),
            profiles: Cache::builder()
                .max_capacity(settings.entries.profile.cap)
                .time_to_live(settings.entries.profile.ttl)
                .time_to_idle(settings.entries.profile.tti)
                .build(),
            skins: Cache::builder()
                .max_capacity(settings.entries.skin.cap)
                .time_to_live(settings.entries.skin.ttl)
                .time_to_idle(settings.entries.skin.tti)
                .build(),
            capes: Cache::builder()
                .max_capacity(settings.entries.cape.cap)
                .time_to_live(settings.entries.cape.ttl)
                .time_to_idle(settings.entries.cape.tti)
                .build(),
            heads: Cache::builder()
                .max_capacity(settings.entries.head.cap)
                .time_to_live(settings.entries.head.ttl)
                .time_to_idle(settings.entries.head.tti)
                .build(),
        }
    }
}

impl CacheLevel for MokaCache {
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_type = "moka", request_type = "uuid"),
        handler = metrics_get_handler
    )]
    async fn get_uuid(&self, key: &str) -> Option<Entry<UuidData>> {
        self.uuids.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_type = "moka", request_type = "uuid"),
        handler = metrics_set_handler
    )]
    async fn set_uuid(&self, key: &str, entry: Entry<UuidData>) {
        self.uuids.insert(key.to_string(), entry).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_type = "moka", request_type = "profile"),
        handler = metrics_get_handler
    )]
    async fn get_profile(&self, key: &Uuid) -> Option<Entry<ProfileData>> {
        self.profiles.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_type = "moka", request_type = "profile"),
        handler = metrics_set_handler
    )]
    async fn set_profile(&self, key: &Uuid, entry: Entry<ProfileData>) {
        self.profiles.insert(*key, entry).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_type = "moka", request_type = "skin"),
        handler = metrics_get_handler
    )]
    async fn get_skin(&self, key: &Uuid) -> Option<Entry<SkinData>> {
        self.skins.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_type = "moka", request_type = "skin"),
        handler = metrics_set_handler
    )]
    async fn set_skin(&self, key: &Uuid, entry: Entry<SkinData>) {
        self.skins.insert(*key, entry).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_type = "moka", request_type = "cape"),
        handler = metrics_get_handler
    )]
    async fn get_cape(&self, key: &Uuid) -> Option<Entry<CapeData>> {
        self.capes.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_type = "moka", request_type = "cape"),
        handler = metrics_set_handler
    )]
    async fn set_cape(&self, uuid: &Uuid, entry: Entry<CapeData>) {
        self.capes.insert(*uuid, entry).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_type = "moka", request_type = "head"),
        handler = metrics_get_handler
    )]
    async fn get_head(&self, key: &(Uuid, bool)) -> Option<Entry<HeadData>> {
        self.heads.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_type = "moka", request_type = "head"),
        handler = metrics_set_handler
    )]
    async fn set_head(&self, key: &(Uuid, bool), entry: Entry<HeadData>) {
        self.heads.insert(*key, entry).await
    }
}
