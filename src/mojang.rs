use crate::error::XenosError;
use crate::error::XenosError::NoContent;
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

lazy_static! {
    // shared http client with connection pool, uses arc internally
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();
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

#[async_trait]
pub trait MojangApi: Send + Sync {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError>;
    async fn fetch_profile(&self, uuid: &Uuid) -> Result<Profile, XenosError>;
    async fn fetch_image_bytes(&self, url: String) -> Result<Bytes, XenosError>;
}

trait ErrorForNoContent {
    fn error_for_no_content(self) -> Result<reqwest::Response, XenosError>;
}

impl ErrorForNoContent for reqwest::Response {
    fn error_for_no_content(self) -> Result<reqwest::Response, XenosError> {
        match self.status() {
            StatusCode::NO_CONTENT => Err(NoContent),
            _ => Ok(self),
        }
    }
}

pub struct Mojang;

#[async_trait]
impl MojangApi for Mojang {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError> {
        Ok(HTTP_CLIENT
            .post("https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname")
            .json(usernames)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn fetch_profile(&self, uuid: &Uuid) -> Result<Profile, XenosError> {
        let url = format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{}",
            uuid.simple()
        );
        Ok(HTTP_CLIENT
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .error_for_no_content()?
            .json()
            .await?)
    }

    async fn fetch_image_bytes(&self, url: String) -> Result<Bytes, XenosError> {
        Ok(HTTP_CLIENT
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .error_for_no_content()?
            .bytes()
            .await?)
    }
}
