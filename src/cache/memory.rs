use crate::cache::{
    monitor_cache_get, monitor_cache_set, Cached, HeadEntry, IntoCached, ProfileEntry, SkinEntry,
    UuidEntry, XenosCache,
};
use crate::error::XenosError;
use async_trait::async_trait;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// The [In-Memory Cache](MemoryCache) is a [cache](XenosCache) that saves cache entries (locally)
/// in memory. This is (theoretically) the fastest cache variant but provides no synchronisation with
/// other Xenos instances/deployments. In production, it should be used in conjunction with a remote
/// cache. Use the [Chaining Cache](crate::cache::chaining::ChainingCache) to chain multiple caches
/// (local -> remote) together for good performance with (partial) synchronisation.
///
/// The expiry can be set for both filled and empty [cache entries](crate::cache::CacheEntry). In
/// general, empty entries should have a higher expiry as it is unlikely that the corresponding
/// profile/username will be created in the next timeframe. On the other hand, we are interested in
/// getting the most up-to-date state of existing (and used) profiles/usernames.
///
/// # Example
///
/// ```rs
/// let cache = MemoryCache::new()
///   .with_expiry_uuid(100, 200)
///   .with_expiry_profile(100, 200)
///   .with_expiry_skin(100, 200)
///   .with_expiry_head(100, 200);
/// ```
#[derive(Debug)]
pub struct MemoryCache {
    // expiry settings
    expiry_uuid: u64,
    expiry_uuid_missing: u64,
    expiry_profile: u64,
    expiry_profile_missing: u64,
    expiry_skin: u64,
    expiry_skin_missing: u64,
    expiry_head: u64,
    expiry_head_missing: u64,
    // caches
    uuids: Arc<RwLock<HashMap<String, UuidEntry>>>,
    profiles: Arc<RwLock<HashMap<Uuid, ProfileEntry>>>,
    skins: Arc<RwLock<HashMap<Uuid, SkinEntry>>>,
    heads: Arc<RwLock<HashMap<String, HeadEntry>>>,
}

impl MemoryCache {
    /// Created a new empty [In-Memory Cache](MemoryCache) with no expiry (~585 aeons).
    /// Use successive builder methods to set expiry explicitly.
    pub fn new() -> Self {
        MemoryCache {
            expiry_uuid: u64::MAX,
            expiry_uuid_missing: u64::MAX,
            expiry_profile: u64::MAX,
            expiry_profile_missing: u64::MAX,
            expiry_skin: u64::MAX,
            expiry_skin_missing: u64::MAX,
            expiry_head: u64::MAX,
            expiry_head_missing: u64::MAX,
            uuids: Arc::new(RwLock::new(HashMap::default())),
            profiles: Arc::new(RwLock::new(HashMap::default())),
            skins: Arc::new(RwLock::new(HashMap::default())),
            heads: Arc::new(RwLock::new(HashMap::default())),
        }
    }

    /// Sets the expiry for username to uuid cache facet.
    pub fn with_expiry_uuid(mut self, default: u64, missing: u64) -> Self {
        self.expiry_uuid = default;
        self.expiry_uuid_missing = missing;
        self
    }

    /// Sets the expiry for uuid to profile cache facet.
    pub fn with_expiry_profile(mut self, default: u64, missing: u64) -> Self {
        self.expiry_profile = default;
        self.expiry_profile_missing = missing;
        self
    }

    /// Sets the expiry for uuid to skin cache facet.
    pub fn with_expiry_skin(mut self, default: u64, missing: u64) -> Self {
        self.expiry_skin = default;
        self.expiry_skin_missing = missing;
        self
    }

    /// Sets the expiry for uuid to head cache facet.
    pub fn with_expiry_head(mut self, default: u64, missing: u64) -> Self {
        self.expiry_head = default;
        self.expiry_head_missing = missing;
        self
    }
}

#[async_trait]
impl XenosCache for MemoryCache {
    #[tracing::instrument(skip(self))]
    async fn get_uuid_by_username(&self, username: &str) -> Result<Cached<UuidEntry>, XenosError> {
        monitor_cache_get("memory", "uuid", || async {
            let entry = self.uuids.read().get(username).cloned();
            let cached = entry.into_cached(&self.expiry_uuid, &self.expiry_uuid_missing);
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
        monitor_cache_set("memory", "uuid", || async {
            self.uuids.write().insert(username.to_string(), entry);
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_profile_by_uuid(&self, uuid: &Uuid) -> Result<Cached<ProfileEntry>, XenosError> {
        monitor_cache_get("memory", "profile", || async {
            let entry = self.profiles.read().get(uuid).cloned();
            let cached = entry.into_cached(&self.expiry_profile, &self.expiry_profile_missing);
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_profile_by_uuid(&self, uuid: Uuid, entry: ProfileEntry) -> Result<(), XenosError> {
        monitor_cache_set("memory", "profile", || async {
            self.profiles.write().insert(uuid, entry);
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_skin_by_uuid(&self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        monitor_cache_get("memory", "skin", || async {
            let entry = self.skins.read().get(uuid).cloned();
            let cached = entry.into_cached(&self.expiry_skin, &self.expiry_skin_missing);
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_skin_by_uuid(&self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        monitor_cache_set("memory", "skin", || async {
            self.skins.write().insert(uuid, entry);
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
        monitor_cache_get("memory", "head", || async {
            let uuid_str = uuid.simple().to_string();
            let entry = self
                .heads
                .read()
                .get(&format!("{uuid_str}.{overlay}"))
                .cloned();
            let cached = entry.into_cached(&self.expiry_head, &self.expiry_head_missing);
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
        monitor_cache_set("memory", "head", || async {
            let uuid_str = uuid.simple().to_string();
            self.heads
                .write()
                .insert(format!("{uuid_str}.{overlay}"), entry);
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
}
