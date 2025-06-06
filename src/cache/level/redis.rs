use crate::cache::entry::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::{CacheLevel, metrics_get_handler, metrics_set_handler};
use crate::config;
use redis::aio::ConnectionManager;
use redis::{
    AsyncCommands, FromRedisValue, RedisResult, RedisWrite, SetExpiry, SetOptions, ToRedisArgs,
    Value, from_redis_value,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
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

/// [Redis Cache](RedisCache) is a [CacheLevel] implementation using redis. The cache has an
/// additional expiration (delete) policies with time-to-live.
///
/// Should redis encounter any error while getting or setting data, the errors are logged and default
/// values are returned. This is done to prevent the application from "crashing" as soon as redis is,
/// for example, temporarily unavailable.
pub struct RedisCache {
    config: config::RedisCache,
    redis_manager: Arc<Mutex<ConnectionManager>>,
}

impl RedisCache {
    /// Created a new [Redis Cache](RedisCache).
    pub fn new(con: ConnectionManager, config: &config::RedisCache) -> Self {
        Self {
            config: config.clone(),
            redis_manager: Arc::new(Mutex::new(con)),
        }
    }

    /// Utility for getting some [Entry] from redis. Handles errors by logging them and returning `None`.
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
                error!("Failed to get value from redis: {:?}", err);
                None
            })
    }

    /// Utility for setting some [Entry] to redis. Handles errors by logging them.
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
                SetOptions::default().with_expiration(SetExpiry::EX(ttl.as_secs())),
            )
            .await
            .unwrap_or_else(|err| {
                error!("Failed to set value to redis: {:?}", err);
            });
    }
}

impl Debug for RedisCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // prints all fields except the redis connection
        f.debug_struct("RedisCache")
            .field("config", &self.config)
            .finish()
    }
}

impl CacheLevel for RedisCache {
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_variant = "redis", request_type = "uuid"),
        handler = metrics_get_handler
    )]
    async fn get_uuid(&self, key: &str) -> Option<Entry<UuidData>> {
        let key = key!("uuid", key.to_lowercase());
        self.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_variant = "redis", request_type = "uuid"),
        handler = metrics_set_handler
    )]
    async fn set_uuid(&self, key: &str, entry: Entry<UuidData>) {
        let key = key!("uuid", key.to_lowercase());
        self.set(key, entry, &self.config.entries.uuid.ttl).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_variant = "redis", request_type = "profile"),
        handler = metrics_get_handler
    )]
    async fn get_profile(&self, key: &Uuid) -> Option<Entry<ProfileData>> {
        let key = key!("profile", key.simple());
        self.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_variant = "redis", request_type = "profile"),
        handler = metrics_set_handler
    )]
    async fn set_profile(&self, key: &Uuid, entry: Entry<ProfileData>) {
        let key = key!("profile", key.simple());
        self.set(key, entry, &self.config.entries.profile.ttl).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_variant = "redis", request_type = "skin"),
        handler = metrics_get_handler
    )]
    async fn get_skin(&self, key: &Uuid) -> Option<Entry<SkinData>> {
        let key = key!("skin", key.simple());
        self.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_variant = "redis", request_type = "skin"),
        handler = metrics_set_handler
    )]
    async fn set_skin(&self, key: &Uuid, entry: Entry<SkinData>) {
        let key = key!("skin", key.simple());
        self.set(key, entry, &self.config.entries.skin.ttl).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_variant = "redis", request_type = "cape"),
        handler = metrics_get_handler
    )]
    async fn get_cape(&self, key: &Uuid) -> Option<Entry<CapeData>> {
        let key = key!("cape", key.simple());
        self.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_variant = "redis", request_type = "cape"),
        handler = metrics_set_handler
    )]
    async fn set_cape(&self, key: &Uuid, entry: Entry<CapeData>) {
        let key = key!("cape", key.simple());
        self.set(key, entry, &self.config.entries.cape.ttl).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_get",
        labels(cache_variant = "redis", request_type = "head"),
        handler = metrics_get_handler
    )]
    async fn get_head(&self, key: &(Uuid, bool)) -> Option<Entry<HeadData>> {
        let key = key!("head", key.0.simple(), key.1);
        self.get(key).await
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "cache_set",
        labels(cache_variant = "redis", request_type = "head"),
        handler = metrics_set_handler
    )]
    async fn set_head(&self, key: &(Uuid, bool), entry: Entry<HeadData>) {
        let key = key!("head", key.0.simple(), key.1);
        self.set(key, entry, &self.config.entries.head.ttl).await
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
