//! The cache module provides multiple [cache](XenosCache) implementations for the xenos service.

pub mod chaining;
pub mod moka;
mod monitor;
pub mod redis;

use crate::cache::Cached::{Expired, Hit, Miss};
use crate::error::XenosError;
use crate::mojang::Profile;
use async_trait::async_trait;
use chrono::Utc;
pub use monitor::{monitor_cache_get, monitor_cache_set};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::time::Duration;
use uuid::Uuid;

/// [Cached] is a utility wrapper for [CacheEntry]. Most caches respond with an [Option] for get requests.
/// In the services we are interested not only if an entry was found but if the entry is considered
/// [Expired] by the cache. By wrapping the [CacheEntry] in [Cached], the caches can communicate their
/// intent while providing ease-of-use for the consumer with the match operator.
///
/// Additionally, the [CacheEntry] provides methods for checking its creation time and age.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Cached<T> {
    /// Value was found.
    Hit(T),

    /// Value was found but is flagged as expired by cache.
    Expired(T),

    /// Value was not found.
    Miss,
}

/// A [CacheEntry] is a wrapper that may hold cached data. It consists of a timestamp, the time at which
/// the entry was created, and optional inner data. If no data is set, the entry is considered `empty`.
///
/// In general, a [CacheEntry] is used as an immutable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheEntry<D>
where
    D: Debug + Clone + PartialEq + Eq,
{
    /// The entry creation time in seconds.
    pub timestamp: u64,

    /// The inner data of the cache entry.
    pub data: Option<D>,
}

impl<D> CacheEntry<D>
where
    D: Debug + Clone + PartialEq + Eq,
{
    /// Creates a new empty instance of [CacheEntry].
    pub fn new_empty() -> Self {
        Self {
            timestamp: get_epoch_seconds(),
            data: None,
        }
    }

    /// Creates a new filled instance of [CacheEntry] with the provided data. The creation time
    /// is set to the current time.
    pub fn new(data: D) -> Self {
        Self {
            timestamp: get_epoch_seconds(),
            data: Some(data),
        }
    }

    /// Checks if the instance has data or is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_none()
    }

    /// Gets the age of the [CacheEntry]. The age of a [CacheEntry] is the relative time from which
    /// the cache entry was created until now.
    pub fn current_age(&self) -> u64 {
        get_epoch_seconds() - self.timestamp
    }

    /// Checks if the [CacheEntry] age is greater or equal than the provided expiry.
    pub fn is_expired(&self, expiry: &Duration) -> bool {
        self.current_age() >= expiry.as_secs()
    }
}

/// A utility for converting something into a [Cached].
pub trait IntoCached<T> {
    fn into_cached(self, expiry: &Duration, expiry_missing: &Duration) -> Cached<T>;
}

impl<D> IntoCached<CacheEntry<D>> for Option<CacheEntry<D>>
where
    D: Debug + Clone + PartialEq + Eq,
{
    fn into_cached(self, expiry: &Duration, expiry_missing: &Duration) -> Cached<CacheEntry<D>> {
        match self {
            None => Miss,
            Some(v) if v.is_empty() && v.is_expired(expiry_missing) => Expired(v),
            Some(v) if v.is_expired(expiry) => Expired(v),
            Some(v) => Hit(v),
        }
    }
}

/// A [UuidData] is a resolved username (case-sensitive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UuidData {
    pub username: String,
    pub uuid: Uuid,
}

/// A [SkinData] is a profile skin with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinData {
    pub bytes: Vec<u8>,
    pub model: String,
}

/// A [CapeData] is a profile cape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapeData {
    pub bytes: Vec<u8>,
}

/// A [HeadData] is a profile skin's head.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeadData {
    pub bytes: Vec<u8>,
}

/// A [UuidEntry] is a [cache entry](CacheEntry) that encapsulates [uuid data](UuidData). It is used
/// to cache username to uuid resolve results.
pub type UuidEntry = CacheEntry<UuidData>;

/// A [ProfileEntry] is a [cache entry](CacheEntry) that encapsulates [profile data](Profile).
/// It is used to cache uuid to profile resolve results.
pub type ProfileEntry = CacheEntry<Profile>;

