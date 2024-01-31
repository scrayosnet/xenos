use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;
use xenos::cache::{HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use xenos::error::XenosError;
use xenos::util::get_epoch_seconds;

#[derive(Default)]
struct MemoryCache {
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
    ) -> Result<Option<UuidEntry>, XenosError> {
        Ok(self.uuids.get(username).cloned())
    }

    async fn set_uuid_by_username(&mut self, entry: UuidEntry) -> Result<(), XenosError> {
        self.uuids.insert(entry.username.clone(), entry);
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Option<ProfileEntry>, XenosError> {
        Ok(self.profiles.get(uuid).cloned())
    }

    async fn set_profile_by_uuid(&mut self, entry: ProfileEntry) -> Result<(), XenosError> {
        self.profiles.insert(entry.uuid, entry);
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Option<SkinEntry>, XenosError> {
        Ok(self.skins.get(uuid).cloned())
    }

    async fn set_skin_by_uuid(&mut self, entry: SkinEntry) -> Result<(), XenosError> {
        self.skins.insert(entry.uuid, entry);
        Ok(())
    }

    async fn get_head_by_uuid(
        &mut self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Option<HeadEntry>, XenosError> {
        let uuid = uuid.simple().to_string();
        let key = &format!("{uuid}.{overlay}");
        Ok(self.heads.get(key).cloned())
    }

    async fn set_head_by_uuid(
        &mut self,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError> {
        let uuid = entry.uuid.simple().to_string();
        let key = format!("{uuid}.{overlay}");
        self.heads.insert(key, entry);
        Ok(())
    }
}

#[tokio::test]
async fn memory_cache_uuids() {
    // given
    let mut cache = MemoryCache::default();
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
        .set_uuid_by_username(entry_hydrofin.clone())
        .await
        .unwrap();
    cache
        .set_uuid_by_username(entry_scrayos.clone())
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
        timestamp: get_epoch_seconds(),
        uuid: Uuid::new_v4(),
        name: "Hydrofin".to_string(),
        properties: vec![],
        profile_actions: vec![],
    };

    // when
    cache.set_profile_by_uuid(entry.clone()).await.unwrap();
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
    let entry = SkinEntry {
        timestamp: get_epoch_seconds(),
        uuid: Uuid::new_v4(),
        bytes: vec![0, 0, 0, 1, 0],
    };

    // when
    cache.set_skin_by_uuid(entry.clone()).await.unwrap();
    let retrieved = cache.get_skin_by_uuid(&entry.uuid).await.unwrap();

    // then
    assert!(retrieved.is_some(), "expect cached to exist");
    assert_eq!(
        entry,
        retrieved.unwrap(),
        "expect cache entry to not change in cache"
    );
}
