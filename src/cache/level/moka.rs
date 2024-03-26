use crate::cache::entry::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::CacheLevel;
use crate::cache::level::{monitor_get, monitor_set};
use crate::settings;
use async_trait::async_trait;
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

#[async_trait]
impl CacheLevel for MokaCache {
    #[tracing::instrument(skip(self))]
    async fn get_uuid(&self, username: &str) -> Option<Entry<UuidData>> {
        monitor_get("moka", "uuid", || self.uuids.get(username)).await
    }

    #[tracing::instrument(skip(self))]
    async fn set_uuid(&self, username: String, entry: Entry<UuidData>) {
        monitor_set("moka", "uuid", || self.uuids.insert(username, entry)).await
    }

    #[tracing::instrument(skip(self))]
    async fn get_profile(&self, uuid: &Uuid) -> Option<Entry<ProfileData>> {
        monitor_get("moka", "profile", || self.profiles.get(uuid)).await
    }

    #[tracing::instrument(skip(self))]
    async fn set_profile(&self, uuid: Uuid, entry: Entry<ProfileData>) {
        monitor_set("moka", "profile", || self.profiles.insert(uuid, entry)).await
    }

    #[tracing::instrument(skip(self))]
    async fn get_skin(&self, uuid: &Uuid) -> Option<Entry<SkinData>> {
        monitor_get("moka", "skin", || self.skins.get(uuid)).await
    }

    #[tracing::instrument(skip(self))]
    async fn set_skin(&self, uuid: Uuid, entry: Entry<SkinData>) {
        monitor_set("moka", "skin", || self.skins.insert(uuid, entry)).await
    }

    #[tracing::instrument(skip(self))]
    async fn get_cape(&self, uuid: &Uuid) -> Option<Entry<CapeData>> {
        monitor_get("moka", "cape", || self.capes.get(uuid)).await
    }

    #[tracing::instrument(skip(self))]
    async fn set_cape(&self, uuid: Uuid, entry: Entry<CapeData>) {
        monitor_set("moka", "cape", || self.capes.insert(uuid, entry)).await
    }

    #[tracing::instrument(skip(self))]
    async fn get_head(&self, uuid: &Uuid, overlay: bool) -> Option<Entry<HeadData>> {
        let key = (*uuid, overlay);
        monitor_get("moka", "head", || self.heads.get(&key)).await
    }

    #[tracing::instrument(skip(self))]
    async fn set_head(&self, uuid: Uuid, overlay: bool, entry: Entry<HeadData>) {
        monitor_set("moka", "head", || self.heads.insert((uuid, overlay), entry)).await
    }
}
