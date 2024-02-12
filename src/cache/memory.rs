use crate::cache::Cached::{Expired, Hit, Miss};
use crate::cache::{Cached, HeadEntry, IntoCached, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use async_trait::async_trait;
use lazy_static::lazy_static;

use crate::settings;
use crate::settings::RedisCache;
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

fn track_cache_result<T>(cached: &Cached<T>, request_type: &str) {
    match cached {
        Expired(_) => {
            MEMORY_CACHE_GET_TOTAL
                .with_label_values(&[request_type, "expired"])
                .inc();
        }
        Hit(_) => {
            MEMORY_CACHE_GET_TOTAL
                .with_label_values(&[request_type, "hit"])
                .inc();
        }
        Miss => {
            MEMORY_CACHE_GET_TOTAL
                .with_label_values(&[request_type, "miss"])
                .inc();
        }
    }
}

#[derive(Debug)]
pub struct MemoryCache {
    pub settings: settings::Cache,
    uuids: HashMap<String, UuidEntry>,
    profiles: HashMap<Uuid, ProfileEntry>,
    skins: HashMap<Uuid, SkinEntry>,
    heads: HashMap<String, HeadEntry>,
}

impl MemoryCache {
    pub fn new(settings: settings::Cache) -> Self {
        MemoryCache {
            settings,
            uuids: HashMap::default(),
            profiles: HashMap::default(),
            skins: HashMap::default(),
            heads: HashMap::default(),
        }
    }

    pub fn with_expiry(expiry: u64) -> Self {
        MemoryCache {
            settings: settings::Cache {
                variant: settings::CacheVariant::Memory,
                redis: RedisCache {
                    address: "".to_string(),
                    ttl: None,
                },
                expiry_uuid: expiry,
                expiry_uuid_missing: expiry,
                expiry_profile: expiry,
                expiry_profile_missing: expiry,
                expiry_skin: expiry,
                expiry_skin_missing: expiry,
                expiry_head: expiry,
                expiry_head_missing: expiry,
            },
            uuids: HashMap::default(),
            profiles: HashMap::default(),
            skins: HashMap::default(),
            heads: HashMap::default(),
        }
    }
}

#[async_trait]
impl XenosCache for MemoryCache {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Cached<UuidEntry>, XenosError> {
        let entry = self.uuids.get(username).cloned();
        let cached = entry.into_cached(
            &self.settings.expiry_uuid,
            &self.settings.expiry_uuid_missing,
        );
        track_cache_result(&cached, "uuid");
        Ok(cached)
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
        let cached = entry.into_cached(
            &self.settings.expiry_profile,
            &self.settings.expiry_profile_missing,
        );
        track_cache_result(&cached, "profile");
        Ok(cached)
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
        let cached = entry.into_cached(
            &self.settings.expiry_skin,
            &self.settings.expiry_skin_missing,
        );
        track_cache_result(&cached, "skin");
        Ok(cached)
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
        let cached = entry.into_cached(
            &self.settings.expiry_head,
            &self.settings.expiry_head_missing,
        );
        track_cache_result(&cached, "head");
        Ok(cached)
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
    use crate::cache::{get_epoch_seconds, ProfileData, ProfileProperty, UuidData};
    use uuid::uuid;

    lazy_static! {
        static ref HYDROFIN: Profile = Profile {
            uuid: uuid!("09879557-e479-45a9-b434-a56377674627"),
            username: "Hydrofin",
            properties: vec![],
            profile_actions: vec![],
        };
    }

    #[allow(dead_code)]
    struct Profile {
        uuid: Uuid,
        username: &'static str,
        properties: Vec<ProfileProperty>,
        profile_actions: Vec<String>,
    }

    #[allow(dead_code)]
    impl Profile {
        fn uuid_data(&self) -> UuidData {
            UuidData {
                username: self.username.to_string(),
                uuid: self.uuid,
            }
        }

        fn profile_data(&self) -> ProfileData {
            ProfileData {
                uuid: self.uuid,
                name: self.username.to_string(),
                properties: self.properties.clone(),
                profile_actions: self.profile_actions.clone(),
            }
        }
    }

    #[tokio::test]
    async fn get_uuid_by_username_hit() {
        // given
        let mut cache = MemoryCache::with_expiry(3000);
        cache.uuids.insert(
            HYDROFIN.username.to_lowercase(),
            UuidEntry {
                timestamp: get_epoch_seconds(),
                data: Some(HYDROFIN.uuid_data()),
            },
        );

        // when
        let retrieved = cache
            .get_uuid_by_username(&HYDROFIN.username.to_lowercase())
            .await
            .unwrap();

        // then
        match retrieved {
            Hit(UuidEntry {
                data: Some(data), ..
            }) => {
                assert_eq!(data, HYDROFIN.uuid_data());
            }
            other => panic!("Expected hit with matching fields, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_uuid_by_username_expired() {
        // given
        let mut cache = MemoryCache::with_expiry(20);
        cache.uuids.insert(
            HYDROFIN.username.to_lowercase(),
            UuidEntry {
                timestamp: get_epoch_seconds() - 30,
                data: Some(HYDROFIN.uuid_data()),
            },
        );

        // when
        let retrieved = cache
            .get_uuid_by_username(&HYDROFIN.username.to_lowercase())
            .await
            .unwrap();

        // then
        match retrieved {
            Expired(UuidEntry {
                data: Some(data), ..
            }) => {
                assert_eq!(data, HYDROFIN.uuid_data());
            }
            other => panic!("Expected expired with matching fields, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_uuid_by_username_miss() {
        // given
        let mut cache = MemoryCache::with_expiry(3000);
        cache.uuids.insert(
            HYDROFIN.username.to_lowercase(),
            UuidEntry {
                timestamp: get_epoch_seconds(),
                data: Some(HYDROFIN.uuid_data()),
            },
        );

        // when
        let retrieved = cache.get_uuid_by_username("Notch").await.unwrap();

        // then
        assert_eq!(Miss, retrieved);
    }

    #[tokio::test]
    async fn set_uuid_by_username_success() {
        // given
        let mut cache = MemoryCache::with_expiry(3000);

        // when
        cache
            .set_uuid_by_username(
                &HYDROFIN.username.to_lowercase(),
                UuidEntry {
                    timestamp: get_epoch_seconds(),
                    data: Some(HYDROFIN.uuid_data()),
                },
            )
            .await
            .unwrap();

        // then
        let retrieved = cache.uuids.get(&HYDROFIN.username.to_lowercase()).cloned();

        // then
        match retrieved {
            Some(UuidEntry {
                data: Some(data), ..
            }) => {
                assert_eq!(data, HYDROFIN.uuid_data());
            }
            other => panic!("Expected some with matching fields, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn set_uuid_by_username_override() {
        // given
        let mut cache = MemoryCache::with_expiry(3000);
        cache.uuids.insert(
            HYDROFIN.username.to_lowercase(),
            UuidEntry {
                timestamp: get_epoch_seconds(),
                data: None,
            },
        );

        // when
        cache
            .set_uuid_by_username(
                &HYDROFIN.username.to_lowercase(),
                UuidEntry {
                    timestamp: get_epoch_seconds(),
                    data: Some(HYDROFIN.uuid_data()),
                },
            )
            .await
            .unwrap();

        // then
        let retrieved = cache.uuids.get(&HYDROFIN.username.to_lowercase()).cloned();

        // then
        match retrieved {
            Some(UuidEntry {
                data: Some(data), ..
            }) => {
                assert_eq!(data, HYDROFIN.uuid_data());
            }
            other => panic!("Expected some with matching fields, got {:?}", other),
        }
    }
}
