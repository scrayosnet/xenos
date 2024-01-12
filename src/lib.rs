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

use std::collections::BTreeMap;
use std::io::Write;
use base64::prelude::*;
use image::{ColorType, EncodableLayout, GenericImageView, ImageEncoder};
use image::codecs::png::{PngEncoder};
use uuid::{Uuid};
use serde::{Deserialize, Serialize};
use worker::*;


/// Represents a single Minecraft user profile with all current properties.
///
/// Each Minecraft account is associated with exactly one profile that reflects the visual and
/// technical state that the player is in. Some fields can be influenced by the player while other
/// fields are strictly set by the system.
///
/// The `properties` usually only include one property called `textures`, but this may change over
/// time, so it is kept as an array as that is what's specified in the JSON. The `profile_actions`
/// are empty for non-sanctioned accounts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// The unique identifier of the Minecraft user profile.
    id: Uuid,
    /// The current visual name of the Minecraft user profile.
    name: String,
    /// The currently assigned properties of the Minecraft user profile.
    #[serde(default)]
    properties: Vec<ProfileProperty>,
    /// The pending imposed moderative actions of the Minecraft user profile.
    #[serde(default)]
    profile_actions: Vec<String>,
}

/// Represents a single property of a Minecraft user profile.
///
/// A property defines one specific aspect of a user profile. The most prominent property is called
/// `textures` and contains information on the skin and visual appearance of the user. Each property
/// name is unique for an individual user.
///
/// All properties are cryptographic signed to verify the authenticity of the provided data. The
/// `signature` of the property is signed with Yggdrasil's private key and therefore its
/// authenticity can be verified by the Minecraft client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileProperty {
    /// The unique, identifiable name of the profile property.
    name: String,
    /// The base64 encoded value of the profile property.
    value: String,
    /// The base64 encoded signature of the profile property.
    /// Only provided if `?unsigned=false` is appended to url
    signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TexturesProperty {
    timestamp: u64,
    profile_id: Uuid,
    profile_name: String,
    signature_required: Option<bool>,
    textures: BTreeMap<String, Texture>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    url: String,
    metadata: Option<TextureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextureMetadata {
    model: String,
}

/// Retrieves the Minecraft profile for a specific unique identifier.
///
/// Tries to retrieve the Minecraft profile from the official API and serialize the response,
/// wrapping any errors that are triggered by the attempt. The profile is guaranteed to be complete,
/// if any profile is returned by this call.
///
/// # Errors
///
/// - pending (profile does not exist, api rate limit, other http error)
pub async fn get_profile(user_id: Uuid) -> Result<Profile> {
    let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", user_id.simple());
    reqwest::get(url).await
        .map_err(|err| Error::RustError(format!("{:?}", err)))?
        .json().await
        .map_err(|err| Error::RustError(format!("{:?}", err)))
}

pub async fn get_image(url: String) -> Result<bytes::Bytes> {
    reqwest::get(url).await
        .map_err(|err| Error::RustError(format!("{:?}", err)))?
        .bytes().await
        .map_err(|err| Error::RustError(format!("{:?}", err)))
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
pub fn get_uuid(ctx: RouteContext<()>) -> Result<Uuid> {
    let str = ctx.param("uuid")
        .ok_or(Error::BindingError("missing uuid".to_string()))?;
    Uuid::try_parse(str)
        .map_err(|err| Error::BindingError(format!("{:?}", err)))
}

pub fn parse_texture_prop(b64: String) -> Result<TexturesProperty> {
    let json = BASE64_STANDARD.decode(b64)
        .map_err(|err| Error::RustError(format!("{:?}", err)))?;
    serde_json::from_slice::<TexturesProperty>(&json)
        .map_err(|err| Error::RustError(format!("{:?}", err)))
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

pub async fn handle_uuids(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    // TODO implement me
    Response::empty()
}

pub async fn handle_profile(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // retrieve the unique id from the request params
    let user_id: Uuid = get_uuid(ctx)?;

    // retrieve the profile if not cached
    let profile: Profile = get_profile(user_id).await?;

    Response::from_json(&profile)
}

pub async fn handle_skin(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // retrieve the unique id from the request params
    let user_id: Uuid = get_uuid(ctx)?;

    // retrieve the profile if not cached
    let profile: Profile = get_profile(user_id).await?;

    // parse profile texture property
    let prop = profile.properties
        .iter()
        .find(|prop| prop.name == "textures".to_string())
        .ok_or(Error::RustError("No textures provided".to_string()))?;
    let textures = parse_texture_prop(prop.value.clone())?;

    // TODO handle no custom skin
    let skin_url = textures.textures.get("SKIN")
        .ok_or(Error::RustError("No skin provided".to_string()))?
        .url.clone();
    let skin_bytes = get_image(skin_url).await?;

    Response::from_bytes(skin_bytes.to_vec())
        // append filename header
        .map(|mut res| {
            res.headers_mut().append(
                "Content-Disposition",
                &*format!("attachment; filename=\"skin_{}.png\"", user_id.simple())
            ).unwrap();
            res
        })
}

pub async fn handle_head(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // retrieve the unique id from the request params
    let user_id: Uuid = get_uuid(ctx)?;

    // retrieve the profile if not cached
    let profile: Profile = get_profile(user_id).await?;

    // parse profile texture property
    let prop = profile.properties
        .iter()
        .find(|prop| prop.name == "textures".to_string())
        .ok_or(Error::RustError("No textures provided".to_string()))?;
    let textures = parse_texture_prop(prop.value.clone())?;

    // TODO handle no custom skin
    let skin_url = textures.textures.get("SKIN")
        .ok_or(Error::RustError("No skin provided".to_string()))?
        .url.clone();
    let skin_bytes = get_image(skin_url).await?;

    let skin_img = image::load_from_memory(&skin_bytes)
        .map_err(|err| Error::RustError(format!("{:?}", err)))?;
    let head_img = skin_img
        .view(8, 8, 8, 8)
        .to_image();

    let mut head_bytes: Vec<u8> = Vec::new();
    PngEncoder::new(head_bytes.by_ref())
        .write_image(head_img.as_bytes(), 8, 8, ColorType::Rgba8)
        .map_err(|err| Error::RustError(format!("{:?}", err)))?;

    Response::from_bytes(head_bytes)
        // append filename header
        .map(|mut res| {
            res.headers_mut().append(
                "Content-Disposition",
                &*format!("attachment; filename=\"head_{}.png\"", user_id.simple())
            ).unwrap();
            res
        })
}
