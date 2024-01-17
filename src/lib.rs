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
use worker::Error::{Json};
use worker::*;
use crate::api::{MojangApi, Profile};


trait IntoWorkerError {
    fn into_worker_err(self) -> worker::Error;
}

impl IntoWorkerError for reqwest::Error {
    fn into_worker_err(self) -> worker::Error {
        match self.status() {
            Some(StatusCode::NOT_FOUND) => {
                Json(("".to_string(), StatusCode::NOT_FOUND.as_u16()))
            }
            Some(StatusCode::TOO_MANY_REQUESTS) => {
                Json(("".to_string(), StatusCode::TOO_MANY_REQUESTS.as_u16()))
            }
            _ => {
                // self.to_string()
                Json(("{\"msg\": \"test\"}".to_string(), StatusCode::IM_A_TEAPOT.as_u16()))
            }
        }
    }
}

impl IntoWorkerError for uuid::Error {
    fn into_worker_err(self) -> worker::Error {
        worker::Error::BindingError(format!("{:?}", self))
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
pub fn get_uuid(ctx: RouteContext<()>) -> worker::Result<Uuid> {
    let str = ctx.param("uuid")
        .ok_or(worker::Error::BindingError("missing uuid".to_string()))?;
    Uuid::try_parse(str)
        .map_err(|err| err.into_worker_err())
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

pub async fn get_profile(user_id: &Uuid) -> worker::Result<Profile> {
    mojang_api()
        .write().unwrap()
        .get_profile(user_id).await
        .map_err(|err| err.into_worker_err())
}

pub async fn get_skin(user_id: &Uuid) -> worker::Result<Vec<u8>> {
    let skin_url = get_profile(user_id).await?
        .get_textures()
        .ok_or(Json(("".to_string(), StatusCode::NOT_FOUND.as_u16())))?
        .get_skin_url()
        .ok_or(Json(("".to_string(), StatusCode::NOT_FOUND.as_u16())))?;

    mojang_api()
        .write().unwrap()
        .get_image_bytes(skin_url).await
        .map(|bytes| bytes.to_vec())
        .map_err(|err| err.into_worker_err())
}

pub async fn get_head(user_id: &Uuid) -> worker::Result<Vec<u8>> {
    let skin_bytes = get_skin(user_id).await?;

    let skin_img = image::load_from_memory_with_format(&skin_bytes, image::ImageFormat::Png)
        .map_err(|err| Json((err.to_string(), StatusCode::INTERNAL_SERVER_ERROR.as_u16())))?;

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
        .map_err(|err| Json((err.to_string(), StatusCode::INTERNAL_SERVER_ERROR.as_u16())))?;
    Ok(head_bytes)
}

pub async fn handle_uuids(_req: Request, _ctx: RouteContext<()>) -> worker::Result<Response> {
    // TODO implement me
    Response::empty()
}

pub async fn handle_profile(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id: Uuid = get_uuid(ctx)?;
    if let Ok(profile) = get_profile(&user_id).await {
        return Response::from_json(&profile)
    }
    Response::from_bytes(Vec::from("test"))
}

pub async fn handle_skin(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id: Uuid = get_uuid(ctx)?;
    let skin_bytes = get_skin(&user_id).await?;
    Response::from_bytes(skin_bytes)
}

pub async fn handle_head(_req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    let user_id: Uuid = get_uuid(ctx)?;
    let head_bytes = get_head(&user_id).await?;
    Response::from_bytes(head_bytes)
}
