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
mod cache;
mod error;
mod retry;

use crate::api::{MojangApi, Profile, UsernameResolved};
use crate::cache::*;
use crate::error::*;
use image::{imageops, ColorType, GenericImageView, ImageOutputFormat};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::StatusCode;
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use uuid::Uuid;
use worker::*;

lazy_static! {
    static ref MOJANG_API: MojangApi = MojangApi::default();
    static ref USERNAME_REGEX: Regex = Regex::new("^[a-zA-Z0-9_]{2,16}$").unwrap();
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
        .run(req, env)
        .await
}

pub async fn handle_uuids(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let mut usernames: Vec<String> = match req.json().await {
        Ok(usernames) => usernames,
        Err(err) => return Response::error(err.to_string(), StatusCode::BAD_REQUEST.as_u16()),
    };
    usernames.sort();
    usernames.dedup();
    match get_uuids(&usernames, &ctx).await {
        Ok(resolved) => Response::from_json(&resolved),
        Err(err) => err.into_response(),
    }
}

pub async fn handle_profile(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let user_id = match get_uuid(&ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            console_debug!("user id parse failed: {:?}", err);
            return err.into_response();
        }
    };
    match get_profile(&user_id, &ctx).await {
        Ok(profile) => Response::from_json(&profile),
        Err(err) => {
            console_debug!("get profile failed: {:?}", err);
            err.into_response()
        }
    }
}

pub async fn handle_skin(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let user_id = match get_uuid(&ctx) {
        Ok(uuid) => uuid,
        Err(err) => return err.into_response(),
    };
    match get_skin(&user_id, &ctx).await {
        Ok(skin_bytes) => Response::from_bytes(skin_bytes),
        Err(err) => err.into_response(),
    }
}

pub async fn handle_head(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let user_id = match get_uuid(&ctx) {
        Ok(uuid) => uuid,
        Err(err) => return err.into_response(),
    };
    match get_head(&user_id, &ctx).await {
        Ok(head_bytes) => Response::from_bytes(head_bytes),
        Err(err) => err.into_response(),
    }
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
    let str = ctx
        .param("uuid")
        .ok_or(XenosError::InvalidUuid("missing".to_string()))?;
    Uuid::try_parse(str).map_err(|_err| XenosError::InvalidUuid(str.to_string()))
}

pub async fn get_uuids(
    usernames: &[String],
    ctx: &RouteContext<()>,
) -> std::result::Result<Vec<UsernameResolved>, XenosError> {
    // initialize with id not found
    let mut uuids: HashMap<String, UsernameResolved> =
        HashMap::from_iter(usernames.iter().map(|username| {
            (
                username.to_lowercase(),
                UsernameResolved {
                    name: username.to_lowercase(),
                    id: Uuid::nil(),
                },
            )
        }));

    let mut cache_misses = vec![];
    for (username, uuid) in uuids.iter_mut() {
        // 1. filter invalid (regex)
        if !USERNAME_REGEX.is_match(username.as_str()) {
            continue;
        }
        // 2. get from cache
        if let Some(res) = ctx.get_user_id(username).await? {
            *uuid = res;
        } else {
            cache_misses.push(username.clone())
        }
    }

    // 3. all others get from mojang in one request
    if !cache_misses.is_empty() {
        let found: BTreeMap<_, _> = MOJANG_API
            .get_usernames(&cache_misses)
            .await?
            .into_iter()
            .map(|data| (data.name.to_lowercase(), data))
            .collect();
        for username in cache_misses {
            let res = found.get(&username).cloned().unwrap_or(UsernameResolved {
                name: username.to_lowercase(),
                id: Uuid::nil(),
            });
            uuids.insert(res.name.to_lowercase(), res.clone());
            ctx.put_user_id(res).await?;
        }
    }

    Ok(uuids.into_values().collect())
}

pub async fn get_profile(
    user_id: &Uuid,
    ctx: &RouteContext<()>,
) -> std::result::Result<Profile, XenosError> {
    // try to get from cache
    let cached = ctx.get_profile(user_id).await?;
    if let Some(profile) = cached {
        return Ok(profile);
    }
    // otherwise get from mojang and add to cache
    let profile = MOJANG_API.get_profile(user_id).await?;
    ctx.put_profile(profile.clone()).await?;
    Ok(profile)
}

pub async fn get_skin(
    user_id: &Uuid,
    ctx: &RouteContext<()>,
) -> std::result::Result<Vec<u8>, XenosError> {
    // try to get from cache
    let cached = ctx.get_skin(user_id).await?;
    if let Some(bytes) = cached {
        return Ok(bytes);
    }
    // otherwise get from mojang and add to cache
    let skin_url = get_profile(user_id, ctx)
        .await?
        .get_textures()?
        .get_skin_url()
        .ok_or(XenosError::MojangInvalidProfileTextures(
            "missing skin".to_string(),
        ))?;
    let bytes = MOJANG_API.get_image_bytes(skin_url).await?.to_vec();
    ctx.put_skin(user_id, bytes.clone()).await?;
    Ok(bytes)
}

pub async fn get_head(
    user_id: &Uuid,
    ctx: &RouteContext<()>,
) -> std::result::Result<Vec<u8>, XenosError> {
    // try to get from cache
    let cached = ctx.get_head(user_id).await?;
    if let Some(bytes) = cached {
        return Ok(bytes);
    }

    // otherwise get from mojang and add to cache
    let skin_bytes = get_skin(user_id, ctx).await?;

    let skin_img = image::load_from_memory_with_format(&skin_bytes, image::ImageFormat::Png)
        .map_err(|_err| {
            XenosError::MojangInvalidProfileTextures("failed to read image bytes".to_string())
        })?;

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
    )
    .map_err(|_err| {
        XenosError::MojangInvalidProfileTextures("failed to write image bytes".to_string())
    })?;
    ctx.put_head(user_id, head_bytes.clone()).await?;
    Ok(head_bytes)
}
