pub mod pb {
    tonic::include_proto!("scrayosnet.xenos");
}

use crate::cache;
use crate::cache::{HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use crate::error::XenosError::{InvalidTextures, NoContent, NotFound, NotRetrieved, Reqwest};
use crate::mojang::{MojangApi, UsernameResolved};
use crate::service::pb::{
    HeadRequest, HeadResponse, ProfileProperty, ProfileRequest, ProfileResponse, SkinRequest,
    SkinResponse,
};
use crate::util::{get_epoch_seconds, has_elapsed};
use image::{imageops, ColorType, GenericImageView, ImageOutputFormat};
use lazy_static::lazy_static;
use pb::profile_server::Profile;
use pb::{UuidRequest, UuidResult, UuidResponse};
use regex::Regex;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::io::Cursor;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use uuid::Uuid;

const CACHE_TIME: u64 = 5 * 60;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();
}

type GrpcResult<T> = Result<Response<T>, Status>;

pub struct XenosService {
    pub cache: Box<Mutex<dyn XenosCache>>,
    pub mojang: Box<dyn MojangApi>,
}

impl XenosService {
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
            if let Some(cached) = self
                .cache
                .lock()
                .await
                .get_uuid_by_username(username)
                .await?
            {
                let elapsed = has_elapsed(&cached.timestamp, &CACHE_TIME);
                *uuid = cached;
                if !elapsed {
                    continue;
                }
            }
            cache_misses.push(username.clone())
        }

        // 4. all others get from mojang in one request
        if !cache_misses.is_empty() {
            let response = match self.mojang.fetch_uuids(&cache_misses).await {
                Ok(r) => r,
                Err(Reqwest(err)) => {
                    return match err.status() {
                        Some(StatusCode::TOO_MANY_REQUESTS) => Ok(uuids.into_values().collect()),
                        _ => Err(Reqwest(err)),
                    };
                }
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
                uuids.insert(key, entry.clone());
                self.cache.lock().await.set_uuid_by_username(entry).await?;
            }
        }

        Ok(uuids.into_values().collect())
    }

    async fn fetch_profile(&self, uuid: &Uuid) -> Result<ProfileEntry, XenosError> {
        // return cached if not elapsed
        let cached = match self.cache.lock().await.get_profile_by_uuid(uuid).await? {
            None => None,
            Some(entry) => {
                if !has_elapsed(&entry.timestamp, &CACHE_TIME) {
                    return Ok(entry);
                }
                Some(entry)
            }
        };

        // try to fetch
        let profile = match self.mojang.fetch_profile(uuid).await {
            Ok(r) => r,
            Err(Reqwest(err)) => {
                return match err.status() {
                    Some(StatusCode::TOO_MANY_REQUESTS) => cached.ok_or(NotRetrieved),
                    Some(StatusCode::NOT_FOUND) => Err(NotFound),
                    _ => Err(Reqwest(err)),
                };
            }
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
            .set_profile_by_uuid(entry.clone())
            .await?;
        Ok(entry)
    }

    async fn fetch_skin(&self, uuid: &Uuid) -> Result<SkinEntry, XenosError> {
        let cached = match self.cache.lock().await.get_skin_by_uuid(uuid).await? {
            None => None,
            Some(entry) => {
                if !has_elapsed(&entry.timestamp, &CACHE_TIME) {
                    return Ok(entry);
                }
                Some(entry)
            }
        };

        let profile = match self.fetch_profile(uuid).await {
            Ok(profile) => profile,
            Err(NotRetrieved) => return cached.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };
        let skin_url = profile
            .get_textures()?
            .textures
            .skin
            .ok_or(InvalidTextures("skin missing".to_string()))?
            .url;
        let skin = match self.mojang.fetch_image_bytes(skin_url).await {
            Ok(r) => r,
            Err(Reqwest(err)) => {
                return match err.status() {
                    Some(StatusCode::TOO_MANY_REQUESTS) => cached.ok_or(NotRetrieved),
                    Some(StatusCode::NOT_FOUND) => Err(NotFound),
                    _ => Err(Reqwest(err)),
                };
            }
            Err(err) => return Err(err),
        };
        let entry = SkinEntry {
            timestamp: get_epoch_seconds(),
            uuid: uuid.to_owned(),
            bytes: skin.to_vec(),
        };
        self.cache
            .lock()
            .await
            .set_skin_by_uuid(entry.clone())
            .await?;
        Ok(entry)
    }

    async fn fetch_head(&self, uuid: &Uuid, overlay: &bool) -> Result<HeadEntry, XenosError> {
        let cached = match self.cache.lock().await.get_head_by_uuid(uuid, overlay).await? {
            None => None,
            Some(entry) => {
                if !has_elapsed(&entry.timestamp, &CACHE_TIME) {
                    return Ok(entry);
                }
                Some(entry)
            }
        };

        let skin = match self.fetch_skin(uuid).await {
            Ok(profile) => profile,
            Err(NotRetrieved) => return cached.ok_or(NotRetrieved),
            Err(err) => return Err(err),
        };

        let skin_img = image::load_from_memory_with_format(&skin.bytes, image::ImageFormat::Png)?;
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

        let entry = HeadEntry {
            timestamp: get_epoch_seconds(),
            uuid: uuid.to_owned(),
            bytes: head_bytes,
        };
        self.cache
            .lock()
            .await
            .set_head_by_uuid(entry.clone(), overlay)
            .await?;
        Ok(entry)
    }
}