/// A [SkinEntry] is a [cache entry](CacheEntry) that encapsulates [skin data](SkinData). It is used
/// to cache uuid to skin data resolve results.
pub type SkinEntry = CacheEntry<SkinData>;

/// A [CapeEntry] is a [cache entry](CacheEntry) that encapsulates [cape data](CapeData). It is used
/// to cache uuid to skin data resolve results.
pub type CapeEntry = CacheEntry<CapeData>;

/// A [HeadEntry] is a [cache entry](CacheEntry) that encapsulates [head data](HeadData). It is used
/// to cache uuid to head data resolve results.
pub type HeadEntry = CacheEntry<HeadData>;

/// A [Cache](XenosCache) represents any cache used by Xenos. [Cache entries](CacheEntry) are
/// returned best-effort. That means .can be in
/// one of three states:
/// - [Hit] if a valid entry was found in the cache,
/// - [Expired] if an entry was found, but it has expired, and
/// - [Miss] if no entry was found.
///
/// Based on the implementation, some response types may not be represented, e.g. a cache might not
/// support [expired](Expired) [cache entries](CacheEntry).
///
/// The cache implementation itself handles concurrency, so it does not have to be wrapped in e.g.
/// a [Mutex](tokio::sync::Mutex).
#[async_trait]
pub trait XenosCache: Debug + Send + Sync {
    async fn get_uuid_by_username(&self, username: &str) -> Result<Cached<UuidEntry>, XenosError>;
    async fn set_uuid_by_username(
        &self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError>;
    async fn get_profile_by_uuid(&self, uuid: &Uuid) -> Result<Cached<ProfileEntry>, XenosError>;
    async fn set_profile_by_uuid(&self, uuid: Uuid, entry: ProfileEntry) -> Result<(), XenosError>;
    async fn get_skin_by_uuid(&self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError>;
    async fn set_skin_by_uuid(&self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError>;
    async fn get_cape_by_uuid(&self, uuid: &Uuid) -> Result<Cached<CapeEntry>, XenosError>;
    async fn set_cape_by_uuid(&self, uuid: Uuid, entry: CapeEntry) -> Result<(), XenosError>;
    async fn get_head_by_uuid(
        &self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError>;
    async fn set_head_by_uuid(
        &self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError>;
}

/// Gets the current time in seconds.
pub fn get_epoch_seconds() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap()
}

#[cfg(test)]
mod test {
    use super::*;
    type VoidEntry = CacheEntry<()>;

    #[test]
    fn is_empty() {
        // given
        let entry = VoidEntry::new_empty();

        // when

        // then
        assert!(entry.is_empty());
    }

    #[test]
    fn is_not_empty() {
        // given
        let entry = VoidEntry::new(());

        // when

        // then
        assert!(!entry.is_empty());
    }

    #[test]
    fn expired() {
        // given
        let entry = VoidEntry::new_empty();

        // when

        // then
        assert!(entry.is_expired(&Duration::from_secs(0)));
        assert!(!entry.is_expired(&Duration::from_secs(1)));
        assert!(!entry.is_expired(&Duration::from_secs(10)));
        assert!(!entry.is_expired(&Duration::from_secs(120)));
    }

    #[test]
    fn into_cached_miss() {
        // given
        let val: Option<VoidEntry> = None;

        // when
        let entry = val.into_cached(&Duration::from_secs(0), &Duration::from_secs(0));

        // then
        assert!(matches!(entry, Miss));
    }

    #[test]
    fn into_cached_expired() {
        // given
        let val: Option<VoidEntry> = Some(VoidEntry::new_empty());

        // when
        let entry = val.into_cached(&Duration::from_secs(0), &Duration::from_secs(0));

        // then
        assert!(matches!(entry, Expired(_)));
    }

    #[test]
    fn into_cached_hit() {
        // given
        let val: Option<VoidEntry> = Some(VoidEntry::new_empty());

        // when
        let entry = val.into_cached(&Duration::from_secs(10), &Duration::from_secs(10));

        // then
        assert!(matches!(entry, Hit(_)));
    }
}
