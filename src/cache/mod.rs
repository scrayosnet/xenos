pub mod memory;
pub mod redis;
pub mod uncached;

use crate::cache::Cached::{Expired, Hit, Miss};
use crate::error::XenosError;
use crate::mojang::TexturesProperty;
use async_trait::async_trait;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Cached<T> {
    Hit(T),
    Expired(T),
    Miss,
}

trait IntoCached<T> {
    fn into_cached(self, ttl: &u64) -> Cached<T>;
}

impl<T> IntoCached<T> for Option<T>
where
    T: CacheEntry,
{
    fn into_cached(self, ttl: &u64) -> Cached<T> {
        match self {
            None => Miss,
            Some(v) if has_elapsed(&v.get_timestamp(), ttl) => Expired(v),
            Some(v) => Hit(v),
        }
    }
}

pub trait CacheEntry {
    fn get_timestamp(&self) -> u64;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UuidEntry {
    pub timestamp: u64,
    pub username: String,
    pub uuid: Uuid,
}

impl CacheEntry for UuidEntry {
    fn get_timestamp(&self) -> u64 {
        self.timestamp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileEntry {
    pub timestamp: u64,
    pub uuid: Uuid,
    pub name: String,
    #[serde(default)]
    pub properties: Vec<ProfileProperty>,
    #[serde(default)]
    pub profile_actions: Vec<String>,
}

impl CacheEntry for ProfileEntry {
    fn get_timestamp(&self) -> u64 {
        self.timestamp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinEntry {
    pub timestamp: u64,
    pub bytes: Vec<u8>,
}

impl CacheEntry for SkinEntry {
    fn get_timestamp(&self) -> u64 {
        self.timestamp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeadEntry {
    pub timestamp: u64,
    pub bytes: Vec<u8>,
}

impl CacheEntry for HeadEntry {
    fn get_timestamp(&self) -> u64 {
        self.timestamp
    }
}

impl ProfileEntry {
    pub fn get_textures(&self) -> Result<TexturesProperty, XenosError> {
        let prop = self
            .properties
            .iter()
            .find(|prop| prop.name == *"textures")
            .ok_or(XenosError::InvalidTextures("missing".to_string()))?;
        ProfileEntry::parse_texture_prop(prop.value.clone())
    }

    fn parse_texture_prop(b64: String) -> Result<TexturesProperty, XenosError> {
        let json = BASE64_STANDARD
            .decode(b64)
            .map_err(|_err| XenosError::InvalidTextures("base64 decode failed".to_string()))?;
        serde_json::from_slice::<TexturesProperty>(&json)
            .map_err(|_err| XenosError::InvalidTextures("json decode failed".to_string()))
    }
}

#[async_trait]
pub trait XenosCache: Send + Sync {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Cached<UuidEntry>, XenosError>;
    async fn set_uuid_by_username(
        &mut self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError>;
    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError>;
    async fn set_profile_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: ProfileEntry,
    ) -> Result<(), XenosError>;
    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError>;
    async fn set_skin_by_uuid(&mut self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError>;
    async fn get_head_by_uuid(
        &mut self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError>;
    async fn set_head_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError>;
}

pub fn get_epoch_seconds() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap()
}

pub fn has_elapsed(time: &u64, dur: &u64) -> bool {
    let now = get_epoch_seconds();
    time + dur < now
}
