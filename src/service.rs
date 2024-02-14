use crate::cache;
use crate::cache::Cached::*;
use crate::cache::{
    get_epoch_seconds, CacheEntry, HeadEntry, ProfileData, ProfileEntry, SkinEntry, UuidData,
    UuidEntry, XenosCache,
};
use crate::error::XenosError;
use crate::error::XenosError::{InvalidTextures, NotRetrieved};
use crate::mojang::{MojangApi, Profile};
use image::{imageops, ColorType, GenericImageView, ImageOutputFormat};
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;
use XenosError::NotFound;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();
}

lazy_static! {
    pub static ref PROFILE_REQ_AGE_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_age_seconds",
        "The grpc profile response age in seconds.",
        &["request_type", "status"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
    pub static ref PROFILE_REQ_LAT_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_latency_seconds",
        "The grpc profile request latency in seconds.",
        &["request_type", "status"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

pub fn track_service_call<D>(
    result: &Result<CacheEntry<D>, XenosError>,
    start: Instant,
    request_type: &str,
) where
    D: Debug + Clone + PartialEq + Eq,
{
    let status = match result {
        Ok(entry) => {
            PROFILE_REQ_AGE_HISTOGRAM
                .with_label_values(&[request_type, "ok"])
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
}

impl From<Profile> for ProfileData {
    fn from(value: Profile) -> Self {
        ProfileData {
            uuid: value.id,
            name: value.name,
            properties: value
                .properties
                .into_iter()
                .map(|prop| cache::ProfileProperty {
                    name: prop.name,
                    value: prop.value,
                    signature: prop.signature,
                })
                .collect(),
            profile_actions: value.profile_actions,
        }
    }
}

pub struct Service {
    pub cache: Box<Mutex<dyn XenosCache>>,
    pub mojang: Box<dyn MojangApi>,
}

impl Service {
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
            let cached = self
                .cache
                .lock()
                .await
                .get_uuid_by_username(username)
                .await?;
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
                self.cache
                    .lock()
                    .await
                    .set_uuid_by_username(&username, entry)
                    .await?;
            }
        }

        Ok(uuids)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        let start = Instant::now();
        let result = self._get_profile(uuid).await;
        track_service_call(&result, start, "profile");
        result
    }

    async fn _get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        // try to get from cache
        let cached = self.cache.lock().await.get_profile_by_uuid(uuid).await?;
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
                    .lock()
                    .await
                    .set_profile_by_uuid(*uuid, ProfileEntry::new_empty())
                    .await?;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };

        let entry = ProfileEntry::new(profile.into());
        self.cache
            .lock()
            .await
            .set_profile_by_uuid(*uuid, entry.clone())
            .await?;
        Ok(entry)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        let start = Instant::now();
        let result = self._get_skin(uuid).await;
        track_service_call(&result, start, "skin");
        result
    }

    async fn _get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        // try to get from cache
        let cached = self.cache.lock().await.get_skin_by_uuid(uuid).await?;
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
                    .lock()
                    .await
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
                    .lock()
                    .await
                    .set_skin_by_uuid(*uuid, SkinEntry::new_empty())
                    .await?;
                return Err(NotFound);
            }
            Err(err) => return Err(err),
        };
        let entry = SkinEntry::new(skin.to_vec());
        self.cache
            .lock()
            .await
            .set_skin_by_uuid(*uuid, entry.clone())
            .await?;
        Ok(entry)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
        let start = Instant::now();
        let result = self._get_head(uuid, overlay).await;
        track_service_call(&result, start, "head");
        result
    }

    async fn _get_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
        let cached = self
            .cache
            .lock()
            .await
            .get_head_by_uuid(uuid, overlay)
            .await?;
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
                    .lock()
                    .await
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
            .lock()
            .await
            .set_head_by_uuid(*uuid, entry.clone(), overlay)
            .await?;
        Ok(entry)
    }
}
