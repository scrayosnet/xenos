use crate::error::XenosError;
use crate::mojang::TexturesProperty;
use async_trait::async_trait;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use redis::aio::ConnectionManager;
use redis::{
    from_redis_value, AsyncCommands, FromRedisValue, RedisResult, RedisWrite, ToRedisArgs, Value,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UuidEntry {
    pub timestamp: u64,
    pub username: String,
    pub uuid: Uuid,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinEntry {
    pub timestamp: u64,
    pub uuid: Uuid,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeadEntry {
    pub timestamp: u64,
    pub uuid: Uuid,
    pub bytes: Vec<u8>,
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

// type serialization

impl FromRedisValue for UuidEntry {
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let v: String = from_redis_value(v)?;
        Ok(serde_json::from_str(&v)?)
    }
}

impl ToRedisArgs for UuidEntry {
    fn write_redis_args<W>(&self, out: &mut W)
        where
            W: ?Sized + RedisWrite,
    {
        let str = serde_json::to_string(self).unwrap_or("".to_string());
        out.write_arg(str.as_ref())
    }
}

impl FromRedisValue for ProfileEntry {
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let v: String = from_redis_value(v)?;
        Ok(serde_json::from_str(&v)?)
    }
}

impl ToRedisArgs for ProfileEntry {
    fn write_redis_args<W>(&self, out: &mut W)
        where
            W: ?Sized + RedisWrite,
    {
        let str = serde_json::to_string(self).unwrap_or("".to_string());
        out.write_arg(str.as_ref())
    }
}

impl FromRedisValue for SkinEntry {
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let v: String = from_redis_value(v)?;
        Ok(serde_json::from_str(&v)?)
    }
}

impl ToRedisArgs for SkinEntry {
    fn write_redis_args<W>(&self, out: &mut W)
        where
            W: ?Sized + RedisWrite,
    {
        let str = serde_json::to_string(self).unwrap_or("".to_string());
        out.write_arg(str.as_ref())
    }
}

impl FromRedisValue for HeadEntry {
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let v: String = from_redis_value(v)?;
        Ok(serde_json::from_str(&v)?)
    }
}

impl ToRedisArgs for HeadEntry {
    fn write_redis_args<W>(&self, out: &mut W)
        where
            W: ?Sized + RedisWrite,
    {
        let str = serde_json::to_string(self).unwrap_or("".to_string());
        out.write_arg(str.as_ref())
    }
}

// cache instance

#[async_trait]
pub trait XenosCache: Send + Sync {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Option<UuidEntry>, XenosError>;
    async fn set_uuid_by_username(&mut self, entry: UuidEntry) -> Result<(), XenosError>;
    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Option<ProfileEntry>, XenosError>;
    async fn set_profile_by_uuid(&mut self, entry: ProfileEntry) -> Result<(), XenosError>;
    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Option<SkinEntry>, XenosError>;
    async fn set_skin_by_uuid(&mut self, entry: SkinEntry) -> Result<(), XenosError>;
    async fn get_head_by_uuid(&mut self, uuid: &Uuid, overlay: &bool) -> Result<Option<HeadEntry>, XenosError>;
    async fn set_head_by_uuid(&mut self, entry: HeadEntry, overlay: &bool) -> Result<(), XenosError>;
}

pub struct RedisCache {
    pub redis_manager: ConnectionManager,
}

pub fn build_key(ns: &str, sub: &str) -> String {
    format!("xenos.{ns}.{sub}")
}

#[async_trait]
impl XenosCache for RedisCache {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Option<UuidEntry>, XenosError> {
        let entry = self
            .redis_manager
            .get(build_key("uuid", username.to_lowercase().as_str()))
            .await?;
        Ok(entry)
    }

    async fn set_uuid_by_username(&mut self, entry: UuidEntry) -> Result<(), XenosError> {
        self.redis_manager
            .set(
                build_key("uuid", entry.username.to_lowercase().as_str()),
                entry,
            )
            .await?;
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Option<ProfileEntry>, XenosError> {
        let entry = self
            .redis_manager
            .get(build_key("profile", uuid.simple().to_string().as_str()))
            .await?;
        Ok(entry)
    }

    async fn set_profile_by_uuid(&mut self, entry: ProfileEntry) -> Result<(), XenosError> {
        self.redis_manager
            .set(
                build_key("profile", entry.uuid.simple().to_string().as_str()),
                entry,
            )
            .await?;
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Option<SkinEntry>, XenosError> {
        let entry = self
            .redis_manager
            .get(build_key("skin", uuid.simple().to_string().as_str()))
            .await?;
        Ok(entry)
    }

    async fn set_skin_by_uuid(&mut self, entry: SkinEntry) -> Result<(), XenosError> {
        self.redis_manager
            .set(
                build_key("skin", entry.uuid.simple().to_string().as_str()),
                entry,
            )
            .await?;
        Ok(())
    }

    async fn get_head_by_uuid(&mut self, uuid: &Uuid, overlay: &bool) -> Result<Option<HeadEntry>, XenosError> {
        let uuid = uuid.simple().to_string();
        let entry = self
            .redis_manager
            .get(build_key("head", &format!("{uuid}.{overlay}")))
            .await?;
        Ok(entry)
    }

    async fn set_head_by_uuid(&mut self, entry: HeadEntry, overlay: &bool) -> Result<(), XenosError> {
        let uuid = entry.uuid.simple().to_string();
        self.redis_manager
            .set(
                build_key("head", &format!("{uuid}.{overlay}")),
                entry,
            )
            .await?;
        Ok(())
    }
}
