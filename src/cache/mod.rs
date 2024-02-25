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

/// A [Cached] is a cache response wrapper. It is used to signal the state of the cache response.
/// A cache response may be in one of three states (excluding error results).
/// - [Hit] if a valid entry was found in the cache,
/// - [Expired] if an entry was found, but it has expired, and
/// - [Miss] if no entry was found.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Cached<T> {
    Hit(T),
    Expired(T),
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
    pub timestamp: u64,
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

    /// Creates a new filled instance of [CacheEntry] with the provided data.
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
            Some(v) if v.is_empty() && has_elapsed(&v.timestamp, expiry_missing) => Expired(v),
            Some(v) if has_elapsed(&v.timestamp, expiry) => Expired(v),
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

/// A [UuidEntry] is a [cache entry](CacheEntry) that encapsulates [uuid data](UuidData). It is used
/// to cache username to uuid resolve results.
pub type UuidEntry = CacheEntry<UuidData>;

/// A [ProfileEntry] is a [cache entry](CacheEntry) that encapsulates [profile data](Profile).
/// It is used to cache uuid to profile resolve results.
pub type ProfileEntry = CacheEntry<Profile>;

/// A [SkinEntry] is a [cache entry](CacheEntry) that encapsulates [skin data](Vec<u8>). It is used
/// to cache uuid to skin data resolve results.
pub type SkinEntry = CacheEntry<Vec<u8>>;

/// A [HeadEntry] is a [cache entry](CacheEntry) that encapsulates [head data](Vec<u8>). It is used
/// to cache uuid to head data resolve results.
pub type HeadEntry = CacheEntry<Vec<u8>>;

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

pub fn get_epoch_seconds() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap()
}

pub fn has_elapsed(time: &u64, dur: &Duration) -> bool {
    let now = get_epoch_seconds();
    time + dur.as_secs() < now
}
