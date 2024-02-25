use crate::cache::{
    monitor_cache_get, monitor_cache_set, Cached, HeadEntry, IntoCached, ProfileEntry, SkinEntry,
    UuidEntry, XenosCache,
};
use crate::error::XenosError;
use crate::settings;
use async_trait::async_trait;
use moka::future::Cache;
use uuid::Uuid;

// TODO add docu
#[derive(Debug)]
pub struct MokaCache {
    settings: settings::MokaCache,
    // caches TODO one cache for all?
    uuids: Cache<String, UuidEntry>,
    profiles: Cache<Uuid, ProfileEntry>,
    skins: Cache<Uuid, SkinEntry>,
    heads: Cache<(Uuid, bool), HeadEntry>,
}

impl MokaCache {
    /// Created a new empty [Moka Cache](MokaCache) with max capacity and no expiry (~585 aeons).
    /// Use successive builder methods to set expiry explicitly.
    pub fn new(settings: &settings::MokaCache) -> Self {
        Self {
            settings: settings.clone(),
            uuids: Cache::new(settings.entries.uuid.max_capacity),
            profiles: Cache::new(settings.entries.uuid.max_capacity),
            skins: Cache::new(settings.entries.uuid.max_capacity),
            heads: Cache::new(settings.entries.uuid.max_capacity),
        }
    }
}

// TODO use ttl and tti
#[async_trait]
impl XenosCache for MokaCache {
    #[tracing::instrument(skip(self))]
    async fn get_uuid_by_username(&self, username: &str) -> Result<Cached<UuidEntry>, XenosError> {
        monitor_cache_get("moka", "uuid", || async {
            let entry = self.uuids.get(username).await;
            let cached = entry.into_cached(
                &self.settings.entries.uuid.expiry,
                &self.settings.entries.uuid.expiry_missing,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_uuid_by_username(
        &self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        monitor_cache_set("moka", "uuid", || async {
            self.uuids.insert(username.to_string(), entry).await;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_profile_by_uuid(&self, uuid: &Uuid) -> Result<Cached<ProfileEntry>, XenosError> {
        monitor_cache_get("moka", "profile", || async {
            let entry = self.profiles.get(uuid).await;
            let cached = entry.into_cached(
                &self.settings.entries.profile.expiry,
                &self.settings.entries.profile.expiry_missing,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_profile_by_uuid(&self, uuid: Uuid, entry: ProfileEntry) -> Result<(), XenosError> {
        monitor_cache_set("moka", "profile", || async {
            self.profiles.insert(uuid, entry).await;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_skin_by_uuid(&self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        monitor_cache_get("moka", "skin", || async {
            let entry = self.skins.get(uuid).await;
            let cached = entry.into_cached(
                &self.settings.entries.skin.expiry,
                &self.settings.entries.skin.expiry_missing,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_skin_by_uuid(&self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        monitor_cache_set("moka", "skin", || async {
            self.skins.insert(uuid, entry).await;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_head_by_uuid(
        &self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        monitor_cache_get("moka", "head", || async {
            let entry = self.heads.get(&(*uuid, *overlay)).await;
            let cached = entry.into_cached(
                &self.settings.entries.head.expiry,
                &self.settings.entries.head.expiry_missing,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_head_by_uuid(
        &self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError> {
        monitor_cache_set("moka", "head", || async {
            self.heads.insert((uuid, *overlay), entry).await;
            Ok(())
        })
        .await
    }
}
