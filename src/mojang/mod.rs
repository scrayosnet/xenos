pub mod api;
#[cfg(feature = "mojang-testing")]
pub mod testing;

use crate::error::XenosError;
use async_trait::async_trait;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub fn get_textures(&self) -> Result<TexturesProperty, XenosError> {
        let prop = self
            .properties
            .iter()
            .find(|prop| prop.name == *"textures")
            .ok_or(XenosError::InvalidTextures("missing".to_string()))?;
        decode_texture_prop(prop.value.clone())
    }
}

/// Decodes a base64 encoded [texture property](TexturesProperty).
pub fn decode_texture_prop(b64: String) -> Result<TexturesProperty, XenosError> {
    let json = BASE64_STANDARD
        .decode(b64)
        .map_err(|_err| XenosError::InvalidTextures("base64 decode failed".to_string()))?;
    serde_json::from_slice::<TexturesProperty>(&json)
        .map_err(|_err| XenosError::InvalidTextures("json decode failed".to_string()))
}

/// Encodes [texture property](TexturesProperty) to base64.
pub fn encode_texture_prop(prop: &TexturesProperty) -> Result<String, XenosError> {
    let vec = serde_json::to_vec(prop)
        .map_err(|_err| XenosError::InvalidTextures("json encode failed".to_string()))?;
    Ok(BASE64_STANDARD.encode(vec))
}

#[async_trait]
pub trait Mojang: Send + Sync {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError>;
    async fn fetch_profile(&self, uuid: &Uuid) -> Result<Profile, XenosError>;
    async fn fetch_image_bytes(&self, url: String, resource_tag: &str)
        -> Result<Bytes, XenosError>;
}
