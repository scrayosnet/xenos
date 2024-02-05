pub mod pb {
    use crate::cache::{HeadEntry, ProfileEntry, SkinEntry, UuidEntry};

    tonic::include_proto!("scrayosnet.xenos");

    impl From<ProfileEntry> for ProfileResponse {
        fn from(value: ProfileEntry) -> Self {
            ProfileResponse {
                timestamp: value.timestamp,
                uuid: value.uuid.hyphenated().to_string(),
                name: value.name,
                properties: value
                    .properties
                    .into_iter()
                    .map(|prop| ProfileProperty {
                        name: prop.name,
                        value: prop.value,
                        signature: prop.signature,
                    })
                    .collect(),
                profile_actions: value.profile_actions,
            }
        }
    }

    impl From<SkinEntry> for SkinResponse {
        fn from(value: SkinEntry) -> Self {
            SkinResponse {
                timestamp: value.timestamp,
                data: value.bytes,
            }
        }
    }

    impl From<HeadEntry> for HeadResponse {
        fn from(value: HeadEntry) -> Self {
            HeadResponse {
                timestamp: value.timestamp,
                data: value.bytes,
            }
        }
    }

    impl From<UuidEntry> for UuidResult {
        fn from(value: UuidEntry) -> Self {
            UuidResult {
                timestamp: value.timestamp,
                username: value.username,
                uuid: value.uuid.hyphenated().to_string(),
            }
        }
    }

    impl From<Vec<UuidEntry>> for UuidResponse {
        fn from(value: Vec<UuidEntry>) -> Self {
            UuidResponse {
                resolved: value.into_iter().map(|v| v.into()).collect(),
            }
        }
    }
}

use crate::cache;
use crate::cache::Cached::*;
use crate::cache::{get_epoch_seconds, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use crate::error::XenosError::{InvalidTextures, NotFound, NotRetrieved};
use crate::mojang::{MojangApi, UsernameResolved};
use crate::service::pb::{
    HeadRequest, HeadResponse, ProfileRequest, ProfileResponse, SkinRequest, SkinResponse,
};
use image::{imageops, ColorType, GenericImageView, ImageOutputFormat};
use lazy_static::lazy_static;
use pb::profile_server::Profile;
use pb::{UuidRequest, UuidResponse};
use prometheus::{register_histogram_vec, HistogramVec};
use regex::Regex;
use std::collections::HashMap;
use std::io::Cursor;
use std::time::Instant;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use uuid::Uuid;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();
}

