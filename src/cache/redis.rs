use crate::cache::Cached::{Expired, Hit, Miss};
use crate::cache::{Cached, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use crate::util::has_elapsed;
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::{
    from_redis_value, AsyncCommands, FromRedisValue, RedisResult, RedisWrite, ToRedisArgs, Value,
};
use uuid::Uuid;

pub struct RedisCache {
    pub cache_time: u64,
    pub redis_manager: ConnectionManager,
}

impl RedisCache {
    fn has_expired(&self, timestamp: &u64) -> bool {
        has_elapsed(timestamp, &self.cache_time)
    }
}

pub fn build_key(ns: &str, sub: &str) -> String {
    format!("xenos.{ns}.{sub}")
}

#[async_trait]
impl XenosCache for RedisCache {
    async fn get_uuid_by_username(
        &mut self,
        username: &str,
    ) -> Result<Cached<UuidEntry>, XenosError> {
        let cached: Option<UuidEntry> = self
            .redis_manager
            .get(build_key("uuid", username.to_lowercase().as_str()))
            .await?;
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => Ok(Expired(entry)),
            Some(entry) => Ok(Hit(entry)),
            None => Ok(Miss),
        }
    }

    async fn set_uuid_by_username(
        &mut self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        self.redis_manager
            .set(build_key("uuid", username.to_lowercase().as_str()), entry)
            .await?;
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError> {
        let cached: Option<ProfileEntry> = self
            .redis_manager
            .get(build_key("profile", uuid.simple().to_string().as_str()))
            .await?;
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => Ok(Expired(entry)),
            Some(entry) => Ok(Hit(entry)),
            None => Ok(Miss),
        }
    }

    async fn set_profile_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: ProfileEntry,
    ) -> Result<(), XenosError> {
        self.redis_manager
            .set(
                build_key("profile", uuid.simple().to_string().as_str()),
                entry,
            )
            .await?;
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        let cached: Option<SkinEntry> = self
            .redis_manager
            .get(build_key("skin", uuid.simple().to_string().as_str()))
            .await?;
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => Ok(Expired(entry)),
            Some(entry) => Ok(Hit(entry)),
            None => Ok(Miss),
        }
    }

    async fn set_skin_by_uuid(&mut self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        self.redis_manager
            .set(build_key("skin", uuid.simple().to_string().as_str()), entry)
            .await?;
        Ok(())
    }

    async fn get_head_by_uuid(
        &mut self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        let uuid_str = uuid.simple().to_string();
        let cached: Option<HeadEntry> = self
            .redis_manager
            .get(build_key("head", &format!("{uuid_str}.{overlay}")))
            .await?;
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => Ok(Expired(entry)),
            Some(entry) => Ok(Hit(entry)),
            None => Ok(Miss),
        }
    }

    async fn set_head_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError> {
        let uuid_str = uuid.simple().to_string();
        self.redis_manager
            .set(build_key("head", &format!("{uuid_str}.{overlay}")), entry)
            .await?;
        Ok(())
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
