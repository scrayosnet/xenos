use crate::cache::{
    monitor_cache_get, monitor_cache_set, CacheEntry, Cached, CapeEntry, HeadEntry, IntoCached,
    ProfileEntry, SkinEntry, UuidEntry, XenosCache,
};
use crate::error::XenosError;
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
use tokio::sync::Mutex;
use uuid::Uuid;

/// Builds a redis cache entry key. The ns (namespace) represents the resource type and sub identifies
/// the resource from its peers.
pub fn build_key(ns: &str, sub: &str) -> String {
    format!("xenos.{ns}.{sub}")
}

/// [Redis Cache](RedisCache) is a [cache](XenosCache) implementation using redis. The cache has an
/// additional expiration (delete) policies with time-to-live.
pub struct RedisCache {
    settings: settings::RedisCache,
    redis_manager: Arc<Mutex<ConnectionManager>>,
}

impl RedisCache {
    /// Created a new empty [Redis Cache](RedisCache) with no expiry (~585 aeons).
    /// Use successive builder methods to set expiry and ttl explicitly.
    pub fn new(con: ConnectionManager, settings: &settings::RedisCache) -> Self {
        Self {
            settings: settings.clone(),
            redis_manager: Arc::new(Mutex::new(con)),
        }
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
impl XenosCache for RedisCache {
    #[tracing::instrument(skip(self))]
    async fn get_uuid_by_username(&self, username: &str) -> Result<Cached<UuidEntry>, XenosError> {
        monitor_cache_get("redis", "uuid", || async {
            let entry: Option<UuidEntry> = self
                .redis_manager
                .lock()
                .await
                .get(build_key("uuid", username.to_lowercase().as_str()))
                .await?;
            let cached = entry.into_cached(
                &self.settings.entries.uuid.exp,
                &self.settings.entries.uuid.exp_na,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_uuid_by_username(
        &self,
        username: &str,
        entry: UuidEntry,
    ) -> Result<(), XenosError> {
        monitor_cache_set("redis", "uuid", || async {
            self.redis_manager
                .lock()
                .await
                .set_options(
                    build_key("uuid", username.to_lowercase().as_str()),
                    entry,
                    SetOptions::default().with_expiration(SetExpiry::EX(
                        self.settings.entries.uuid.ttl.as_secs() as usize,
                    )),
                )
                .await?;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_profile_by_uuid(&self, uuid: &Uuid) -> Result<Cached<ProfileEntry>, XenosError> {
        monitor_cache_get("redis", "profile", || async {
            let entry: Option<ProfileEntry> = self
                .redis_manager
                .lock()
                .await
                .get(build_key("profile", uuid.simple().to_string().as_str()))
                .await?;
            let cached = entry.into_cached(
                &self.settings.entries.profile.exp,
                &self.settings.entries.profile.exp_na,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_profile_by_uuid(&self, uuid: Uuid, entry: ProfileEntry) -> Result<(), XenosError> {
        monitor_cache_set("redis", "profile", || async {
            self.redis_manager
                .lock()
                .await
                .set_options(
                    build_key("profile", uuid.simple().to_string().as_str()),
                    entry,
                    SetOptions::default().with_expiration(SetExpiry::EX(
                        self.settings.entries.profile.ttl.as_secs() as usize,
                    )),
                )
                .await?;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_skin_by_uuid(&self, uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        monitor_cache_get("redis", "skin", || async {
            let entry: Option<SkinEntry> = self
                .redis_manager
                .lock()
                .await
                .get(build_key("skin", uuid.simple().to_string().as_str()))
                .await?;
            let cached = entry.into_cached(
                &self.settings.entries.skin.exp,
                &self.settings.entries.skin.exp_na,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_skin_by_uuid(&self, uuid: Uuid, entry: SkinEntry) -> Result<(), XenosError> {
        monitor_cache_set("redis", "skin", || async {
            self.redis_manager
                .lock()
                .await
                .set_options(
                    build_key("skin", uuid.simple().to_string().as_str()),
                    entry,
                    SetOptions::default().with_expiration(SetExpiry::EX(
                        self.settings.entries.skin.ttl.as_secs() as usize,
                    )),
                )
                .await?;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_cape_by_uuid(&self, uuid: &Uuid) -> Result<Cached<CapeEntry>, XenosError> {
        monitor_cache_get("redis", "cape", || async {
            let entry: Option<CapeEntry> = self
                .redis_manager
                .lock()
                .await
                .get(build_key("cape", uuid.simple().to_string().as_str()))
                .await?;
            let cached = entry.into_cached(
                &self.settings.entries.cape.exp,
                &self.settings.entries.cape.exp_na,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_cape_by_uuid(&self, uuid: Uuid, entry: CapeEntry) -> Result<(), XenosError> {
        monitor_cache_set("redis", "cape", || async {
            self.redis_manager
                .lock()
                .await
                .set_options(
                    build_key("cape", uuid.simple().to_string().as_str()),
                    entry,
                    SetOptions::default().with_expiration(SetExpiry::EX(
                        self.settings.entries.cape.ttl.as_secs() as usize,
                    )),
                )
                .await?;
            Ok(())
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn get_head_by_uuid(
        &self,
        uuid: &Uuid,
        overlay: bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        monitor_cache_get("redis", "head", || async {
            let uuid_str = uuid.simple().to_string();
            let entry: Option<HeadEntry> = self
                .redis_manager
                .lock()
                .await
                .get(build_key("head", &format!("{uuid_str}.{overlay}")))
                .await?;
            let cached = entry.into_cached(
                &self.settings.entries.head.exp,
                &self.settings.entries.head.exp_na,
            );
            Ok(cached)
        })
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn set_head_by_uuid(
        &self,
        uuid: Uuid,
        entry: HeadEntry,
        overlay: bool,
    ) -> Result<(), XenosError> {
        monitor_cache_set("redis", "head", || async {
            let uuid_str = uuid.simple().to_string();
            self.redis_manager
                .lock()
                .await
                .set_options(
                    build_key("head", &format!("{uuid_str}.{overlay}")),
                    entry,
                    SetOptions::default().with_expiration(SetExpiry::EX(
                        self.settings.entries.head.ttl.as_secs() as usize,
                    )),
                )
                .await?;
            Ok(())
        })
        .await
    }
}

impl<D> FromRedisValue for CacheEntry<D>
where
    D: Debug + Clone + PartialEq + Eq + DeserializeOwned,
{
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let v: String = from_redis_value(v)?;
        Ok(serde_json::from_str(&v)?)
    }
}

impl<D> ToRedisArgs for CacheEntry<D>
where
    D: Debug + Clone + PartialEq + Eq + Serialize,
{
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + RedisWrite,
    {
        let str = serde_json::to_string(self).unwrap_or("".to_string());
        out.write_arg(str.as_ref())
    }
}
