use crate::cache::Cached::*;
use crate::cache::{
    get_epoch_seconds, CacheEntry, HeadEntry, ProfileEntry, SkinEntry, UuidData, UuidEntry,
    XenosCache,
};
use crate::error::XenosError;
use crate::error::XenosError::{InvalidTextures, NotRetrieved};
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
use XenosError::NotFound;

lazy_static! {
    /// The username regex is used to check if a given username could be a valid username.
    /// If a string does not match the regex, the mojang API will never find a matching user id.
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();
}

// TODO update buckets
lazy_static! {
    /// A histogram for the age in seconds of cache results. Use the [monitor_service_call]
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

async fn monitor_service_call<F, Fut, D>(
    request_type: &str,
    f: F,
) -> Result<CacheEntry<D>, XenosError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<CacheEntry<D>, XenosError>>,
    D: Debug + Clone + Eq,
{
    let start = Instant::now();
    let result = f().await;
    let status = match &result {
        Ok(entry) => {
            PROFILE_REQ_AGE_HISTOGRAM
                .with_label_values(&[request_type])
                .observe((get_epoch_seconds() - entry.timestamp) as f64);
            "ok"
        }
        Err(NotRetrieved) => "not_retrieved",
        Err(NotFound) => "not_found",
        Err(_) => "error",
    };
    PROFILE_REQ_LAT_HISTOGRAM
        .with_label_values(&[request_type, status])
        .observe(start.elapsed().as_secs_f64());
    result
}

pub struct Service {
    settings: Arc<Settings>,
    cache: Box<dyn XenosCache>,
    mojang: Box<dyn Mojang>,
}

// service api
impl Service {
    /// Builds a new [Service] with selected cache and mojang api implementation.
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

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    #[tracing::instrument(skip(skin_bytes))]
    fn build_skin_head(skin_bytes: &[u8]) -> Result<Vec<u8>, XenosError> {
        let skin_img = image::load_from_memory_with_format(skin_bytes, image::ImageFormat::Png)?;
        let mut head_img = skin_img.view(8, 8, 8, 8).to_image();
        let overlay_head_img = skin_img.view(40, 8, 8, 8).to_image();
        imageops::overlay(&mut head_img, &overlay_head_img, 0, 0);

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

    #[tracing::instrument(skip(self))]
    pub async fn get_uuids(
        &self,
        usernames: &[String],
    ) -> Result<HashMap<String, UuidEntry>, XenosError> {
        // 1. initialize with uuid not found
        let mut uuids: HashMap<String, UuidEntry> = HashMap::from_iter(
            usernames
                .iter()
                .map(|username| (username.to_lowercase(), UuidEntry::new_empty())),
        );

        let mut cache_misses = vec![];
        for (username, uuid) in uuids.iter_mut() {
            // 2. filter invalid (regex)
            if !USERNAME_REGEX.is_match(username.as_str()) {
                continue;
            }
            // 3. get from cache; if elapsed, try to refresh
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
                Err(NotRetrieved) => return Ok(uuids),
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

    #[tracing::instrument(skip(self))]
    pub async fn get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        monitor_service_call("profile", || self._get_profile(uuid)).await
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

    #[tracing::instrument(skip(self))]
    pub async fn get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        monitor_service_call("skin", || self._get_skin(uuid)).await
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

    #[tracing::instrument(skip(self))]
    pub async fn get_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
        monitor_service_call("head", || self._get_head(uuid, overlay)).await
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

        let head_bytes = Self::build_skin_head(&skin_data)?;

        let entry = HeadEntry::new(head_bytes);
        self.cache
            .set_head_by_uuid(*uuid, entry.clone(), overlay)
            .await?;
        Ok(entry)
    }
}
