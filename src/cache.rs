use crate::api::{Profile, UsernameResolved};
use crate::XenosError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;
use worker::{console_debug, RouteContext};

pub trait XenosCache {
    // usernames cache
    async fn get_user_id(&self, username: &String) -> Result<Option<UsernameResolved>, XenosError>;
    async fn put_user_id(&self, username: UsernameResolved) -> Result<(), XenosError>;
    // profile cache
    async fn get_profile(&self, user_id: &Uuid) -> Result<Option<Profile>, XenosError>;
    async fn put_profile(&self, profile: Profile) -> Result<(), XenosError>;
    // skin and head cache
    async fn get_skin(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError>;
    async fn put_skin(&self, user_id: &Uuid, skin: Vec<u8>) -> Result<(), XenosError>;
    async fn get_head(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError>;
    async fn put_head(&self, user_id: &Uuid, head: Vec<u8>) -> Result<(), XenosError>;
}

async fn get<T: DeserializeOwned>(
    ctx: &RouteContext<()>,
    prefix: &str,
    key: &str,
) -> Result<Option<T>, XenosError> {
    console_debug!("Getting cache for '{}_{}'", prefix, key);
    Ok(ctx
        .kv("xenos")
        .map_err(|err| XenosError::CacheRetrieve(err))?
        .get(&*format!("{}_{}", prefix, key))
        .json()
        .await
        .map_err(|err| XenosError::Cache(err))?)
}

async fn put<T: Serialize>(
    ctx: &RouteContext<()>,
    prefix: &str,
    key: &str,
    val: T,
    ttl: u64,
) -> Result<(), XenosError> {
    console_debug!("Putting cache for '{}_{}'", prefix, key);
    Ok(ctx
        .kv("xenos")
        .map_err(|err| XenosError::CacheRetrieve(err))?
        .put(&*format!("{}_{}", prefix, key), val)
        .map_err(|err| XenosError::Cache(err))?
        .expiration_ttl(ttl)
        .execute()
        .await
        .map_err(|err| XenosError::Cache(err))?)
}

async fn get_bytes(
    ctx: &RouteContext<()>,
    prefix: &str,
    key: &str,
) -> Result<Option<Vec<u8>>, XenosError> {
    console_debug!("Getting cache for '{}_{}'", prefix, key);
    Ok(ctx
        .kv("xenos")
        .map_err(|err| XenosError::CacheRetrieve(err))?
        .get(&*format!("{}_{}", prefix, key))
        .bytes()
        .await
        .map_err(|err| XenosError::Cache(err))?)
}

async fn put_bytes(
    ctx: &RouteContext<()>,
    prefix: &str,
    key: &str,
    val: &[u8],
    ttl: u64,
) -> Result<(), XenosError> {
    console_debug!("Putting cache for '{}_{}'", prefix, key);
    Ok(ctx
        .kv("xenos")
        .map_err(|err| XenosError::CacheRetrieve(err))?
        .put_bytes(&*format!("{}_{}", prefix, key), val)
        .map_err(|err| XenosError::Cache(err))?
        .expiration_ttl(ttl)
        .execute()
        .await
        .map_err(|err| XenosError::Cache(err))?)
}

impl XenosCache for RouteContext<()> {
    async fn get_user_id(&self, username: &String) -> Result<Option<UsernameResolved>, XenosError> {
        get(self, "username", username.to_lowercase().as_str()).await
    }

    async fn put_user_id(&self, username: UsernameResolved) -> Result<(), XenosError> {
        // cache unknown usernames for longer
        let mut ttl = 60;
        if username.id.is_nil() {
            ttl = 600;
        }
        put(
            self,
            "username",
            username.name.to_lowercase().as_str(),
            username,
            ttl,
        )
        .await
    }

    async fn get_profile(&self, user_id: &Uuid) -> Result<Option<Profile>, XenosError> {
        get(self, "profile", user_id.simple().to_string().as_str()).await
    }

    async fn put_profile(&self, profile: Profile) -> Result<(), XenosError> {
        put(
            self,
            "profile",
            profile.id.simple().to_string().as_str(),
            profile,
            60,
        )
        .await
    }

    async fn get_skin(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError> {
        get_bytes(self, "skin", user_id.simple().to_string().as_str()).await
    }

    async fn put_skin(&self, user_id: &Uuid, skin: Vec<u8>) -> Result<(), XenosError> {
        put_bytes(
            self,
            "skin",
            user_id.simple().to_string().as_str(),
            skin.as_slice(),
            60,
        )
        .await
    }

    async fn get_head(&self, user_id: &Uuid) -> Result<Option<Vec<u8>>, XenosError> {
        get_bytes(self, "head", user_id.simple().to_string().as_str()).await
    }

    async fn put_head(&self, user_id: &Uuid, head: Vec<u8>) -> Result<(), XenosError> {
        put_bytes(
            self,
            "head",
            user_id.simple().to_string().as_str(),
            head.as_slice(),
            60,
        )
        .await
    }
}