fn parse_uuid(str: &str) -> Result<Uuid, Status> {
    Uuid::try_parse(str).map_err(|_| Status::invalid_argument("invalid uuid"))
}

#[tonic::async_trait]
impl Profile for XenosService {
    async fn get_uuids(&self, request: Request<UuidRequest>) -> GrpcResult<UuidResponse> {
        let usernames = request.into_inner().usernames;
        let uuids = match self.fetch_uuids(&usernames).await {
            Ok(uuids) => uuids,
            Err(err) => return Err(Status::internal(err.to_string())),
        };

        let resolved = uuids
            .into_iter()
            .map(|entry| UuidResult {
                timestamp: entry.timestamp,
                username: entry.username,
                uuid: entry.uuid.hyphenated().to_string(),
            })
            .collect();
        Ok(Response::new(UuidResponse { resolved }))
    }

    async fn get_profile(
        &self,
        request: Request<ProfileRequest>,
    ) -> Result<Response<ProfileResponse>, Status> {
        let uuid = parse_uuid(&request.into_inner().uuid)?;
        // get profile
        let profile = match self.fetch_profile(&uuid).await {
            Ok(profile) => profile,
            Err(NotFound) | Err(NoContent) => return Err(Status::not_found("profile not found")),
            Err(NotRetrieved) => return Err(Status::unavailable("unable to retrieve")),
            Err(err) => return Err(Status::internal(err.to_string())),
        };
        // build response
        let response = ProfileResponse {
            timestamp: profile.timestamp,
            uuid: profile.uuid.hyphenated().to_string(),
            name: profile.name,
            properties: profile
                .properties
                .into_iter()
                .map(|prop| ProfileProperty {
                    name: prop.name,
                    value: prop.value,
                    signature: prop.signature,
                })
                .collect(),
            profile_actions: profile.profile_actions,
        };
        Ok(Response::new(response))
    }

    async fn get_skin(&self, request: Request<SkinRequest>) -> Result<Response<SkinResponse>, Status> {
        let uuid = parse_uuid(&request.into_inner().uuid)?;
        // get skin
        let skin = match self.fetch_skin(&uuid).await {
            Ok(skin) => skin,
            Err(NotFound) | Err(NoContent) => return Err(Status::not_found("skin not found")),
            Err(NotRetrieved) => return Err(Status::unavailable("unable to retrieve")),
            Err(err) => return Err(Status::internal(err.to_string())),
        };
        // build response
        let response = SkinResponse {
            timestamp: skin.timestamp,
            data: skin.bytes,
        };
        Ok(Response::new(response))
    }

    async fn get_head(&self, request: Request<HeadRequest>) -> Result<Response<HeadResponse>, Status> {
        let req = request.into_inner();
        let uuid = parse_uuid(&req.uuid)?;
        let overlay = &req.overlay;
        // get head
        let head = match self.fetch_head(&uuid, &overlay).await {
            Ok(head) => head,
            Err(NotFound) | Err(NoContent) => return Err(Status::not_found("head not found")),
            Err(NotRetrieved) => return Err(Status::unavailable("unable to retrieve")),
            Err(err) => return Err(Status::internal(err.to_string())),
        };
        // build response
        let response = HeadResponse {
            timestamp: head.timestamp,
            data: head.bytes,
        };
        Ok(Response::new(response))
    }
}
