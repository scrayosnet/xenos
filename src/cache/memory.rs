use crate::cache::Cached::{Hit, Miss};
use crate::cache::{Cached, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
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
    uuids: HashMap<String, UuidEntry>,
    profiles: HashMap<Uuid, ProfileEntry>,
    skins: HashMap<Uuid, SkinEntry>,
    heads: HashMap<String, HeadEntry>,
}

#[async_trait]
impl XenosCache for MemoryCache {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Cached<UuidEntry>, XenosError> {
        let entry = self.uuids.get(username).cloned();
        match entry {
            Some(entry) => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["uuid", "hit"])
                    .inc();
                Ok(Hit(entry))
            }
            None => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["uuid", "miss"])
                    .inc();
                Ok(Miss)
            }
        }
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
        match entry {
            Some(entry) => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["profile", "hit"])
                    .inc();
                Ok(Hit(entry))
            }
            None => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["profile", "miss"])
                    .inc();
                Ok(Miss)
            }
        }
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
        match entry {
            Some(entry) => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["skin", "hit"])
                    .inc();
                Ok(Hit(entry))
            }
            None => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["skin", "miss"])
                    .inc();
                Ok(Miss)
            }
        }
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
        match entry {
            Some(entry) => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["head", "hit"])
                    .inc();
                Ok(Hit(entry))
            }
            None => {
                MEMORY_CACHE_GET_TOTAL
                    .with_label_values(&["head", "miss"])
                    .inc();
                Ok(Miss)
            }
        }
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
    use crate::cache::Cached::Hit;

    #[tokio::test]
    async fn memory_cache_uuids() {
        // given
        let mut cache = MemoryCache::default();
        let entry_hydrofin = UuidEntry {
            timestamp: 100,
            username: "Hydrofin".to_string(),
            uuid: Uuid::new_v4(),
        };
        let entry_scrayos = UuidEntry {
            timestamp: 100100,
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
    async fn memory_cache_profile() {
        // given
        let mut cache = MemoryCache::default();
        let entry = ProfileEntry {
            timestamp: 100,
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
    async fn memory_cache_skin() {
        // given
        let mut cache = MemoryCache::default();
        let uuid = Uuid::new_v4();
        let entry = SkinEntry {
            timestamp: 1001001,
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
}
