use crate::cache::Cached::{Expired, Hit, Miss};
use crate::cache::{Cached, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use crate::util::has_elapsed;
use async_trait::async_trait;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec};
use redis::aio::ConnectionManager;
use redis::{
    from_redis_value, AsyncCommands, FromRedisValue, RedisResult, RedisWrite, ToRedisArgs, Value,
};
use uuid::Uuid;

lazy_static! {
    pub static ref REDIS_SET_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xenos_redis_set_total",
        "Total number of set requests to the redis cache.",
        &["request_type"],
    )
    .unwrap();
    pub static ref REDIS_GET_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xenos_redis_get_total",
        "Total number of get requests to the redis cache.",
        &["request_type", "cache_result"],
    )
    .unwrap();
    pub static ref REDIS_GET_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_redis_get_duration_seconds",
        "The redis get request latencies in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

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
        let timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["uuid"])
            .start_timer();
        let cached: Option<UuidEntry> = self
            .redis_manager
            .get(build_key("uuid", username.to_lowercase().as_str()))
            .await?;
        timer.observe_duration();
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => {
                REDIS_GET_TOTAL
                    .with_label_values(&["uuid", "expired"])
                    .inc();
                Ok(Expired(entry))
            }
            Some(entry) => {
                REDIS_GET_TOTAL.with_label_values(&["uuid", "hit"]).inc();
                Ok(Hit(entry))
            }
            None => {
                REDIS_GET_TOTAL.with_label_values(&["uuid", "miss"]).inc();
                Ok(Miss)
            }
        }
    }

    async fn set_uuid_by_username(
        &mut self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        REDIS_SET_TOTAL.with_label_values(&["uuid"]).inc();
        self.redis_manager
            .set(build_key("uuid", username.to_lowercase().as_str()), entry)
            .await?;
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError> {
        let timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["profile"])
            .start_timer();
        let cached: Option<ProfileEntry> = self
            .redis_manager
            .get(build_key("profile", uuid.simple().to_string().as_str()))
            .await?;
        timer.observe_duration();
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => {
                REDIS_GET_TOTAL
                    .with_label_values(&["profile", "expired"])
                    .inc();
                Ok(Expired(entry))
            }
            Some(entry) => {
                REDIS_GET_TOTAL.with_label_values(&["profile", "hit"]).inc();
                Ok(Hit(entry))
            }
            None => {
                REDIS_GET_TOTAL
                    .with_label_values(&["profile", "miss"])
                    .inc();
                Ok(Miss)
            }
        }
    }

    async fn set_profile_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: ProfileEntry,
    ) -> Result<(), XenosError> {
        REDIS_SET_TOTAL.with_label_values(&["profile"]).inc();
        self.redis_manager
            .set(
                build_key("profile", uuid.simple().to_string().as_str()),
                entry,
            )
            .await?;
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        let timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["skin"])
            .start_timer();
        let cached: Option<SkinEntry> = self
            .redis_manager
            .get(build_key("skin", uuid.simple().to_string().as_str()))
            .await?;
        timer.observe_duration();
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => {
                REDIS_GET_TOTAL
                    .with_label_values(&["skin", "expired"])
                    .inc();
                Ok(Expired(entry))
            }
            Some(entry) => {
                REDIS_GET_TOTAL.with_label_values(&["skin", "hit"]).inc();
                Ok(Hit(entry))
            }
            None => {
                REDIS_GET_TOTAL.with_label_values(&["skin", "miss"]).inc();
                Ok(Miss)
            }
        }
    }

    async fn set_skin_by_uuid(&mut self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        REDIS_SET_TOTAL.with_label_values(&["skin"]).inc();
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
        let timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["head"])
            .start_timer();
        let uuid_str = uuid.simple().to_string();
        let cached: Option<HeadEntry> = self
            .redis_manager
            .get(build_key("head", &format!("{uuid_str}.{overlay}")))
            .await?;
        timer.observe_duration();
        match cached {
            Some(entry) if self.has_expired(&entry.timestamp) => {
                REDIS_GET_TOTAL
                    .with_label_values(&["head", "expired"])
                    .inc();
                Ok(Expired(entry))
            }
            Some(entry) => {
                REDIS_GET_TOTAL.with_label_values(&["head", "hit"]).inc();
                Ok(Hit(entry))
            }
            None => {
                REDIS_GET_TOTAL.with_label_values(&["head", "miss"]).inc();
                Ok(Miss)
            }
        }
    }

    async fn set_head_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: &bool,
    ) -> Result<(), XenosError> {
        let uuid_str = uuid.simple().to_string();
        REDIS_SET_TOTAL.with_label_values(&["head"]).inc();
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
