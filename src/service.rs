use crate::cache;
use crate::cache::Cached::*;
use crate::cache::{
    get_epoch_seconds, CacheEntry, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache,
};
use crate::error::XenosError;
use crate::error::XenosError::{InvalidTextures, NotRetrieved};
use crate::mojang::MojangApi;
use image::{imageops, ColorType, GenericImageView, ImageOutputFormat};
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use regex::Regex;
use std::collections::HashMap;
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
        "xenos_profile_requests_total",
        "The grpc profile response age in seconds.",
        &["request_type", "status"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
    pub static ref PROFILE_REQ_LAT_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_request_duration_seconds",
        "The grpc profile request latency in seconds.",
        &["request_type", "status"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

pub fn track_service_call<T>(result: &Result<T, XenosError>, start: Instant, request_type: &str)
where
    T: CacheEntry,
{
    let status = match result {
        Ok(entry) => {
            PROFILE_REQ_AGE_HISTOGRAM
                .with_label_values(&[request_type, "ok"])
                .observe((get_epoch_seconds() - entry.get_timestamp()) as f64);
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

pub struct Service {
    pub cache: Box<Mutex<dyn XenosCache>>,
    pub mojang: Box<dyn MojangApi>,
}

impl Service {
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

    pub async fn get_uuids(&self, usernames: &[String]) -> Result<Vec<UuidEntry>, XenosError> {
        // 1. initialize with uuid not found
        let mut uuids: HashMap<String, UuidEntry> =
            HashMap::from_iter(usernames.iter().map(|username| {
                (
                    username.to_lowercase(),
                    UuidEntry {
                        timestamp: 0,
                        username: username.to_lowercase(),
                        uuid: Uuid::nil(),
                        not_found: true,
                    },
                )
            }));

        // cache misses contains all usernames (lowercase) that match the regex and are either
        // expired in the cache or not in the cache
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
                Err(NotRetrieved) => return Ok(uuids.into_values().collect()),
                Err(err) => return Err(err),
            };
            let found: HashMap<_, _> = response
                .into_iter()
                .map(|data| (data.name.to_lowercase(), data))
                .collect();
            for username in cache_misses {
                let entry = match found.get(&username) {
                    None => match uuids.get(&username) {
                        // not in cache and not found -> set as not_found
                        None => UuidEntry {
                            timestamp: get_epoch_seconds(),
                            username: username.clone(),
                            uuid: Uuid::nil(),
                            not_found: true,
                        },
                        // expired in cache and not found -> update as not_found
                        Some(entry) => UuidEntry {
                            not_found: true,
                            ..entry.clone()
                        },
                    },
                    // found -> set as found
                    Some(res) => UuidEntry {
                        timestamp: get_epoch_seconds(),
                        username: res.name.clone(),
                        uuid: res.id,
                        not_found: false,
                    },
                };
                uuids.insert(username.clone(), entry.clone());
                self.cache
                    .lock()
                    .await
                    .set_uuid_by_username(&username, entry)
                    .await?;
            }
        }

        Ok(uuids.into_values().collect())
    }

    pub async fn get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        let start = Instant::now();
        let result = self._get_profile(uuid).await;
        track_service_call(&result, start, "profile");
        result
    }

    async fn _get_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        // return cached if not elapsed
        let cached = self.cache.lock().await.get_profile_by_uuid(uuid).await?;
        let fallback = match cached {
            Hit(entry) => return Ok(entry),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        // try to fetch
        let profile = match self.mojang.fetch_profile(uuid).await {
            Ok(r) => r,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };
        let entry = ProfileEntry {
            timestamp: get_epoch_seconds(),
            uuid: *uuid,
            name: profile.name,
            properties: profile
                .properties
                .into_iter()
                .map(|prop| cache::ProfileProperty {
                    name: prop.name,
                    value: prop.value,
                    signature: prop.signature,
                })
                .collect(),
            profile_actions: profile.profile_actions,
        };
        self.cache
            .lock()
            .await
            .set_profile_by_uuid(*uuid, entry.clone())
            .await?;
        Ok(entry)
    }

    pub async fn get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        let start = Instant::now();
        let result = self._get_skin(uuid).await;
        track_service_call(&result, start, "skin");
        result
    }

    async fn _get_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        let cached = self.cache.lock().await.get_skin_by_uuid(uuid).await?;
        let fallback = match cached {
            Hit(entry) => return Ok(entry),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        let profile = match self.get_profile(uuid).await {
            Ok(profile) => profile,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };

        let skin_url = profile
            .get_textures()?
            .textures
            .skin
            .ok_or(InvalidTextures("skin missing".to_string()))?
            .url;
        let skin = match self.mojang.fetch_image_bytes(skin_url, "skin").await {
            Ok(r) => r,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };
        let entry = SkinEntry {
            timestamp: get_epoch_seconds(),
            bytes: skin.to_vec(),
        };
        self.cache
            .lock()
            .await
            .set_skin_by_uuid(*uuid, entry.clone())
            .await?;
        Ok(entry)
    }

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

        let skin = match self.get_skin(uuid).await {
            Ok(profile) => profile,
            Err(NotRetrieved) => return fallback.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };

        let head_bytes = Self::build_skin_head(&skin.bytes)?;

        let entry = HeadEntry {
            timestamp: get_epoch_seconds(),
            bytes: head_bytes,
        };
        self.cache
            .lock()
            .await
            .set_head_by_uuid(*uuid, entry.clone(), overlay)
            .await?;
        Ok(entry)
    }
}
