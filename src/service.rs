use crate::cache::entry::Cached::{Expired, Hit, Miss};
use crate::cache::entry::{CapeData, HeadData, SkinData, UuidData};
use crate::cache::entry::{Dated, Entry, ProfileData};
use crate::cache::level::CacheLevel;
use crate::cache::Cache;
use crate::error::ServiceError;
use crate::error::ServiceError::{NotFound, Unavailable};
use crate::mojang;
use crate::mojang::{
    build_skin_head, ApiError, Mojang, ALEX_HEAD, ALEX_SKIN, CLASSIC_MODEL, SLIM_MODEL, STEVE_HEAD,
    STEVE_SKIN,
};
use crate::settings::Settings;
use lazy_static::lazy_static;
use metrics::MetricsEvent;
use prometheus::{register_histogram_vec, HistogramVec};
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

lazy_static! {
    /// The username regex is used to check if a given username could be a valid username.
    /// If a string does not match the regex, the mojang API will never find a matching user id.
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();

    /// A histogram for the age in seconds of cache results. Use the [monitor_service_call_with_age]
    /// utility for ease of use.
    pub static ref PROFILE_REQ_AGE_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_age_seconds",
        "The grpc profile response age in seconds.",
        &["request_type"],
        vec![5.0, 10.0, 60.0, 600.0, 3600.0, 86400.0, 604800.0, 2419200.0]
    )
    .unwrap();

    /// A histogram for the service call latency in seconds with response status. Use the
    /// [monitor_service_call] utility for ease of use.
    pub static ref PROFILE_REQ_LAT_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_latency_seconds",
        "The grpc profile request latency in seconds.",
        &["request_type", "status"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

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
    PROFILE_REQ_LAT_HISTOGRAM
        .with_label_values(&[request_type, status])
        .observe(event.time);

    if let Ok(dated) = event.result {
        PROFILE_REQ_AGE_HISTOGRAM
            .with_label_values(&[request_type])
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
    PROFILE_REQ_LAT_HISTOGRAM
        .with_label_values(&[request_type, status])
        .observe(event.time);
}

/// The [Service] is the backbone of Xenos. All exposed services (gRPC/REST) use a shared instance of
/// this service. The [Service] incorporates a [Cache] and [Mojang] implementations
/// as well as a clone of the [application settings](Settings). It is expected, that the settings
/// match the settings used to construct the cache and api.
pub struct Service<L, R, M>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    settings: Arc<Settings>,
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
    /// the provided settings match the settings used to construct the cache and api.
    pub fn new(settings: Arc<Settings>, cache: Cache<L, R>, mojang: M) -> Self {
        Self {
            settings,
            cache,
            mojang,
        }
    }

    /// Returns the [application settings](Settings) that were used to construct the [Service].
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Resolves the provided (case-insensitive) username to its (case-sensitive) username and uuid
    /// from cache or mojang.
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(metric = "service", labels(request_type = "uuid"), handler = metrics_age_handler)]
    pub async fn get_uuid(&self, username: &str) -> Result<Dated<UuidData>, ServiceError> {
        let mut uuids = self.get_uuids(&[username.to_string()]).await?;
        match uuids.remove(&username.to_lowercase()) {
            Some(uuid) => uuid.some_or(NotFound),
            None => Err(NotFound),
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
            // evidently unused (invalid) usernames should not clutter the cache nor should they fill
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
        // try to get from cache
        let cached = self.cache.get_profile(uuid).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to fetch from mojang and update cache
        match self
            .mojang
            .fetch_profile(uuid, self.settings.signed_profiles)
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
        // try to get from cache
        let cached = self.cache.get_skin(uuid).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to get profile
        let profile = match self.get_profile(uuid).await {
            Ok(profile) => profile.data,
            Err(Unavailable) => {
                return fallback
                    .ok_or(Unavailable)
                    .and_then(|entry| entry.some_or(NotFound))
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
            // fallback to classic model (I didn't check that this is the correct default behavior)
            .unwrap_or(CLASSIC_MODEL.to_string());

        // try to fetch from mojang and update cache
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
        // try to get from cache
        let cached = self.cache.get_cape(uuid).await;
        let fallback = match cached {
            Hit(entry) => return entry.some_or(NotFound),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to get profile
        let profile = match self.get_profile(uuid).await {
            Ok(profile) => profile.data,
            Err(Unavailable) => {
                return fallback
                    .ok_or(Unavailable)
                    .and_then(|entry| entry.some_or(NotFound))
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

        // try to fetch from mojang and update cache
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
        // try to get from cache
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
                    .and_then(|entry| entry.some_or(NotFound))
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
