use crate::cache::{
    monitor_cache_get, monitor_cache_set, CacheEntry, Cached, HeadEntry, IntoCached, ProfileEntry,
    SkinEntry, UuidEntry, XenosCache,
};
use crate::error::XenosError;
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
    // expiry settings
    ttl: Option<usize>,
    expiry_uuid: u64,
    expiry_uuid_missing: u64,
    expiry_profile: u64,
    expiry_profile_missing: u64,
    expiry_skin: u64,
    expiry_skin_missing: u64,
    expiry_head: u64,
    expiry_head_missing: u64,
    // redis connection
    redis_manager: Arc<Mutex<ConnectionManager>>,
}

impl Debug for RedisCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // prints all fields except the redis connection
        f.debug_struct("RedisCache")
            .field("ttl", &self.ttl)
            .field("expiry_uuid", &self.expiry_uuid)
            .field("expiry_uuid_missing", &self.expiry_uuid_missing)
            .field("expiry_profile", &self.expiry_profile)
            .field("expiry_profile_missing", &self.expiry_profile_missing)
            .field("expiry_skin", &self.expiry_skin)
            .field("expiry_skin_missing", &self.expiry_skin_missing)
            .field("expiry_head", &self.expiry_head)
            .field("expiry_head_missing", &self.expiry_head_missing)
            .finish()
    }
}

impl RedisCache {
    /// Created a new empty [Redis Cache](RedisCache) with no expiry (~585 aeons).
    /// Use successive builder methods to set expiry and ttl explicitly.
    pub fn new(con: ConnectionManager) -> Self {
        Self {
            ttl: None,
            expiry_uuid: u64::MAX,
            expiry_uuid_missing: u64::MAX,
            expiry_profile: u64::MAX,
            expiry_profile_missing: u64::MAX,
            expiry_skin: u64::MAX,
            expiry_skin_missing: u64::MAX,
            expiry_head: u64::MAX,
            expiry_head_missing: u64::MAX,
            redis_manager: Arc::new(Mutex::new(con)),
        }
    }

    /// Sets the expiry for username to uuid cache facet.
    pub fn with_expiry_uuid(mut self, default: u64, missing: u64) -> Self {
        self.expiry_uuid = default;
        self.expiry_uuid_missing = missing;
        self
    }

    /// Sets the expiry for uuid to profile cache facet.
    pub fn with_expiry_profile(mut self, default: u64, missing: u64) -> Self {
        self.expiry_profile = default;
        self.expiry_profile_missing = missing;
        self
    }

    /// Sets the expiry for uuid to skin cache facet.
    pub fn with_expiry_skin(mut self, default: u64, missing: u64) -> Self {
        self.expiry_skin = default;
        self.expiry_skin_missing = missing;
        self
    }

    /// Sets the expiry for uuid to head cache facet.
    pub fn with_expiry_head(mut self, default: u64, missing: u64) -> Self {
        self.expiry_head = default;
        self.expiry_head_missing = missing;
        self
    }

    /// Sets the expiry for all entry types (default/missing).
    pub fn with_ttl(mut self, ttl: Option<usize>) -> Self {
        self.ttl = ttl;
        self
    }

    /// Generates [SetOptions] from the cache configuration. Mostly used to set the optional ttl.
    fn build_set_options(&self) -> SetOptions {
        let mut opts = SetOptions::default();
        if let Some(ttl) = self.ttl {
            opts = opts.with_expiration(SetExpiry::EX(ttl));
        }
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
            let cached = entry.into_cached(&self.expiry_uuid, &self.expiry_uuid_missing);
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
            let cached = entry.into_cached(&self.expiry_profile, &self.expiry_profile_missing);
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
            let cached = entry.into_cached(&self.expiry_skin, &self.expiry_skin_missing);
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
            let cached = entry.into_cached(&self.expiry_head, &self.expiry_head_missing);
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
