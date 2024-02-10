use crate::cache::Cached::{Expired, Hit, Miss};
use crate::cache::{
    has_elapsed, CacheEntry, Cached, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache,
};
use crate::error::XenosError;
use async_trait::async_trait;
use lazy_static::lazy_static;
use prometheus::{register_int_counter_vec, IntCounterVec};
use std::collections::HashMap;
use uuid::Uuid;

lazy_static! {
    pub static ref MEMORY_CACHE_SET_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xenos_memory_cache_set_total",
        "Total number of set requests to the memory cache.",
        &["request_type"],
    )
    .unwrap();
    pub static ref MEMORY_CACHE_GET_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xenos_memory_cache_get_total",
        "Total number of get requests to the memory cache.",
        &["request_type", "cache_result"],
    )
    .unwrap();
}

#[derive(Default)]
pub struct MemoryCache {
    pub cache_time: u64,
    uuids: HashMap<String, UuidEntry>,
    profiles: HashMap<Uuid, ProfileEntry>,
    skins: HashMap<Uuid, SkinEntry>,
    heads: HashMap<String, HeadEntry>,
}

impl MemoryCache {
    pub fn with_cache_time(cache_time: u64) -> Self {
        MemoryCache {
            cache_time,
            ..Default::default()
        }
    }

    // converts a option into a cached while also incrementing memory cache response metrics
    fn cached_from<T: CacheEntry>(&self, value: Option<T>, request_type: &str) -> Cached<T> {
        match value {
            Some(value) if self.has_expired(&value.get_timestamp()) => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&[request_type, "expired"])
                    .inc();
                Expired(value)
            }
            Some(value) => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&[request_type, "hit"])
                    .inc();
                Hit(value)
            }
            None => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&[request_type, "miss"])
                    .inc();
                Miss
            }
        }
    }

    fn has_expired(&self, timestamp: &u64) -> bool {
        has_elapsed(timestamp, &self.cache_time)
    }
}

#[async_trait]
impl XenosCache for MemoryCache {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Cached<UuidEntry>, XenosError> {
        let entry = self.uuids.get(username).cloned();
        Ok(self.cached_from(entry, "uuid"))
    }

    async fn set_uuid_by_username(
        &mut self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        MEMORY_CACHE_SET_TOTAL.with_label_values(&["uuid"]).inc();
        self.uuids.insert(username.to_string(), entry);
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError> {
        let entry = self.profiles.get(uuid).cloned();
        Ok(self.cached_from(entry, "profile"))
    }

    async fn set_profile_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: ProfileEntry,
    ) -> Result<(), XenosError> {
        MEMORY_CACHE_SET_TOTAL.with_label_values(&["profile"]).inc();
        self.profiles.insert(uuid, entry);
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        let entry = self.skins.get(uuid).cloned();
        Ok(self.cached_from(entry, "skin"))
    }

    async fn set_skin_by_uuid(&mut self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        MEMORY_CACHE_SET_TOTAL.with_label_values(&["skin"]).inc();
        self.skins.insert(uuid, entry);
        Ok(())
    }

    async fn get_head_by_uuid(
        &mut self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        let uuid_str = uuid.simple().to_string();
        let entry = self.heads.get(&format!("{uuid_str}.{overlay}")).cloned();
        Ok(self.cached_from(entry, "head"))
    }

    async fn set_head_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError> {
        MEMORY_CACHE_SET_TOTAL.with_label_values(&["head"]).inc();
        let uuid_str = uuid.simple().to_string();
        self.heads.insert(format!("{uuid_str}.{overlay}"), entry);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::get_epoch_seconds;
    use crate::cache::Cached::Hit;

    #[tokio::test]
    async fn get_uuid_by_username_hit() {
        // given
        let mut cache = MemoryCache::with_cache_time(3000);
        let entry_hydrofin = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        };
        let entry_scrayos = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Scrayos".to_string(),
            uuid: Uuid::new_v4(),
        };

        // when
        cache
            .set_uuid_by_username(&entry_hydrofin.username, entry_hydrofin.clone())
            .await
            .unwrap();
        cache
            .set_uuid_by_username(&entry_scrayos.username, entry_scrayos.clone())
            .await
            .unwrap();
        let retrieved_hydrofin = cache
            .get_uuid_by_username(&entry_hydrofin.username)
            .await
            .unwrap();

