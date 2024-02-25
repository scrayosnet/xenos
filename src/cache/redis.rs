use crate::cache::{
    monitor_cache_get, monitor_cache_set, CacheEntry, Cached, HeadEntry, IntoCached, ProfileEntry,
    SkinEntry, UuidEntry, XenosCache,
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

// TODO add docu
pub struct RedisCache {
    settings: settings::RedisCache,
    redis_manager: Arc<Mutex<ConnectionManager>>,
}

impl Debug for RedisCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // prints all fields except the redis connection
        f.debug_struct("RedisCache")
            .field("settings", &self.settings)
            .finish()
    }
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

    /// Generates [SetOptions] from the cache configuration. Mostly used to set the optional ttl.
    fn build_set_options(&self) -> SetOptions {
        let mut opts = SetOptions::default();
        // TODO reimplement ttl!
        //if let Some(ttl) = self.ttl {
        //    opts = opts.with_expiration(SetExpiry::EX(ttl));
        //}
        opts
    }
}

// TODO add docu
pub fn build_key(ns: &str, sub: &str) -> String {
    format!("xenos.{ns}.{sub}")
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
                &self.settings.entries.uuid.expiry,
                &self.settings.entries.uuid.expiry_missing,
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
                    self.build_set_options(),
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
                &self.settings.entries.profile.expiry,
                &self.settings.entries.profile.expiry_missing,
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
                    self.build_set_options(),
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
                &self.settings.entries.skin.expiry,
                &self.settings.entries.skin.expiry_missing,
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
                    self.build_set_options(),
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
        overlay: &bool,
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
                &self.settings.entries.head.expiry,
                &self.settings.entries.head.expiry_missing,
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
        overlay: &bool,
    ) -> Result<(), XenosError> {
        monitor_cache_set("redis", "head", || async {
            let uuid_str = uuid.simple().to_string();
            self.redis_manager
                .lock()
                .await
                .set_options(
                    build_key("head", &format!("{uuid_str}.{overlay}")),
                    entry,
                    self.build_set_options(),
                )
                .await?;
            Ok(())
        })
        .await
    }
}

// type serialization

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
