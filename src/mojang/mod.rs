pub mod api;
#[cfg(feature = "static-testing")]
pub mod testing;

use async_trait::async_trait;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use bytes::Bytes;
use image::{imageops, ColorType, GenericImageView, ImageError, ImageFormat};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use uuid::Uuid;

/// The model key for the classic skin (e.g. "Steve")
pub const CLASSIC_MODEL: &str = "classic";

/// The model key for the slim skin (e.g. "Alex")
pub const SLIM_MODEL: &str = "slim";

/// The official mojang Steve skin (not approved by mojang).
/// See https://assets.mojang.com/SkinTemplates/steve.png
pub const STEVE_SKIN: Bytes =
    Bytes::from_static(include_bytes!("../../resources/profiles/steve_skin.png"));

/// The official mojang Alex skin (not approved by mojang).
/// See https://assets.mojang.com/SkinTemplates/alex.png
pub const ALEX_SKIN: Bytes =
    Bytes::from_static(include_bytes!("../../resources/profiles/alex_skin.png"));

lazy_static! {
    /// The head bytes of the official mojang Steve skin.
    pub static ref STEVE_HEAD: Bytes = Bytes::from(
        build_skin_head(&STEVE_SKIN, false).expect("expect Steve head to be build successfully"),
    );

    /// The head bytes of the official mojang Alex skin.
    pub static ref ALEX_HEAD: Bytes = Bytes::from(
        build_skin_head(&ALEX_SKIN, false).expect("expect Alex head to be build successfully"),
    );
}

/// [ApiError] is the error definition for the Mojang api. The inconsistent error responses from
/// Mojang are mapped to these.
pub enum ApiError {
    /// The api is currently unavailable (outage/timeout/rate limited) or is out-of-date.
    Unavailable,

    /// The requested resource was not found.
    NotFound,
}

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
    pub id: Uuid,
    /// The current visual name of the Minecraft user profile.
    pub name: String,
    /// The currently assigned properties of the Minecraft user profile.
    #[serde(default)]
    pub properties: Vec<ProfileProperty>,
    /// The pending imposed moderative actions of the Minecraft user profile.
    #[serde(default)]
    pub profile_actions: Vec<String>,
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
    pub name: String,
    /// The base64 encoded value of the profile property.
    pub value: String,
    /// The base64 encoded signature of the profile property.
    /// Only provided if `?unsigned=false` is appended to url
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TexturesProperty {
    pub timestamp: u64,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub signature_required: Option<bool>,
    pub textures: Textures,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub struct Textures {
    pub skin: Option<Texture>,
    pub cape: Option<Texture>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    pub url: String,
    pub metadata: Option<TextureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextureMetadata {
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsernameResolved {
    pub id: Uuid,
    pub name: String,
}

impl Profile {
    /// Gets the [texture property](TexturesProperty) of the [profile](Profile). It is expected, that
    /// the property exists on the [profile](Profile) and is valid.
    pub fn get_textures(&self) -> TexturesProperty {
        let prop = self
            .properties
            .iter()
            .find(|prop| prop.name == *"textures")
            .expect("expected textures exist on profile");
        decode_texture_prop(prop.value.clone())
    }
}

/// Decodes a base64 encoded [texture property](TexturesProperty).
pub fn decode_texture_prop(b64: String) -> TexturesProperty {
    let json = BASE64_STANDARD
        .decode(b64)
        .expect("expected textures to be base64 decodable");
    serde_json::from_slice::<TexturesProperty>(&json)
        .expect("expected textures to be json decodable")
}

/// Encodes [texture property](TexturesProperty) to base64.
pub fn encode_texture_prop(prop: &TexturesProperty) -> String {
    let vec = serde_json::to_vec(prop).expect("expected textures to be encodable");
    BASE64_STANDARD.encode(vec)
}

/// Calculates the java hashcode of a [Uuid].
/// See https://hg.openjdk.org/jdk8/jdk8/jdk/file/687fd7c7986d/src/share/classes/java/util/UUID.java#l394
pub fn uuid_java_hashcode(uuid: &Uuid) -> i32 {
    let (most_sig_bits, least_sig_bits) = uuid.as_u64_pair();
    let hilo = most_sig_bits ^ least_sig_bits;
    ((hilo >> 32) ^ hilo) as i32
}

/// Checks if the default skin for a user is "Steve". Otherwise, it is "Alex".
/// See https://wiki.vg/Mojang_API#UUID_to_Profile_and_Skin.2FCape
pub fn is_steve(uuid: &Uuid) -> bool {
    uuid_java_hashcode(uuid) % 2 == 0
}

/// Builds the head image bytes from a skin. Expects a valid skin.
#[tracing::instrument(skip(skin_bytes))]
pub fn build_skin_head(skin_bytes: &[u8], overlay: bool) -> Result<Vec<u8>, ImageError> {
    let skin_img = image::load_from_memory_with_format(skin_bytes, ImageFormat::Png)?;
    let mut head_img = skin_img.view(8, 8, 8, 8).to_image();

    if overlay {
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
        ImageFormat::Png,
    )?;
    Ok(head_bytes)
}

#[async_trait]
pub trait Mojang: Send + Sync {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, ApiError>;
    async fn fetch_profile(&self, uuid: &Uuid, signed: bool) -> Result<Profile, ApiError>;
    async fn fetch_image_bytes(&self, url: String, resource_tag: &str) -> Result<Bytes, ApiError>;
}
