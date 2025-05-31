use crate::cache::Cache;
use crate::cache::entry::Cached::{Expired, Hit, Miss};
use crate::cache::entry::{CapeData, HeadData, SkinData, UuidData};
use crate::cache::entry::{Dated, Entry, ProfileData};
use crate::cache::level::CacheLevel;
use crate::config::Config;
use crate::error::ServiceError;
use crate::error::ServiceError::{NotFound, Unavailable};
use crate::metrics::{PROFILE_REQ_AGE, PROFILE_REQ_LAT, ProfileAgeLabels, ProfileLatLabels};
use crate::mojang;
use crate::mojang::{
    ALEX_HEAD, ALEX_SKIN, ApiError, CLASSIC_MODEL, Mojang, SLIM_MODEL, STEVE_HEAD, STEVE_SKIN,
    build_skin_head,
};
use metrics::MetricsEvent;
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock};
use tracing::warn;
use uuid::Uuid;

/// The username regex is used to check if a given username could be a valid username.
/// If a string does not match the regex, the mojang API will never find a matching user id.
static USERNAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[a-zA-Z0-9_]{2,16}$").expect("failed to compile username regex"));

fn metrics_age_handler<T: Clone + Debug + Eq>(event: MetricsEvent<Result<Dated<T>, ServiceError>>) {
    let status = match event.result {
        Ok(_) => "ok",
        Err(Unavailable) => "unavailable",
        Err(NotFound) => "not_found",
        Err(_) => "error",
    };
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    PROFILE_REQ_LAT
        .get_or_create(&ProfileLatLabels {
            request_type,
            status,
        })
        .observe(event.time);

    if let Ok(dated) = event.result {
        PROFILE_REQ_AGE
            .get_or_create(&ProfileAgeLabels { request_type })
            .observe(dated.current_age() as f64);
    }
}

fn metrics_handler<T: Clone + Debug + Eq>(event: MetricsEvent<Result<T, ServiceError>>) {
    let status = match event.result {
        Ok(_) => "ok",
        Err(Unavailable) => "unavailable",
        Err(NotFound) => "not_found",
        Err(_) => "error",
    };
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    PROFILE_REQ_LAT
        .get_or_create(&ProfileLatLabels {
            request_type,
            status,
        })
        .observe(event.time);
}

/// The [Service] is the backbone of Xenos. All exposed services (gRPC/REST) use a shared instance of
/// this service. The [Service] incorporates a [Cache] and [Mojang] implementations
/// as well as a clone of the [application config](Config). It is expected, that the config
/// match the config used to construct the cache and api.
pub struct Service<L, R, M>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    config: Arc<Config>,
    cache: Cache<L, R>,
    mojang: M,
}

