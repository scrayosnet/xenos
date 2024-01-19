//! Xenos is a serverless workers-rs service, that provides cached and highly optimized access to
//! the Mojang API.
//!
//! It is intended as a replacement for the official API and offers additional endpoints for
//! frequently used operations on the data. That way the already processed data can be cached in its
//! final representation and the processing does not need to happen again and again.
//!
//! The responses are all cached for a short duration so that the data does not become stale but
//! the overall performance is improved and we do not run into any API limits. The individual
//! caching durations are specified at the handler methods.
//!
//! All calls that would usually expect the UUIDs to be either in simple format or in its usual
//! hyphenated form accept both formats interchangeable. This was done in order to minimize
//! conversions that would usually need to happen within the clients.
//!
//! This crate is not intended to be used as a library but is only used to create the WASM that will
//! be executed within Cloudflare's edge infrastructure.
//!
//! See the [official API](https://wiki.vg/Mojang_API) for reference.

mod api;
mod retry;
mod cache;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{OnceLock};
use image::{ColorType, GenericImageView, imageops, ImageOutputFormat};
use reqwest::StatusCode;
use uuid::{Uuid};
use worker::*;
use crate::api::{MojangApi, Profile, UsernameResolved};
use crate::cache::*;

#[derive(thiserror::Error, Debug)]
pub enum XenosError {
    // binding errors
    #[error("invalid uuid: {0}")]
    InvalidUuid(String),
    // mojang related errors
    #[error("mojang: too many requests")]
    MojangTooManyRequests(),
    #[error("mojang: profile not found")]
    MojangNotFound(),
    #[error("mojang: request failed")]
    MojangError(#[from] reqwest::Error),
    #[error("invalid profile textures: {0}")]
    MojangInvalidProfileTextures(String),
    // cache errors
    #[error("cache retrieve error")]
    CacheRetrieve(#[from] worker::Error),
    #[error("cache error")]
    Cache(#[from] worker::kv::KvError),
}

impl XenosError {
    pub fn into_response(self) -> worker::Result<worker::Response> {
        match self {
            XenosError::MojangTooManyRequests() => Response::error(
                "too many requests",
                StatusCode::TOO_MANY_REQUESTS.as_u16(),
            ),
            XenosError::MojangNotFound() => Response::error(
                "resource not found",
                StatusCode::NOT_FOUND.as_u16(),
            ),
            XenosError::MojangError(inner) => Response::error(
                inner.to_string(),
                StatusCode::NOT_FOUND.as_u16(),
            ),
            XenosError::InvalidUuid(str) => Response::error(
                format!("invalid uuid: {}", str.to_string()),
                StatusCode::BAD_REQUEST.as_u16(),
            ),
            XenosError::MojangInvalidProfileTextures(str) => Response::error(
                format!("invalid profile textures: {}", str.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ),
            XenosError::CacheRetrieve(err) => Response::error(
                err.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ),
            XenosError::Cache(_) => Response::error(
                "cache error",
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ),
        }
    }
}

// static mojang api
fn mojang_api() -> &'static MojangApi {
    static API: OnceLock<MojangApi> = OnceLock::new();
    API.get_or_init(|| MojangApi::default())
}

/// Distributes the incoming requests from Cloudflare.
///
/// The requests are assigned to the individual endpoints and all requests, that cannot be matched
/// for any of the specified routes, are rejected. All sub handlers are async and handle their
/// responses with their supplied contexts.
#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // distribute the requests to the individual routes
    Router::new()
        .post_async("/uuids", handle_uuids)
        .get_async("/profile/:uuid", handle_profile)
        .get_async("/skin/:uuid", handle_skin)
        .get_async("/head/:uuid", handle_head)
        .run(req, env).await
}

/// Retrieves a deserialized unique identifier from a supplied route context.
///
/// Expects the UUID to be present within the URL parameter `uuid` and it needs to either be in
/// simplified or hyphenated form. The identifier is then parsed into the corresponding struct, so
/// that it can be formatted in the desired form for each API request.
///
/// # Errors
///
/// - pending (no value present, cannot be parsed)
pub fn get_uuid(ctx: &RouteContext<()>) -> std::result::Result<Uuid, XenosError> {
    let str = ctx.param("uuid")
        .ok_or(XenosError::InvalidUuid("missing".to_string()))?;
    Uuid::try_parse(str)
        .map_err(|_err| XenosError::InvalidUuid(str.to_string()))
}

pub async fn get_usernames(usernames: &Vec<String>, ctx: &RouteContext<()>) -> std::result::Result<Vec<UsernameResolved>, XenosError> {
    let mut result = vec![];
    let mut non_cached = vec![];
    // try to get from cache
    for username in usernames {
        let cached = ctx.get_user_id(username).await?;
        if let Some(res) = cached {
            result.push(res)
        } else {
            non_cached.push(username.to_lowercase())
        }
    }
    console_log!("Got {} user_ids from cache", result.len());
    if non_cached.is_empty() {
        return Ok(result)
    }

    // otherwise get missing from mongo and add to cache
    let resolved = mojang_api().get_usernames(&non_cached).await?;
    let mut resolved_map: BTreeMap<_, _> = resolved.into_iter()
        .map(|data| (data.name.to_lowercase(), data))
        .collect();
    for username in non_cached {
        let res = resolved_map
            .remove(&username)
            .unwrap_or_else(|| UsernameResolved {
                id: Uuid::nil(),
                name: username, // stores lower case name
            });
        ctx.put_user_id(res.clone()).await?;
        result.push(res);
    }
    Ok(result)
}

pub async fn get_profile(user_id: &Uuid, ctx: &RouteContext<()>) -> std::result::Result<Profile, XenosError> {
    // try to get from cache
    let cached = ctx.get_profile(user_id).await?;
    if let Some(profile) = cached {
        return Ok(profile)
    }
    // otherwise get from mongo and add to cache
    let profile = mojang_api()
        .get_profile(user_id).await?;
    ctx.put_profile(profile.clone()).await?;
    Ok(profile)
}

pub async fn get_skin(user_id: &Uuid, ctx: &RouteContext<()>) -> std::result::Result<Vec<u8>, XenosError> {
    // try to get from cache
    let cached = ctx.get_skin(user_id).await?;
    if let Some(bytes) = cached {
        return Ok(bytes)
    }
    // otherwise get from mongo and add to cache
    let skin_url = get_profile(user_id, ctx).await?
        .get_textures()?
        .get_skin_url()
        .ok_or(XenosError::MojangInvalidProfileTextures("missing skin".to_string()))?;
    let bytes = mojang_api()
        .get_image_bytes(skin_url).await?
        .to_vec();
    ctx.put_skin(user_id, bytes.clone()).await?;
    Ok(bytes)
}

pub async fn get_head(user_id: &Uuid, ctx: &RouteContext<()>) -> std::result::Result<Vec<u8>, XenosError> {
    // try to get from cache
    let cached = ctx.get_head(user_id).await?;
    if let Some(bytes) = cached {
        return Ok(bytes)
    }

    // otherwise get from mongo and add to cache
    let skin_bytes = get_skin(user_id, ctx).await?;

    let skin_img = image::load_from_memory_with_format(&skin_bytes, image::ImageFormat::Png)
        .map_err(|_err| XenosError::MojangInvalidProfileTextures("failed to read image bytes".to_string()))?;

    let mut head_img = skin_img
        .view(8, 8, 8, 8)
        .to_image();
    let overlay_head_img = skin_img
        .view(40, 8, 8, 8)
        .to_image();
    imageops::overlay(&mut head_img, &overlay_head_img, 0, 0);

    let mut head_bytes: Vec<u8> = Vec::new();
    let mut cur = Cursor::new(&mut head_bytes);
    image::write_buffer_with_format(&mut cur, &head_img, 8, 8, ColorType::Rgba8, ImageOutputFormat::Png)
        .map_err(|_err| XenosError::MojangInvalidProfileTextures("failed to write image bytes".to_string()))?;
    ctx.put_head(user_id, head_bytes.clone()).await?;
    Ok(head_bytes)
}

pub async fn handle_uuids(mut req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let usernames: Vec<String> = match req.json().await {
        Ok(usernames) => usernames,
        Err(_) => {
            return Response::error("", StatusCode::BAD_REQUEST.as_u16())
        }
    };
    match get_usernames(&usernames, &ctx).await {
        Ok(resolved) => Response::from_json(&resolved),
        Err(err) => err.into_response()
    }
}

pub async fn handle_profile(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id = match get_uuid(&ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            return err.into_response()
        }
    };
    match get_profile(&user_id, &ctx).await {
        Ok(profile) => Response::from_json(&profile),
        Err(err) => err.into_response(),
    }
}

pub async fn handle_skin(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id = match get_uuid(&ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            return err.into_response()
        }
    };
    match get_skin(&user_id, &ctx).await {
        Ok(skin_bytes) => Response::from_bytes(skin_bytes),
        Err(err) => err.into_response(),
    }
}

pub async fn handle_head(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id = match get_uuid(&ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            return err.into_response()
        }
    };
    match get_head(&user_id, &ctx).await {
        Ok(head_bytes) => Response::from_bytes(head_bytes),
        Err(err) => err.into_response(),
    }
}
