use crate::cache::Cached::*;
use crate::cache::{
    get_epoch_seconds, CacheEntry, HeadEntry, ProfileEntry, SkinEntry, UuidData, UuidEntry,
    XenosCache,
};
use crate::error::XenosError;
use crate::error::XenosError::{InvalidTextures, NotFound, NotRetrieved};
use crate::mojang::Mojang;
use crate::settings::Settings;
use image::{imageops, ColorType, GenericImageView, ImageOutputFormat};
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

lazy_static! {
    /// The username regex is used to check if a given username could be a valid username.
    /// If a string does not match the regex, the mojang API will never find a matching user id.
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();
}

// TODO update buckets
lazy_static! {
    /// A histogram for the age in seconds of cache results. Use the [monitor_service_call_with_age]
    /// utility for ease of use.
    pub static ref PROFILE_REQ_AGE_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_age_seconds",
        "The grpc profile response age in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
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

/// A utility that wraps a [Service] call, monitoring its runtime, response status and [CacheEntry]
/// age for [prometheus]. The age of a [CacheEntry] is the relative time from which the cache entry
/// was created until now.
async fn monitor_service_call_with_age<F, Fut, D>(
    request_type: &str,
    f: F,
) -> Result<CacheEntry<D>, XenosError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<CacheEntry<D>, XenosError>>,
    D: Debug + Clone + Eq,
{
    let result = monitor_service_call(request_type, f).await;
    match &result {
        Ok(entry) => {
            // if cache entry found, monitor its age
            PROFILE_REQ_AGE_HISTOGRAM
                .with_label_values(&[request_type])
                .observe((get_epoch_seconds() - entry.timestamp) as f64);
        }
        _ => {}
    };
    result
}

/// A utility that wraps a [Service] call, monitoring its runtime and response status.
async fn monitor_service_call<F, Fut, D>(request_type: &str, f: F) -> Result<D, XenosError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<D, XenosError>>,
    D: Debug + Clone + Eq,
{
    let start = Instant::now();
    let result = f().await;
    let status = match &result {
        Ok(_) => "ok",
        Err(NotRetrieved) => "not_retrieved",
        Err(NotFound) => "not_found",
        Err(_) => "error",
    };
    PROFILE_REQ_LAT_HISTOGRAM
        .with_label_values(&[request_type, status])
        .observe(start.elapsed().as_secs_f64());
    result
}

/// The [Service] is the backbone of Xenos. All exposed services (gRPC/REST) use a shared instance of
/// this service. The [Service] incorporates a [cache](XenosCache) and [mojang api](Mojang) implementations
/// as well as a clone of the [application settings](Settings). It is expected, that the settings
/// match the settings used to construct the cache and api.
pub struct Service {
    settings: Arc<Settings>,
    cache: Box<dyn XenosCache>,
    mojang: Box<dyn Mojang>,
}

impl Service {
    /// Builds a new [Service] with provided cache and mojang api implementation. It is expected, that
    /// the provided settings match the settings used to construct the cache and api.
    pub fn new(
        settings: Arc<Settings>,
        cache: Box<dyn XenosCache>,
        mojang: Box<dyn Mojang>,
    ) -> Self {
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

    /// Builds the head image bytes from a skin. Expects a valid skin.
    #[tracing::instrument(skip(skin_bytes))]
    fn build_skin_head(skin_bytes: &[u8], overlay: &bool) -> Result<Vec<u8>, XenosError> {
        let skin_img = image::load_from_memory_with_format(skin_bytes, image::ImageFormat::Png)?;
        let mut head_img = skin_img.view(8, 8, 8, 8).to_image();

        if *overlay {
            let overlay_head_img = skin_img.view(40, 8, 8, 8).to_image();
            imageops::overlay(&mut head_img, &overlay_head_img, 0, 0);
        }

        let mut head_bytes: Vec<u8> = Vec::new();
        let mut cur = Cursor::new(&mut head_bytes);
        image::write_buffer_with_format(
            &mut cur,
            &head_img,
            8,
            8,
            ColorType::Rgba8,
            ImageOutputFormat::Png,
        )?;
        Ok(head_bytes)
    }

    /// Resolves the provided (case-insensitive) usernames to their (case-sensitive) username and uuid
    /// from cache or mojang.
    #[tracing::instrument(skip(self))]
    pub async fn get_uuids(
        &self,
        usernames: &[String],
    ) -> Result<HashMap<String, UuidEntry>, XenosError> {
        monitor_service_call("uuids", || self._get_uuids(usernames)).await
    }

    pub async fn _get_uuids(
        &self,
        usernames: &[String],
    ) -> Result<HashMap<String, UuidEntry>, XenosError> {
        // 1. initialize with uuid not found
        // contrary to the mojang api, we want all requested usernames to map to something instead of
        // being omitted in case the username is invalid/unused
        let mut uuids: HashMap<String, UuidEntry> = HashMap::from_iter(
            usernames
                .iter()
                .map(|username| (username.to_lowercase(), UuidEntry::new_empty())),
        );

        let mut cache_misses = vec![];
        for (username, uuid) in uuids.iter_mut() {
            // 2. filter invalid usernames (regex)
            // evidently unused (invalid) usernames should not clutter the cache nor should they fill
            // to the mojang request rate limit. As such, they are excluded beforehand
            if !USERNAME_REGEX.is_match(username.as_str()) {
                continue;
            }
            // 3. get from cache; if cache result is expired, try to refresh cache
            let cached = self.cache.get_uuid_by_username(username).await?;
            match cached {
                Hit(entry) => {
                    *uuid = entry;
                }
                Expired(entry) => {
                    *uuid = entry;
                    cache_misses.push(username.clone());
                }
                Miss => {
                    cache_misses.push(username.clone());
                }
            }
        }

        // 4. all others get from mojang in one request
        if !cache_misses.is_empty() {
            let response = match self.mojang.fetch_uuids(&cache_misses).await {
                Ok(r) => r,
                // currently, partial responses are not supported
                Err(err) => return Err(err),
            };
            let found: HashMap<_, _> = response
                .into_iter()
                .map(|data| (data.name.to_lowercase(), data))
                .collect();
            for username in cache_misses {
                // build new cache entry
                let data = found.get(&username).cloned().map(|res| UuidData {
                    username: res.name,
                    uuid: res.id,
                });
                let entry = UuidEntry {
                    timestamp: get_epoch_seconds(),
                    data,
                };
                // update response and cache
                uuids.insert(username.clone(), entry.clone());
                self.cache.set_uuid_by_username(&username, entry).await?;
            }
        }

        Ok(uuids)
    }

    /// Gets the profile for an uuid from cache or mojang.
    #[tracing::instrument(skip(self))]
    pub async fn get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        monitor_service_call_with_age("profile", || self._get_profile(uuid)).await
    }

    async fn _get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        // try to get from cache
        let cached = self.cache.get_profile_by_uuid(uuid).await?;
        let fallback = match cached {
            Hit(entry) => return Ok(entry),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to fetch from mojang and update cache
        let profile = match self.mojang.fetch_profile(uuid).await {
            Ok(profile) => profile,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(NotFound) => {
                self.cache
                    .set_profile_by_uuid(*uuid, ProfileEntry::new_empty())
                    .await?;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };

        let entry = ProfileEntry::new(profile.into());
        self.cache.set_profile_by_uuid(*uuid, entry.clone()).await?;
        Ok(entry)
    }

    /// Gets the profile skin for an uuid from cache or mojang.
    #[tracing::instrument(skip(self))]
    pub async fn get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        monitor_service_call_with_age("skin", || self._get_skin(uuid)).await
    }

    async fn _get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        // try to get from cache
        let cached = self.cache.get_skin_by_uuid(uuid).await?;
        let fallback = match cached {
            Hit(entry) => return Ok(entry),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        let profile_data = match self.get_profile(uuid).await {
            Ok(ProfileEntry {
                data: Some(profile_data),
                ..
            }) => profile_data,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Ok(_) | Err(NotFound) => {
                self.cache
                    .set_skin_by_uuid(*uuid, SkinEntry::new_empty())
                    .await?;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };

        let skin_url = profile_data
            .get_textures()?
            .textures
            .skin
            .ok_or(InvalidTextures("skin missing".to_string()))?
            .url;
        let skin = match self.mojang.fetch_image_bytes(skin_url, "skin").await {
            Ok(skin) => skin,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(NotFound) => {
                self.cache
                    .set_skin_by_uuid(*uuid, SkinEntry::new_empty())
                    .await?;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };
        let entry = SkinEntry::new(skin.to_vec());
        self.cache.set_skin_by_uuid(*uuid, entry.clone()).await?;
        Ok(entry)
    }

    /// Gets the profile head for an uuid from cache or mojang. The head may include the head overlay.
    #[tracing::instrument(skip(self))]
    pub async fn get_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
        monitor_service_call_with_age("head", || self._get_head(uuid, overlay)).await
    }

    async fn _get_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
        let cached = self.cache.get_head_by_uuid(uuid, overlay).await?;
        let fallback = match cached {
            Hit(entry) => return Ok(entry),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        let skin_data = match self.get_skin(uuid).await {
            Ok(SkinEntry {
                data: Some(skin_data),
                ..
            }) => skin_data,
            Ok(_) | Err(NotFound) => {
                self.cache
                    .set_head_by_uuid(*uuid, HeadEntry::new_empty(), overlay)
                    .await?;
                return Err(NotFound);
            }
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };

        let head_bytes = Self::build_skin_head(&skin_data, overlay)?;

        let entry = HeadEntry::new(head_bytes);
        self.cache
            .set_head_by_uuid(*uuid, entry.clone(), overlay)
            .await?;
        Ok(entry)
    }
}