impl<L, R, M> Service<L, R, M>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    /// Builds a new [Service] with provided cache and mojang api implementation. It is expected, that
    /// the provided config match the config used to construct the cache and api.
    pub fn new(config: Arc<Config>, cache: Cache<L, R>, mojang: M) -> Self {
        Self {
            config,
            cache,
            mojang,
        }
    }

    /// Returns the [application config](Config) that were used to construct the [Service].
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Resolves the provided (case-insensitive) username to its (case-sensitive) username and uuid
    /// from cache or mojang.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "uuid"), handler = metrics_age_handler)]
    pub async fn get_uuid(&self, username: &str) -> Result<Dated<UuidData>, ServiceError> {
        // try to get from cache
        let cached = self.cache.get_uuid(username).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to fetch from mojang and update the cache
        match self.mojang.fetch_uuid(username).await {
            Ok(uuid) => {
                let data = UuidData {
                    username: uuid.name,
                    uuid: uuid.id,
                };
                let dated = self.cache.set_uuid(username, Some(data)).await.unwrap();
                Ok(dated)
            }
            Err(ApiError::NotFound) => {
                self.cache.set_uuid(username, None).await;
                Err(NotFound)
            }
            Err(ApiError::Unavailable) => fallback
                .ok_or(Unavailable)
                .and_then(|entry| entry.some_or(NotFound)),
        }
    }

    /// Resolves the provided (case-insensitive) usernames to their (case-sensitive) username and uuid
    /// from cache or mojang.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "uuids"), handler = metrics_handler)]
    pub async fn get_uuids(
        &self,
        usernames: &[String],
    ) -> Result<HashMap<String, Entry<UuidData>>, ServiceError> {
        // 1. initialize with uuid not found
        // contrary to the mojang api, we want all requested usernames to map to something instead of
        // being omitted in case the username is invalid/unused
        let mut uuids: HashMap<String, Entry<UuidData>> = HashMap::from_iter(
            usernames
                .iter()
                .map(|username| (username.to_lowercase(), Dated::from(None))),
        );

        // append cache expired onto cache misses so that the misses are fetched first
        // if cache misses are only expired values, then it forms a valid response
        let mut cache_misses = vec![];
        let mut cache_expired = vec![];
        let mut has_misses = false;
        for (username, uuid) in uuids.iter_mut() {
            // 2. filter invalid usernames (regex)
            // evidently unused (invalid) usernames should not clutter the cache, nor should they fill
            // to the mojang request rate limit. As such, they are excluded beforehand
            if !USERNAME_REGEX.is_match(username.as_str()) {
                continue;
            }
            // 3. get from cache; if cache result is expired, try to fetch and refresh
            let cached = self.cache.get_uuid(username).await;
            match cached {
                Hit(entry) => {
                    *uuid = entry;
                }
                Expired(entry) => {
                    *uuid = entry;
                    cache_expired.push(username.clone());
                }
                Miss => {
                    has_misses = true;
                    cache_misses.push(username.clone());
                }
            }
        }
        cache_misses.extend(cache_expired);

        // 4. all others get from mojang in one request
        if !cache_misses.is_empty() {
            let response = match self.mojang.fetch_uuids(&cache_misses).await {
                Ok(r) => r,
                Err(err) => {
                    // 4a. if it has no misses, use (expired) cached entries instead
                    if !has_misses {
                        return Ok(uuids);
                    }
                    return Err(err.into());
                }
            };
            let mut found: HashMap<_, _> = response
                .into_iter()
                .map(|data| (data.name.to_lowercase(), data))
                .collect();
            for username in cache_misses {
                // build new cache entry
                let data = found.remove(&username).map(|res| UuidData {
                    username: res.name.to_string(),
                    uuid: res.id,
                });
                // update response and cache
                let entry = self.cache.set_uuid(&username, data).await;
                uuids.insert(username.clone(), entry);
            }
        }

        Ok(uuids)
    }

    /// Gets the profile for an uuid from cache or mojang.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "profile"), handler = metrics_age_handler)]
    pub async fn get_profile(&self, uuid: &Uuid) -> Result<Dated<ProfileData>, ServiceError> {
        // try to get from the cache
        let cached = self.cache.get_profile(uuid).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to fetch from mojang and update the cache
        match self
            .mojang
            .fetch_profile(uuid, self.config.signed_profiles)
            .await
        {
            Ok(profile) => {
                let dated = self.cache.set_profile(uuid, Some(profile)).await.unwrap();
                Ok(dated)
            }
            Err(ApiError::NotFound) => {
                self.cache.set_profile(uuid, None).await;
                Err(NotFound)
            }
            Err(ApiError::Unavailable) => fallback
                .ok_or(Unavailable)
                .and_then(|entry| entry.some_or(NotFound)),
        }
    }

    /// Gets the profile skin for an uuid from cache or mojang.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "skin"), handler = metrics_age_handler)]
    pub async fn get_skin(&self, uuid: &Uuid) -> Result<Dated<SkinData>, ServiceError> {
        // try to get from the cache
        let cached = self.cache.get_skin(uuid).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to get a profile
        let profile = match self.get_profile(uuid).await {
            Ok(profile) => profile.data,
            Err(Unavailable) => {
                return fallback
                    .ok_or(Unavailable)
                    .and_then(|entry| entry.some_or(NotFound));
            }
            Err(NotFound) => {
                self.cache.set_skin(uuid, None).await;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };

        // get textures or return default skin
        let Some(textures) = profile.get_textures()?.textures.skin else {
            return Ok(Dated::from(get_default_skin(uuid)));
        };
        let skin_model = textures
            .metadata
            .map(|md| md.model)
            // fallback to the classic model (I didn't check that this is the correct default behavior)
            .unwrap_or(CLASSIC_MODEL.to_string());

        // try to fetch from mojang and update the cache
        match self.mojang.fetch_bytes(textures.url).await {
            Ok(skin_bytes) => {
                let skin = SkinData {
                    bytes: skin_bytes.to_vec(),
                    model: skin_model,
                    default: false,
                };
                let dated = self.cache.set_skin(uuid, Some(skin)).await.unwrap();
                Ok(dated)
            }
            // handle NotFound as Unavailable as the profile (and therefore the skin) should exist
            Err(ApiError::NotFound) | Err(ApiError::Unavailable) => fallback
                .ok_or(Unavailable)
                .and_then(|entry| entry.some_or(NotFound)),
        }
    }

    /// Gets the profile cape for an uuid from cache or mojang.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "cape"), handler = metrics_age_handler)]
    pub async fn get_cape(&self, uuid: &Uuid) -> Result<Dated<CapeData>, ServiceError> {
        // try to get from the cache
        let cached = self.cache.get_cape(uuid).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to get the profile
        let profile = match self.get_profile(uuid).await {
            Ok(profile) => profile.data,
            Err(Unavailable) => {
                return fallback
                    .ok_or(Unavailable)
                    .and_then(|entry| entry.some_or(NotFound));
            }
            Err(NotFound) => {
                self.cache.set_cape(uuid, None).await;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };

        // try to get textures
        let Some(textures) = profile.get_textures()?.textures.cape else {
            return Err(NotFound);
        };

        // try to fetch from mojang and update the cache
        match self.mojang.fetch_bytes(textures.url).await {
            Ok(cape_bytes) => {
                let cape = CapeData {
                    bytes: cape_bytes.to_vec(),
                };
                let dated = self.cache.set_cape(uuid, Some(cape)).await.unwrap();
                Ok(dated)
            }
            // handle NotFound as Unavailable as the profile (and therefore the cape) should exist
            Err(ApiError::NotFound) | Err(ApiError::Unavailable) => fallback
                .ok_or(Unavailable)
                .and_then(|entry| entry.some_or(NotFound)),
        }
    }

    /// Gets the profile head for an uuid from cache or mojang. The head may include the head overlay.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "head"), handler = metrics_age_handler)]
    pub async fn get_head(
        &self,
        uuid: &Uuid,
        overlay: bool,
    ) -> Result<Dated<HeadData>, ServiceError> {
        // try to get from the cache
        let cached = self.cache.get_head(&(*uuid, overlay)).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to get skin
        let skin = match self.get_skin(uuid).await {
            Ok(skin) => skin.data,
            Err(Unavailable) => {
                return fallback
                    .ok_or(Unavailable)
                    .and_then(|entry| entry.some_or(NotFound));
            }
            Err(NotFound) => {
                self.cache.set_head(&(*uuid, false), None).await;
                self.cache.set_head(&(*uuid, true), None).await;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };

        // handle default skins
        if skin.default {
            return Ok(Dated::from(get_default_head(uuid)));
        }

        // build head
        let head_bytes = build_skin_head(&skin.bytes, overlay)?;
        let head = HeadData {
            bytes: head_bytes,
            default: skin.default,
        };
        let dated = self
            .cache
            .set_head(&(*uuid, overlay), Some(head))
            .await
            .unwrap();
        Ok(dated)
    }
}

/// Gets the default [SkinData] for a [Uuid].
fn get_default_skin(uuid: &Uuid) -> SkinData {
    match mojang::is_steve(uuid) {
        true => SkinData {
            bytes: STEVE_SKIN.to_vec(),
            model: CLASSIC_MODEL.to_string(),
            default: true,
        },
        false => SkinData {
            bytes: ALEX_SKIN.to_vec(),
            model: SLIM_MODEL.to_string(),
            default: true,
        },
    }
}

/// Gets the default [HeadData] for a [Uuid].
fn get_default_head(uuid: &Uuid) -> HeadData {
    match mojang::is_steve(uuid) {
        true => HeadData {
            bytes: STEVE_HEAD.to_vec(),
            default: true,
        },
        false => HeadData {
            bytes: ALEX_HEAD.to_vec(),
            default: true,
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cache::level::no::NoCache;
    use crate::mojang::testing::MojangTestingApi;
    use uuid::uuid;

    #[tokio::test]
    async fn new_nocache() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();

        // when
        let _ = Service::new(Arc::new(config), cache, mojang);
    }

    #[tokio::test]
    async fn get_uuid_found() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuid("Hydrofin").await;

        // then
        let expected_hydrofin = UuidData {
            username: "Hydrofin".to_string(),
            uuid: uuid!("09879557e47945a9b434a56377674627"),
        };
        assert!(matches!(result, Ok(Dated{ data, .. }) if data == expected_hydrofin));
    }

    #[tokio::test]
    async fn get_uuid_not_found() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuid("xXSlayer42Xx").await;

        // then
        assert!(matches!(result, Err(NotFound)));
    }

    #[tokio::test]
    async fn get_uuid_invalid() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuid("56789äas#").await;

        // then
        assert!(matches!(result, Err(NotFound)));
    }

    #[tokio::test]
    async fn get_uuid_empty_not_found() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::new();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuid("Hydrofin").await;

        // then
        assert!(matches!(result, Err(NotFound)));
    }

    #[tokio::test]
    async fn get_uuids_found() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuids(&vec!["Hydrofin".to_string()]).await;

        // then
        match result {
            Ok(resolved) => {
                assert_eq!(1, resolved.len());

                // User 'Hydrofin' is found
                let Some(hydrofin) = resolved.get("hydrofin") else {
                    panic!("failed to resolve user 'Hydrofin'")
                };
                assert_eq!(
                    hydrofin.data,
                    Some(UuidData {
                        username: "Hydrofin".to_string(),
                        uuid: uuid!("09879557e47945a9b434a56377674627")
                    }),
                );
            }
            Err(err) => panic!("failed to resolve uuid: {}", err),
        }
    }

    #[tokio::test]
    async fn get_uuids_not_found() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuids(&vec!["xXSlayer42Xx".to_string()]).await;

        // then
        match result {
            Ok(resolved) => {
                assert_eq!(1, resolved.len());

                // User 'xXSlayer42Xx' not found
                let other = resolved.get("xxslayer42xx");
                assert!(matches!(other, Some(Dated { data: None, .. })));
            }
            Err(err) => panic!("failed to resolve uuid: {}", err),
        }
    }

    #[tokio::test]
    async fn get_uuids_invalid() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service.get_uuids(&vec!["#+".to_string()]).await;

        // then
        match result {
            Ok(resolved) => {
                assert_eq!(1, resolved.len());

                // User '#+' not found
                let other = resolved.get("#+");
                assert!(matches!(other, Some(Dated { data: None, .. })));
            }
            Err(err) => panic!("failed to resolve uuid: {}", err),
        }
    }

    #[tokio::test]
    async fn get_uuids_partial_found() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service
            .get_uuids(&vec!["Hydrofin".to_string(), "xXSlayer42Xx".to_string()])
            .await;

        // then
        match result {
            Ok(resolved) => {
                assert_eq!(2, resolved.len());

                // User 'xXSlayer42Xx' not found
                let other = resolved.get("xxslayer42xx");
                assert!(matches!(other, Some(Dated { data: None, .. })));

                // User 'Hydrofin' is found
                let Some(hydrofin) = resolved.get("hydrofin") else {
                    panic!("failed to resolve user 'Hydrofin'")
                };
                assert_eq!(
                    hydrofin.data,
                    Some(UuidData {
                        username: "Hydrofin".to_string(),
                        uuid: uuid!("09879557e47945a9b434a56377674627")
                    }),
                );
            }
            Err(err) => panic!("failed to resolve uuid: {}", err),
        }
    }

    #[tokio::test]
    async fn get_uuids_partial_invalid() {
        // given
        let config = Config::default();
        let cache = Cache::new(config.cache.entries.clone(), NoCache, NoCache);
        let mojang = MojangTestingApi::with_profiles();
        let service = Service::new(Arc::new(config), cache, mojang);

        // when
        let result = service
            .get_uuids(&vec!["Hydrofin".to_string(), "i<ia9".to_string()])
            .await;

        // then
        match result {
            Ok(resolved) => {
                assert_eq!(2, resolved.len());

                // User 'i<ia9' not found
                let other = resolved.get("i<ia9");
                assert!(matches!(other, Some(Dated { data: None, .. })));

                // User 'Hydrofin' is found
                let Some(hydrofin) = resolved.get("hydrofin") else {
                    panic!("failed to resolve user 'Hydrofin'")
                };
                assert_eq!(
                    hydrofin.data,
                    Some(UuidData {
                        username: "Hydrofin".to_string(),
                        uuid: uuid!("09879557e47945a9b434a56377674627")
                    }),
                );
            }
            Err(err) => panic!("failed to resolve uuid: {}", err),
        }
    }
}
