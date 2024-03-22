use crate::cache::entry::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::CacheLevel;
use crate::cache::level::{monitor_get, monitor_set};
use crate::settings;
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::{
    from_redis_value, AsyncCommands, FromRedisValue, RedisResult, RedisWrite, SetExpiry,
    SetOptions, ToRedisArgs, Value,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::error;
use uuid::Uuid;

/// Builds a sting key for the redis cache. The key is prefixed with "xenos".
macro_rules! key {
    ($x1:expr) => {
        format!("xenos.{}", $x1)
    };
    ($x1:expr, $x2:expr) => {
        format!("xenos.{}.{}", $x1, $x2)
    };
    ($x1:expr, $x2:expr, $x3:expr) => {
        format!("xenos.{}.{}.{}", $x1, $x2, $x3)
    };
}

/// [Redis Cache](RedisCache) is a [cache](XenosCache) implementation using redis. The cache has an
/// additional expiration (delete) policies with time-to-live.
pub struct RedisCache {
    settings: settings::RedisCache,
    redis_manager: Arc<Mutex<ConnectionManager>>,
}

impl RedisCache {
    /// Created a new empty [Redis Cache](RedisCache).
    pub fn new(con: ConnectionManager, settings: &settings::RedisCache) -> Self {
        Self {
            settings: settings.clone(),
            redis_manager: Arc::new(Mutex::new(con)),
        }
    }

    #[tracing::instrument(skip(self))]
    async fn get<D>(&self, key: String) -> Option<Entry<D>>
    where
        D: Clone + Debug + Eq + PartialEq + DeserializeOwned,
    {
        self.redis_manager
            .lock()
            .await
            .get(key)
            .await
            .unwrap_or_else(|err| {
                error!("Failed to get value from redis: {}", err);
                None
            })
    }

    #[tracing::instrument(skip(self))]
    async fn set<D>(&self, key: String, entry: Entry<D>, ttl: &Duration)
    where
        D: Clone + Debug + Eq + PartialEq + Send + Sync + Serialize,
    {
        self.redis_manager
            .lock()
            .await
            .set_options(
                key,
                entry,
                SetOptions::default().with_expiration(SetExpiry::EX(ttl.as_secs() as usize)),
            )
            .await
            .unwrap_or_else(|err| {
                error!("Failed to set value to redis: {}", err);
            });
    }
}

impl Debug for RedisCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // prints all fields except the redis connection
        f.debug_struct("RedisCache")
            .field("settings", &self.settings)
            .finish()
    }
}

#[async_trait]
impl CacheLevel for RedisCache {
    async fn get_uuid(&self, username: &str) -> Option<Entry<UuidData>> {
        let key = key!("uuid", username.to_lowercase());
        monitor_get("redis", "uuid", || self.get(key)).await
    }

    async fn set_uuid(&self, username: String, entry: Entry<UuidData>) {
        let key = key!("uuid", username.to_lowercase());
        monitor_set("redis", "uuid", || {
            self.set(key, entry, &self.settings.entries.uuid.ttl)
        })
        .await
    }

    async fn get_profile(&self, uuid: &Uuid) -> Option<Entry<ProfileData>> {
        let key = key!("profile", uuid.simple());
        monitor_get("redis", "profile", || self.get(key)).await
    }

    async fn set_profile(&self, uuid: Uuid, entry: Entry<ProfileData>) {
        let key = key!("profile", uuid.simple());
        monitor_set("redis", "profile", || {
            self.set(key, entry, &self.settings.entries.profile.ttl)
        })
        .await
    }

    async fn get_skin(&self, uuid: &Uuid) -> Option<Entry<SkinData>> {
        let key = key!("skin", uuid.simple());
        monitor_get("redis", "skin", || self.get(key)).await
    }

    async fn set_skin(&self, uuid: Uuid, entry: Entry<SkinData>) {
        let key = key!("skin", uuid.simple());
        monitor_set("redis", "skin", || {
            self.set(key, entry, &self.settings.entries.skin.ttl)
        })
        .await
    }

    async fn get_cape(&self, uuid: &Uuid) -> Option<Entry<CapeData>> {
        let key = key!("cape", uuid.simple());
        monitor_get("redis", "cape", || self.get(key)).await
    }

    async fn set_cape(&self, uuid: Uuid, entry: Entry<CapeData>) {
        let key = key!("cape", uuid.simple());
        monitor_set("redis", "cape", || {
            self.set(key, entry, &self.settings.entries.cape.ttl)
        })
        .await
    }

    async fn get_head(&self, uuid: &Uuid, overlay: bool) -> Option<Entry<HeadData>> {
        let key = key!("head", uuid.simple(), overlay);
        monitor_get("redis", "head", || self.get(key)).await
    }

    async fn set_head(&self, uuid: Uuid, overlay: bool, entry: Entry<HeadData>) {
        let key = key!("head", uuid.simple(), overlay);
        monitor_set("redis", "head", || {
            self.set(key, entry, &self.settings.entries.head.ttl)
        })
        .await
    }
}

impl<D> FromRedisValue for Entry<D>
where
    D: Clone + Debug + Eq + PartialEq + DeserializeOwned,
{
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let v: String = from_redis_value(v)?;
        Ok(serde_json::from_str(&v)?)
    }
}

impl<D> ToRedisArgs for Entry<D>
where
    D: Clone + Debug + Eq + PartialEq + Serialize,
{
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + RedisWrite,
    {
        let str = serde_json::to_string(self).unwrap_or("".to_string());
        out.write_arg(str.as_ref())
    }
}
