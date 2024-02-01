use crate::cache::{Cached, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

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
        Ok(self.uuids.get(username).cloned().into())
    }

    async fn set_uuid_by_username(
        &mut self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        self.uuids.insert(username.to_string(), entry);
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError> {
        Ok(self.profiles.get(uuid).cloned().into())
    }

    async fn set_profile_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: ProfileEntry,
    ) -> Result<(), XenosError> {
        self.profiles.insert(uuid, entry);
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        Ok(self.skins.get(uuid).cloned().into())
    }

    async fn set_skin_by_uuid(&mut self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        self.skins.insert(uuid, entry);
        Ok(())
    }

    async fn get_head_by_uuid(
        &mut self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        let uuid_str = uuid.simple().to_string();
        Ok(self
            .heads
            .get(&format!("{uuid_str}.{overlay}"))
            .cloned()
            .into())
    }

    async fn set_head_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError> {
        let uuid_str = uuid.simple().to_string();
        self.heads.insert(format!("{uuid_str}.{overlay}"), entry);
        Ok(())
    }
}

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
    assert!(retrieved_hydrofin.is_some(), "expect cached to exist");
    assert_eq!(
        entry_hydrofin,
        retrieved_hydrofin.unwrap(),
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
        .set_profile_by_uuid(entry.uuid.clone(), entry.clone())
        .await
        .unwrap();
    let retrieved = cache.get_profile_by_uuid(&entry.uuid).await.unwrap();

    // then
    assert!(retrieved.is_some(), "expect cached to exist");
    assert_eq!(
        entry,
        retrieved.unwrap(),
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
    cache
        .set_skin_by_uuid(uuid.clone(), entry.clone())
        .await
        .unwrap();
    let retrieved = cache.get_skin_by_uuid(&uuid).await.unwrap();

    // then
    assert!(retrieved.is_some(), "expect cached to exist");
    assert_eq!(
        entry,
        retrieved.unwrap(),
        "expect cache entry to not change in cache"
    );
}