lazy_static! {
    pub static ref PROFILE_REQ_AGE_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_profile_requests_total",
        "The grpc profile response age in seconds.",
        &["request_type"],
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

type GrpcResult<T> = Result<Response<T>, Status>;

pub struct XenosService {
    pub cache: Box<Mutex<dyn XenosCache>>,
    pub mojang: Box<dyn MojangApi>,
}

impl XenosService {
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

    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UuidEntry>, XenosError> {
        // 1. initialize with uuid not found
        let mut uuids: HashMap<String, UuidEntry> =
            HashMap::from_iter(usernames.iter().map(|username| {
                (
                    username.to_lowercase(),
                    UuidEntry {
                        timestamp: 0,
                        username: username.to_lowercase(),
                        uuid: Uuid::nil(),
                    },
                )
            }));

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
                let res = found.get(&username).cloned().unwrap_or(UsernameResolved {
                    name: username.to_lowercase(),
                    id: Uuid::nil(),
                });
                let key = res.name.to_lowercase();
                let entry = UuidEntry {
                    timestamp: get_epoch_seconds(),
                    username: res.name,
                    uuid: res.id,
                };
                uuids.insert(key.clone(), entry.clone());
                self.cache
                    .lock()
                    .await
                    .set_uuid_by_username(&key, entry)
                    .await?;
            }
        }

        Ok(uuids.into_values().collect())
    }

    async fn fetch_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
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

    async fn fetch_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        let cached = self.cache.lock().await.get_skin_by_uuid(uuid).await?;
        let fallback = match cached {
            Hit(entry) => return Ok(entry),
            Expired(entry) => Some(entry),
            Miss => None,
        };

        let profile = match self.fetch_profile(uuid).await {
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

    async fn fetch_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
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

        let skin = match self.fetch_skin(uuid).await {
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

#[tonic::async_trait]
impl Profile for XenosService {
    async fn get_uuids(&self, request: Request<UuidRequest>) -> GrpcResult<UuidResponse> {
        // parse request
        let usernames = request.into_inner().usernames;
        // resolve uuids
        let uuids = match self.fetch_uuids(&usernames).await {
            Ok(uuids) => uuids,
            Err(err) => return Err(Status::internal(err.to_string())),
        };
        // build response
        Ok(Response::new(uuids.into()))
    }

    async fn get_profile(
        &self,
        request: Request<ProfileRequest>,
    ) -> Result<Response<ProfileResponse>, Status> {
        let start = Instant::now();
        // parse input
        let uuid = match Uuid::try_parse(&request.into_inner().uuid) {
            Ok(uuid) => uuid,
            Err(_) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["profile", "bad_request"])
                    .observe(start.elapsed().as_secs() as f64);
                return Err(Status::invalid_argument("invalid uuid"));
            }
        };
        // get profile and build response
        match self.fetch_profile(&uuid).await {
            Ok(profile) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["profile", "ok"])
                    .observe(start.elapsed().as_secs() as f64);
                PROFILE_REQ_AGE_HISTOGRAM
                    .with_label_values(&["profile"])
                    .observe(profile.timestamp as f64);
                Ok(Response::new(profile.into()))
            }
            Err(NotFound) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["profile", "not_found"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::not_found("profile not found"))
            }
            Err(NotRetrieved) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["profile", "not_retrieved"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::unavailable("unable to retrieve"))
            }
            Err(err) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["profile", "error"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::internal(err.to_string()))
            }
        }
    }

    async fn get_skin(
        &self,
        request: Request<SkinRequest>,
    ) -> Result<Response<SkinResponse>, Status> {
        let start = Instant::now();
        // parse input
        let uuid = match Uuid::try_parse(&request.into_inner().uuid) {
            Ok(uuid) => uuid,
            Err(_) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["skin", "bad_request"])
                    .observe(start.elapsed().as_secs() as f64);
                return Err(Status::invalid_argument("invalid uuid"));
            }
        };
        // get skin and build response
        match self.fetch_skin(&uuid).await {
            Ok(skin) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["skin", "ok"])
                    .observe(start.elapsed().as_secs() as f64);
                PROFILE_REQ_AGE_HISTOGRAM
                    .with_label_values(&["skin"])
                    .observe(skin.timestamp as f64);
                Ok(Response::new(skin.into()))
            }
            Err(NotFound) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["skin", "not_found"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::not_found("skin not found"))
            }
            Err(NotRetrieved) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["skin", "not_retrieved"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::unavailable("unable to retrieve"))
            }
            Err(err) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["skin", "error"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::internal(err.to_string()))
            }
        }
    }

    async fn get_head(
        &self,
        request: Request<HeadRequest>,
    ) -> Result<Response<HeadResponse>, Status> {
        let start = Instant::now();
        // parse input
        let req = request.into_inner();
        let overlay = &req.overlay;
        let uuid = match Uuid::try_parse(&req.uuid) {
            Ok(uuid) => uuid,
            Err(_) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["head", "bad_request"])
                    .observe(start.elapsed().as_secs() as f64);
                return Err(Status::invalid_argument("invalid uuid"));
            }
        };
        // get head and build response
        match self.fetch_head(&uuid, overlay).await {
            Ok(head) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["head", "ok"])
                    .observe(start.elapsed().as_secs() as f64);
                PROFILE_REQ_AGE_HISTOGRAM
                    .with_label_values(&["head"])
                    .observe(head.timestamp as f64);
                Ok(Response::new(head.into()))
            }
            Err(NotFound) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["head", "not_found"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::not_found("head not found"))
            }
            Err(NotRetrieved) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["head", "not_retrieved"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::unavailable("unable to retrieve"))
            }
            Err(err) => {
                PROFILE_REQ_LAT_HISTOGRAM
                    .with_label_values(&["head", "error"])
                    .observe(start.elapsed().as_secs() as f64);
                Err(Status::internal(err.to_string()))
            }
        }
    }
}
