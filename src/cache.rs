use serde::de::DeserializeOwned;
use serde::{Serialize};
use uuid::Uuid;
use worker::{console_debug, RouteContext};
use crate::api::Profile;
use crate::XenosError;

pub trait XenosCache {
    // generic access
    async fn get<'de, T: DeserializeOwned>(&self, prefix: &str, key: &str) -> Result<Option<T>, XenosError>;
    async fn put<T: Serialize>(&self, prefix: &str, key: &str, val: T, ttl: u64) -> Result<(), XenosError>;
    async fn get_bytes(&self, prefix: &str, key: &str) -> Result<Option<Vec<u8>>, XenosError>;
    async fn put_bytes(&self, prefix: &str, key: &str, val: &[u8], ttl: u64) -> Result<(), XenosError>;
    // profile cache
    async fn get_profile(&self, user_id: &Uuid) -> Result<Option<Profile>, XenosError>;
    async fn put_profile(&self, profile: Profile) -> Result<(), XenosError>;
    // skin and head cache
    async fn get_skin(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError>;
    async fn put_skin(&self, user_id: &Uuid, skin: Vec<u8>) -> Result<(), XenosError>;
    async fn get_head(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError>;
    async fn put_head(&self, user_id: &Uuid, head: Vec<u8>) -> Result<(), XenosError>;
}

impl XenosCache for RouteContext<()> {

    async fn get<'de, T: DeserializeOwned>(&self, prefix: &str, key: &str) -> Result<Option<T>, XenosError> {
        console_debug!("Getting cache for '{}_{}'", prefix, key);
        Ok(
            self.kv("xenos")
                .map_err(|err| XenosError::CacheRetrieve(err))?
                .get(&*format!("{}_{}", prefix, key))
                .json().await
                .map_err(|err| XenosError::Cache(err))?
        )
    }

    async fn put<T: Serialize>(&self, prefix: &str, key: &str, val: T, ttl: u64) -> Result<(), XenosError> {
        console_debug!("Putting cache for '{}_{}'", prefix, key);
        Ok(
            self.kv("xenos")
                .map_err(|err| XenosError::CacheRetrieve(err))?
                .put(&*format!("{}_{}", prefix, key), val)
                .map_err(|err| XenosError::Cache(err))?
                .expiration_ttl(ttl)
                .execute().await
                .map_err(|err| XenosError::Cache(err))?
        )
    }

    async fn get_bytes(&self, prefix: &str, key: &str) -> Result<Option<Vec<u8>>, XenosError> {
        console_debug!("Getting cache for '{}_{}'", prefix, key);
        Ok(
            self.kv("xenos")
                .map_err(|err| XenosError::CacheRetrieve(err))?
                .get(&*format!("{}_{}", prefix, key))
                .bytes().await
                .map_err(|err| XenosError::Cache(err))?
        )
    }

    async fn put_bytes(&self, prefix: &str, key: &str, val: &[u8], ttl: u64) -> Result<(), XenosError> {
        console_debug!("Putting cache for '{}_{}'", prefix, key);
        Ok(
            self.kv("xenos")
                .map_err(|err| XenosError::CacheRetrieve(err))?
                .put_bytes(&*format!("{}_{}", prefix, key), val)
                .map_err(|err| XenosError::Cache(err))?
                .expiration_ttl(ttl)
                .execute().await
                .map_err(|err| XenosError::Cache(err))?
        )
    }

    async fn get_profile(&self, user_id: &Uuid) -> Result<Option<Profile>, XenosError> {
        self.get("profile", user_id.simple().to_string().as_str()).await
    }

    async fn put_profile(&self, profile: Profile) -> Result<(), XenosError> {
        self.put("profile", profile.id.simple().to_string().as_str(), profile, 30).await
    }

    async fn get_skin(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError> {
        self.get_bytes("skin", user_id.simple().to_string().as_str()).await
    }

    async fn put_skin(&self, user_id: &Uuid, skin: Vec<u8>) -> Result<(), XenosError> {
        self.put_bytes("skin", user_id.simple().to_string().as_str(), skin.as_slice(), 30).await
    }

    async fn get_head(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError> {
        self.get_bytes("head", user_id.simple().to_string().as_str()).await
    }

    async fn put_head(&self, user_id: &Uuid, head: Vec<u8>) -> Result<(), XenosError> {
        self.put_bytes("head", user_id.simple().to_string().as_str(), head.as_slice(), 30).await
    }
}
