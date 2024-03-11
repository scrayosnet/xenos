use crate::cache::Cached::Miss;
use crate::cache::{
    monitor_cache_get, monitor_cache_set, Cached, CapeEntry, HeadEntry, ProfileEntry, SkinEntry,
    UuidEntry, XenosCache,
};
use crate::error::XenosError;
use async_trait::async_trait;
use std::future::Future;
use uuid::Uuid;
use Cached::{Expired, Hit};

/// The [Chaining Cache](ChainingCache) is a [cache](XenosCache) that wraps multiple [caches](XenosCache)
/// in layers. When requesting a cache entry, it searches top-down for a valid entry. It tries to
/// find a non-expired cache entry and will, if only expired entries are found, return the cache
/// entry from the lowest cache. If an entry (non-expired/expired) is found, all cache layers above
/// are updated with that entry (internal invariant).
///
/// - In general, lower caches (layers) should hold cache entries for longer than upper caches.
/// - As the [Chaining Cache](ChainingCache) holds thread save [caches](XenosCache) internally, it itself
/// can be used without additional synchronisation (e.g. [Mutex](tokio::sync::Mutex))
///
/// # Example
///
/// Generally, we intend the cache to be used for combining a local and a remote cache. The local
/// cache would hold (size-limited) entries for short periods of time, while the remote cache is
/// "shared" among multiple instances/deployments of Xenos. This configuration reduces the required
/// network overhead and load on the remote cache for successive requests on the same resource.
///
/// In this configuration the remote cache could be omitted.
///
/// ```rs
/// let cache = ChainingCache::new()
///   .add_cache(LocalCache::new())
///   .add_optional_cache(Some(RemoteCache::new()));
/// ```
///
#[derive(Debug)]
pub struct ChainingCache {
    caches: Vec<Box<dyn XenosCache>>,
}

impl Default for ChainingCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ChainingCache {
    /// Creates a new [Chaining Cache](ChainingCache) with no inner caches.
    pub fn new() -> Self {
        ChainingCache { caches: vec![] }
    }

    /// Pushes an optional cache to the end of the inner caches (the last layer).
    pub async fn add_cache<F, Fut>(mut self, enabled: bool, f: F) -> Result<Self, XenosError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Box<dyn XenosCache>, XenosError>>,
    {
        if enabled {
            let cache = f().await?;
            self.caches.push(cache);
        }
        Ok(self)
    }

    /// Gets an entry from the cache layers. It iteratively goes through the inner caches searching
    /// for an entry. After an entry is found, the caches are updated accordingly and the entry is
    /// returned.
    ///
    /// For this, it requires a pair of `getter` and `setter`. The getter is used to retrieve cache
    /// entries while the setter is used to update inner caches in case an entry was found.
    async fn get<'a, D, G, GF, S, SF>(
        &'a self,
        getter: G,
        setter: S,
    ) -> Result<Cached<D>, XenosError>
    where
        G: Fn(&'a dyn XenosCache) -> GF,
        GF: Future<Output = Result<Cached<D>, XenosError>>,
        S: Fn(&'a dyn XenosCache, D) -> SF,
        SF: Future<Output = Result<(), XenosError>>,
        D: Clone,
    {
        let mut depth = 0;
        let mut result = Miss;
        // try to find cache hit, saving the lowest intermediate result
        for i in 0..self.caches.len() {
            let cached = getter(self.caches[depth].as_ref()).await?;
            match cached {
                Hit(entry) => {
                    result = Hit(entry);
                    depth = i;
                    break;
                }
                Expired(entry) => {
                    result = Expired(entry);
                }
                Miss => {}
            };
        }
        // update upper caches, ensuring the consistency invariant
        match &result {
            Hit(entry) | Expired(entry) => {
                for i in (0..depth).rev() {
                    setter(self.caches[i].as_ref(), entry.clone()).await?;
                }
            }
            _ => {}
        };
        Ok(result)
    }

    /// Sets an entry to all inner caches.
    async fn set<'a, D, S, SF>(&'a self, entry: D, setter: S) -> Result<(), XenosError>
    where
        S: Fn(&'a dyn XenosCache, D) -> SF,
        SF: Future<Output = Result<(), XenosError>>,
        D: Clone,
    {
        // update lower caches first, ensuring the consistency invariant
        for cache in self.caches.iter().rev() {
            setter(cache.as_ref(), entry.clone()).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl XenosCache for ChainingCache {
    #[tracing::instrument(skip(self))]
    async fn get_uuid_by_username(&self, username: &str) -> Result<Cached<UuidEntry>, XenosError> {
        monitor_cache_get("chaining", "uuid", || {
            self.get(
                |cache| cache.get_uuid_by_username(username),
                |cache, entry| cache.set_uuid_by_username(username, entry),
            )
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_uuid_by_username(
        &self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        monitor_cache_set("chaining", "uuid", || {
            self.set(entry, |cache, entry| {
                cache.set_uuid_by_username(username, entry)
            })
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_profile_by_uuid(&self, uuid: &Uuid) -> Result<Cached<ProfileEntry>, XenosError> {
        monitor_cache_get("chaining", "profile", || {
            self.get(
                |cache| cache.get_profile_by_uuid(uuid),
                |cache, entry| cache.set_profile_by_uuid(*uuid, entry),
            )
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_profile_by_uuid(&self, uuid: Uuid, entry: ProfileEntry) -> Result<(), XenosError> {
        monitor_cache_set("chaining", "profile", || {
            self.set(entry, |cache, entry| cache.set_profile_by_uuid(uuid, entry))
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_skin_by_uuid(&self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        monitor_cache_get("chaining", "skin", || {
            self.get(
                |cache| cache.get_skin_by_uuid(uuid),
                |cache, entry| cache.set_skin_by_uuid(*uuid, entry),
            )
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_skin_by_uuid(&self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        monitor_cache_set("chaining", "skin", || {
            self.set(entry, |cache, entry| cache.set_skin_by_uuid(uuid, entry))
        })
        .await
    }

    async fn get_cape_by_uuid(&self, uuid: &Uuid) -> Result<Cached<CapeEntry>, XenosError> {
        monitor_cache_get("chaining", "cape", || {
            self.get(
                |cache| cache.get_cape_by_uuid(uuid),
                |cache, entry| cache.set_cape_by_uuid(*uuid, entry),
            )
        })
        .await
    }

    async fn set_cape_by_uuid(&self, uuid: Uuid, entry: CapeEntry) -> Result<(), XenosError> {
        monitor_cache_set("chaining", "cape", || {
            self.set(entry, |cache, entry| cache.set_cape_by_uuid(uuid, entry))
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_head_by_uuid(
        &self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        monitor_cache_get("chaining", "head", || {
            self.get(
                |cache| cache.get_head_by_uuid(uuid, overlay),
                |cache, entry| cache.set_head_by_uuid(*uuid, entry, overlay),
            )
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
        monitor_cache_set("chaining", "head", || {
            self.set(entry, |cache, entry| {
                cache.set_head_by_uuid(uuid, entry, overlay)
            })
        })
        .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::UuidData;

    #[tokio::test]
    async fn empty_cache() {
        // given
        let cache = ChainingCache::new();

        // when
        cache
            .set_uuid_by_username(
                "hydrofin",
                UuidEntry::new(UuidData {
                    username: "Hydrofin".to_string(),
                    uuid: Uuid::new_v4(),
                }),
            )
            .await
            .expect("expected set request to succeed");
        let cached = cache
            .get_uuid_by_username("hydrofin")
            .await
            .expect("expected get request to succeed");

        // then
        assert_eq!(cached, Miss)
    }
}
