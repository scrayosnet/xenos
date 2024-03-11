use crate::cache::{
    monitor_cache_get, monitor_cache_set, Cached, CapeEntry, HeadEntry, IntoCached, ProfileEntry,
    SkinEntry, UuidEntry, XenosCache,
};
use crate::error::XenosError;
use crate::settings;
use async_trait::async_trait;
use moka::future::Cache;
use uuid::Uuid;

/// [Moka Cache](MokaCache) is a [cache](XenosCache) implementation using moka. It is a thread-safe,
/// futures-aware concurrent in-memory cache. The cache has a configurable maximum capacity and additional
/// expiration (delete) policies with time-to-live and time-to-idle.
#[derive(Debug)]
pub struct MokaCache {
    settings: settings::MokaCache,
    // caches
    uuids: Cache<String, UuidEntry>,
    profiles: Cache<Uuid, ProfileEntry>,
    skins: Cache<Uuid, SkinEntry>,
    capes: Cache<Uuid, CapeEntry>,
    heads: Cache<(Uuid, bool), HeadEntry>,
}

impl MokaCache {
    /// Created a new empty [Moka Cache](MokaCache) with max capacity and no expiry (~585 aeons).
    /// Use successive builder methods to set expiry explicitly.
    pub fn new(settings: &settings::MokaCache) -> Self {
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
impl XenosCache for MokaCache {
    #[tracing::instrument(skip(self))]
    async fn get_uuid_by_username(&self, username: &str) -> Result<Cached<UuidEntry>, XenosError> {
        monitor_cache_get("moka", "uuid", || async {
            let entry = self.uuids.get(username).await;
            let cached = entry.into_cached(
                &self.settings.entries.uuid.exp,
                &self.settings.entries.uuid.exp_na,
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
                &self.settings.entries.profile.exp,
                &self.settings.entries.profile.exp_na,
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
                &self.settings.entries.skin.exp,
                &self.settings.entries.skin.exp_na,
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

    async fn get_cape_by_uuid(&self, uuid: &Uuid) -> Result<Cached<CapeEntry>, XenosError> {
        monitor_cache_get("moka", "cape", || async {
            let entry = self.capes.get(uuid).await;
            let cached = entry.into_cached(
                &self.settings.entries.cape.exp,
                &self.settings.entries.cape.exp_na,
            );
            Ok(cached)
        })
        .await
    }

    async fn set_cape_by_uuid(&self, uuid: Uuid, entry: CapeEntry) -> Result<(), XenosError> {
        monitor_cache_set("moka", "cape", || async {
            self.capes.insert(uuid, entry).await;
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
                &self.settings.entries.head.exp,
                &self.settings.entries.head.exp_na,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::Cached::{Expired, Hit, Miss};
    use crate::cache::UuidData;
    use crate::settings::{CacheEntries, CacheEntry};
    use std::time::Duration;

    fn build_moka_settings() -> settings::MokaCache {
        let cache_entry = CacheEntry {
            cap: 100,
            exp: Duration::from_secs(120),
            exp_na: Duration::from_secs(120),
            ttl: Duration::from_secs(120),
            ttl_na: Duration::from_secs(120),
            tti: Duration::from_secs(120),
            tti_na: Duration::from_secs(120),
        };
        settings::MokaCache {
            enabled: true,
            entries: CacheEntries {
                uuid: cache_entry.clone(),
                profile: cache_entry.clone(),
                skin: cache_entry.clone(),
                cape: cache_entry.clone(),
                head: cache_entry,
            },
        }
    }

    #[tokio::test]
    async fn empty_cache() {
        // given
        let settings = build_moka_settings();
        let cache = MokaCache::new(&settings);

        // when
        let cached = cache
            .get_uuid_by_username("hydrofin")
            .await
            .expect("expected get request to succeed");

        // then
        assert_eq!(cached, Miss)
    }

    #[tokio::test]
    async fn get_uuid_hit() {
        // given
        let settings = build_moka_settings();
        let cache = MokaCache::new(&settings);

        // when
        let entry = UuidEntry::new(UuidData {
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        });
        cache
            .set_uuid_by_username("hydrofin", entry.clone())
            .await
            .expect("expected set request to succeed");
        let cached = cache
            .get_uuid_by_username("hydrofin")
            .await
            .expect("expected get request to succeed");

        // then
        assert_eq!(cached, Hit(entry))
    }

    #[tokio::test]
    async fn get_uuid_expired() {
        // given
        let mut settings = build_moka_settings();
        settings.entries.uuid.exp = Duration::from_nanos(0);
        let cache = MokaCache::new(&settings);

        // when
        let entry = UuidEntry::new(UuidData {
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        });
        cache
            .set_uuid_by_username("hydrofin", entry.clone())
            .await
            .expect("expected set request to succeed");

        let cached = cache
            .get_uuid_by_username("hydrofin")
            .await
            .expect("expected get request to succeed");

        // then
        assert_eq!(cached, Expired(entry))
    }

    #[tokio::test]
    async fn get_uuid_miss() {
        // given
        let settings = build_moka_settings();
        let cache = MokaCache::new(&settings);

        // when
        let entry = UuidEntry::new(UuidData {
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        });
        cache
            .set_uuid_by_username("hydrofin", entry.clone())
            .await
            .expect("expected set request to succeed");
        let cached = cache
            .get_uuid_by_username("scrayos")
            .await
            .expect("expected get request to succeed");

        // then
        assert_eq!(cached, Miss)
    }
}
