use std::collections::{BTreeMap, HashMap};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use bytes::Bytes;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::ApiError;
use crate::ApiError::{MojangError, MojangNotFound};
use crate::retry::{Retry};


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
    pub id: Option<Uuid>,
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
    pub textures: BTreeMap<String, Texture>,
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

trait ErrorForNoContent {
    fn error_for_no_content(self) -> Result<reqwest::Response, ApiError>;
}

impl ErrorForNoContent for reqwest::Response {
    fn error_for_no_content(self) -> Result<reqwest::Response, ApiError> {
        match self.status() {
            StatusCode::NO_CONTENT => Err(MojangNotFound()),
            _ => Ok(self)
        }
    }
}

pub struct MojangApi {
    client: reqwest::Client,
    max_tries: u8,
    cache_profiles: HashMap<Uuid, (u64, Profile)>,
    cache_images: HashMap<String, (u64, Bytes)>,
}

impl Default for MojangApi {
    fn default() -> Self {
        MojangApi {
            client: reqwest::Client::builder()
                .build()
                .unwrap(),
            max_tries: 10,
            cache_profiles: HashMap::new(),
            cache_images: HashMap::new(),
        }
    }
}

impl MojangApi {
    /// Retrieves the Minecraft profile for a specific unique identifier.
    ///
    /// Tries to retrieve the Minecraft profile from the official API and serialize the response,
    /// wrapping any errors that are triggered by the attempt. The profile is guaranteed to be complete,
    /// if any profile is returned by this call.
    ///
    /// # Errors
    ///
    /// - pending (profile does not exist, api rate limit, other http error)
    pub async fn get_profile(&mut self, user_id: &Uuid) -> Result<Profile, ApiError> {
        // try get from cache
        let now = worker::Date::now().as_millis();
        if let Some((time, profile)) = self.cache_profiles.get(user_id) {
            if now - time < 5000 {
                return Ok(profile.clone())
            }
        }
        // retrieve new
        let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", user_id.simple());
        let profile: Profile = self.client
            .get(url)
            .send_retry(self.max_tries).await
            .map_err(|err| MojangError(err))?
            .error_for_status()
            .map_err(|err| MojangError(err))?
            .error_for_no_content()?
            .json().await
            .map_err(|err| MojangError(err))?;
        self.cache_profiles.insert(user_id.clone(), (now, profile.clone()));
        Ok(profile)
    }

    pub async fn get_image_bytes(&mut self, url: String) -> Result<Bytes, ApiError> {
        // try get from cache
        let now = worker::Date::now().as_millis();
        if let Some((time, bytes)) = self.cache_images.get(&url) {
            if now - time < 5000 {
                return Ok(bytes.clone())
            }
        }
        // retrieve new
        let bytes = self.client
            .get(url.clone())
            .send_retry(self.max_tries).await
            .map_err(|err| MojangError(err))?
            .error_for_status()
            .map_err(|err| MojangError(err))?
            .error_for_no_content()?
            .bytes().await
            .map_err(|err| MojangError(err))?;
        self.cache_images.insert(url, (now, bytes.clone()));
        Ok(bytes)
    }
}

impl Profile {
    pub fn get_textures(&self) -> Result<TexturesProperty, ApiError> {
        let prop = self.properties
            .iter()
            .find(|prop| prop.name == "textures".to_string())
            .ok_or(ApiError::InvalidProfileTextures("missing".to_string()))?;
        Profile::parse_texture_prop(prop.value.clone())
    }

    fn parse_texture_prop(b64: String) -> Result<TexturesProperty, ApiError> {
        let json = BASE64_STANDARD.decode(b64)
            .map_err(|_err| ApiError::InvalidProfileTextures("base64 decode failed".to_string()))?;
        serde_json::from_slice::<TexturesProperty>(&json)
            .map_err(|_err| ApiError::InvalidProfileTextures("json decode failed".to_string()))
    }
}

impl TexturesProperty {
    pub fn get_skin_url(&self) -> Option<String> {
        self.textures
            .get("SKIN")
            .map(|texture| texture.url.clone())
    }
}
