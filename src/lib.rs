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

use std::io::Cursor;
use std::sync::{OnceLock, RwLock};
use image::{ColorType, GenericImageView, imageops, ImageOutputFormat};
use reqwest::StatusCode;
use uuid::{Uuid};
use worker::*;
use crate::api::{MojangApi, Profile};
use crate::ApiError::{InvalidProfileTextures, InvalidUuid};

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("mojang: too many requests")]
    MojangTooManyRequests(),
    #[error("mojang: profile not found")]
    MojangNotFound(),
    #[error("mojang: request failed")]
    MojangError(#[from] reqwest::Error),
    #[error("invalid profile textures: {0}")]
    InvalidProfileTextures(String),
    #[error("invalid uuid: {0}")]
    InvalidUuid(String),
}

impl ApiError {
    pub fn into_response(self) -> worker::Result<worker::Response> {
        match self {
            ApiError::MojangTooManyRequests() => Response::error(
                "too many requests",
                StatusCode::TOO_MANY_REQUESTS.as_u16(),
            ),
            ApiError::MojangNotFound() => Response::error(
                "resource not found",
                StatusCode::NOT_FOUND.as_u16(),
            ),
            ApiError::MojangError(inner) => Response::error(
                inner.to_string(),
                StatusCode::NOT_FOUND.as_u16(),
            ),
            ApiError::InvalidUuid(str) => Response::error(
                format!("invalid uuid: {}", str.to_string()),
                StatusCode::BAD_REQUEST.as_u16(),
            ),
            ApiError::InvalidProfileTextures(str) => Response::error(
                format!("invalid profile textures: {}", str.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ),
        }
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
pub fn get_uuid(ctx: RouteContext<()>) -> std::result::Result<Uuid, ApiError> {
    let str = ctx.param("uuid")
        .ok_or(InvalidUuid("missing".to_string()))?;
    Uuid::try_parse(str)
        .map_err(|_err| InvalidUuid(str.to_string()))
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

// static mojang api TODO is this a good idea?
fn mojang_api() -> &'static RwLock<MojangApi> {
    static API: OnceLock<RwLock<MojangApi>> = OnceLock::new();
    API.get_or_init(|| RwLock::new(MojangApi::default()))
}

pub async fn get_profile(user_id: &Uuid) -> std::result::Result<Profile, ApiError> {
    mojang_api()
        .write().unwrap()
        .get_profile(user_id).await
}

pub async fn get_skin(user_id: &Uuid) -> std::result::Result<Vec<u8>, ApiError> {
    let skin_url = get_profile(user_id).await?
        .get_textures()?
        .get_skin_url()
        .ok_or(InvalidProfileTextures("missing skin".to_string()))?;
    let bytes = mojang_api()
        .write().unwrap()
        .get_image_bytes(skin_url).await?;
    Ok(bytes.to_vec())
}

pub async fn get_head(user_id: &Uuid) -> std::result::Result<Vec<u8>, ApiError> {
    let skin_bytes = get_skin(user_id).await?;

    let skin_img = image::load_from_memory_with_format(&skin_bytes, image::ImageFormat::Png)
        .map_err(|_err| InvalidProfileTextures("failed to read image bytes".to_string()))?;

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
        .map_err(|_err| InvalidProfileTextures("failed to write image bytes".to_string()))?;
    Ok(head_bytes)
}

pub async fn handle_uuids(_req: Request, _ctx: RouteContext<()>) -> worker::Result<Response> {
    // TODO implement me!
    Response::empty()
}

pub async fn handle_profile(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id = match get_uuid(ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            return err.into_response()
        }
    };
    match get_profile(&user_id).await {
        Ok(profile) => Response::from_json(&profile),
        Err(err) => err.into_response(),
    }
}

pub async fn handle_skin(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id = match get_uuid(ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            return err.into_response()
        }
    };
    match get_skin(&user_id).await {
        Ok(skin_bytes) => Response::from_bytes(skin_bytes),
        Err(err) => err.into_response(),
    }
}

pub async fn handle_head(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id = match get_uuid(ctx) {
        Ok(uuid) => uuid,
        Err(err) => {
            return err.into_response()
        }
    };
    match get_head(&user_id).await {
        Ok(head_bytes) => Response::from_bytes(head_bytes),
        Err(err) => err.into_response(),
    }
}