        // then
        assert_eq!(
            Hit(entry_hydrofin),
            retrieved_hydrofin,
            "expect cache entry to not change in cache"
        );
    }

    #[tokio::test]
    async fn get_uuid_by_username_miss() {
        // given
        let mut cache = MemoryCache::with_cache_time(3000);
        let entry_hydrofin = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        };
        let entry_scrayos = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Scrayos".to_string(),
            uuid: Uuid::new_v4(),
        };

        // when
        cache
            .set_uuid_by_username(&entry_hydrofin.username, entry_hydrofin.clone())
            .await
            .unwrap();
        cache
            .set_uuid_by_username(&entry_scrayos.username, entry_scrayos.clone())
            .await
            .unwrap();
        let retrieved_hydrofin = cache.get_uuid_by_username("Notch").await.unwrap();

        // then
        assert_eq!(
            Miss, retrieved_hydrofin,
            "expect cache entry to not change in cache"
        );
    }

    #[tokio::test]
    async fn set_uuid_by_username() {
        // given
        let mut cache = MemoryCache::with_cache_time(3000);
        let entry_hydrofin = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        };
        let entry_scrayos = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Scrayos".to_string(),
            uuid: Uuid::new_v4(),
        };

        // when
        cache
            .set_uuid_by_username(&entry_hydrofin.username, entry_hydrofin.clone())
            .await
            .unwrap();
        cache
            .set_uuid_by_username(&entry_scrayos.username, entry_scrayos.clone())
            .await
            .unwrap();

        // then
        let retrieved_scrayos = cache.uuids.get(&entry_scrayos.username).cloned();

        assert_eq!(
            Some(entry_scrayos),
            retrieved_scrayos,
            "expect cache entry to be in map with username key"
        );
    }

    #[tokio::test]
    async fn set_uuid_by_username_override() {
        // given
        let mut cache = MemoryCache::with_cache_time(3000);
        let entry_hydrofin = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        };
        let entry_scrayos = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Scrayos".to_string(),
            uuid: Uuid::new_v4(),
        };
        let entry_scrayos_2 = UuidEntry {
            timestamp: get_epoch_seconds(),
            username: "Scrayos 2".to_string(),
            uuid: Uuid::new_v4(),
        };

        // when
        cache
            .set_uuid_by_username(&entry_scrayos.username, entry_scrayos.clone())
            .await
            .unwrap();
        cache
            .set_uuid_by_username(&entry_hydrofin.username, entry_hydrofin.clone())
            .await
            .unwrap();
        cache
            .set_uuid_by_username(&entry_scrayos.username, entry_scrayos_2.clone())
            .await
            .unwrap();

        // then
        let retrieved_scrayos = cache.uuids.get(&entry_scrayos.username).cloned();

        assert_eq!(
            Some(entry_scrayos_2),
            retrieved_scrayos,
            "expect cache entry to be overridden in map with username key"
        );
    }

    #[tokio::test]
    async fn get_profile_by_uuid_hit() {
        // given
        let mut cache = MemoryCache::with_cache_time(3000);
        let entry = ProfileEntry {
            timestamp: get_epoch_seconds(),
            uuid: Uuid::new_v4(),
            name: "Hydrofin".to_string(),
            properties: vec![],
            profile_actions: vec![],
        };

        // when
        cache
            .set_profile_by_uuid(entry.uuid, entry.clone())
            .await
            .unwrap();
        let retrieved = cache.get_profile_by_uuid(&entry.uuid).await.unwrap();

        // then
        assert_eq!(
            Hit(entry),
            retrieved,
            "expect cache entry to not change in cache"
        );
    }

    #[tokio::test]
    async fn get_skin_by_uuid_hit() {
        // given
        let mut cache = MemoryCache::with_cache_time(3000);
        let uuid = Uuid::new_v4();
        let entry = SkinEntry {
            timestamp: get_epoch_seconds(),
            bytes: vec![0, 0, 0, 1, 0],
        };

        // when
        cache.set_skin_by_uuid(uuid, entry.clone()).await.unwrap();
        let retrieved = cache.get_skin_by_uuid(&uuid).await.unwrap();

        // then
        assert_eq!(
            Hit(entry),
            retrieved,
            "expect cache entry to not change in cache"
        );
    }

    #[tokio::test]
    async fn get_skin_by_uuid_expired() {
        // given
        let mut cache = MemoryCache::with_cache_time(0);
        let uuid = Uuid::new_v4();
        let entry = SkinEntry {
            timestamp: 0,
            bytes: vec![0, 0, 0, 1, 0],
        };

        // when
        cache.set_skin_by_uuid(uuid, entry.clone()).await.unwrap();
        let retrieved = cache.get_skin_by_uuid(&uuid).await.unwrap();

        // then
        assert_eq!(
            Expired(entry),
            retrieved,
            "expect cache entry to not change in cache"
        );
    }
}
