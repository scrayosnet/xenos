use crate::cache::Cached::{Expired, Hit, Miss};
use crate::cache::{Cached, HeadEntry, IntoCached, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use async_trait::async_trait;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec};
use redis::aio::ConnectionManager;
use redis::{
    from_redis_value, AsyncCommands, FromRedisValue, RedisResult, RedisWrite, SetExpiry,
    SetOptions, ToRedisArgs, Value,
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

fn track_cache_result<T>(cached: &Cached<T>, request_type: &str) {
    match cached {
        Expired(_) => {
            REDIS_SET_TOTAL
                .with_label_values(&[request_type, "expired"])
                .inc();
        }
        Hit(_) => {
            REDIS_SET_TOTAL
                .with_label_values(&[request_type, "hit"])
                .inc();
        }
        Miss => {
            REDIS_SET_TOTAL
                .with_label_values(&[request_type, "miss"])
                .inc();
        }
    }
}

pub struct RedisCache {
    pub cache_time: u64,
    pub expiration: Option<usize>,
    pub redis_manager: ConnectionManager,
}

impl RedisCache {
    fn build_set_options(&self) -> SetOptions {
        let mut opts = SetOptions::default();
        if let Some(expiration) = self.expiration {
            opts = opts.with_expiration(SetExpiry::EX(expiration));
        }
        opts
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
        let _timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["uuid"])
            .start_timer();
        let entry: Option<UuidEntry> = self
            .redis_manager
            .get(build_key("uuid", username.to_lowercase().as_str()))
            .await?;
        let cached = entry.into_cached(&self.cache_time);
        track_cache_result(&cached, "uuid");
        Ok(cached)
    }

    async fn set_uuid_by_username(
        &mut self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        REDIS_SET_TOTAL.with_label_values(&["uuid"]).inc();
        self.redis_manager
            .set_options(
                build_key("uuid", username.to_lowercase().as_str()),
                entry,
                self.build_set_options(),
            )
            .await?;
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError> {
        let _timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["profile"])
            .start_timer();
        let entry: Option<ProfileEntry> = self
            .redis_manager
            .get(build_key("profile", uuid.simple().to_string().as_str()))
            .await?;
        let cached = entry.into_cached(&self.cache_time);
        track_cache_result(&cached, "profile");
        Ok(cached)
    }

    async fn set_profile_by_uuid(
        &mut self,
        uuid: Uuid,
        entry: ProfileEntry,
    ) -> Result<(), XenosError> {
        REDIS_SET_TOTAL.with_label_values(&["profile"]).inc();
        self.redis_manager
            .set_options(
                build_key("profile", uuid.simple().to_string().as_str()),
                entry,
                self.build_set_options(),
            )
            .await?;
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        let _timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["skin"])
            .start_timer();
        let entry: Option<SkinEntry> = self
            .redis_manager
            .get(build_key("skin", uuid.simple().to_string().as_str()))
            .await?;
        let cached = entry.into_cached(&self.cache_time);
        track_cache_result(&cached, "skin");
        Ok(cached)
    }

    async fn set_skin_by_uuid(&mut self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        REDIS_SET_TOTAL.with_label_values(&["skin"]).inc();
        self.redis_manager
            .set_options(
                build_key("skin", uuid.simple().to_string().as_str()),
                entry,
                self.build_set_options(),
            )
            .await?;
        Ok(())
    }

    async fn get_head_by_uuid(
        &mut self,
        uuid: &Uuid,
        overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        let _timer = REDIS_GET_HISTOGRAM
            .with_label_values(&["head"])
            .start_timer();
        let uuid_str = uuid.simple().to_string();
        let entry: Option<HeadEntry> = self
            .redis_manager
            .get(build_key("head", &format!("{uuid_str}.{overlay}")))
            .await?;
        let cached = entry.into_cached(&self.cache_time);
        track_cache_result(&cached, "head");
        Ok(cached)
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
            .set_options(
                build_key("head", &format!("{uuid_str}.{overlay}")),
                entry,
                self.build_set_options(),
            )
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
