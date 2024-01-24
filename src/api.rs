use std::collections::{BTreeMap};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use bytes::Bytes;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use worker::{console_debug, console_log};
use crate::XenosError;
use crate::XenosError::*;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsernameResolved {
    pub id: Uuid,
    pub name: String,
}

trait ErrorForNoContent {
    fn error_for_no_content(self) -> Result<reqwest::Response, XenosError>;
}

impl ErrorForNoContent for reqwest::Response {
    fn error_for_no_content(self) -> Result<reqwest::Response, XenosError> {
        match self.status() {
            StatusCode::NO_CONTENT => Err(MojangNotFound()),
            _ => Ok(self)
        }
    }
}

pub struct MojangApi {
    client: reqwest::Client,
    max_tries: u8,
}

impl Default for MojangApi {
    fn default() -> Self {
        MojangApi {
            client: reqwest::Client::builder()
                .build()
                .unwrap(),
            max_tries: 10,
        }
    }
}

impl MojangApi {

    pub async fn get_usernames(&self, usernames: &Vec<String>) -> Result<Vec<UsernameResolved>, XenosError> {
        self.client
            .post("https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname")
            .json(usernames)
            .send_retry(self.max_tries).await
            .map_err(|err| MojangError(err))?
            .error_for_status()
            .map_err(|err| MojangError(err))?
            .error_for_no_content()?
            .json().await
            .map_err(|err| MojangError(err))
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
    pub async fn get_profile(&self, user_id: &Uuid) -> Result<Profile, XenosError> {
        let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", user_id.simple());
        let response = self.client
            .get(url)
            .send_retry(self.max_tries).await;
        console_debug!("mojang response {:?}", response);
        let resp = response
            .map_err(|err| MojangError(err))?;
        console_debug!("mojang response body {:?}", resp.text().await);
        Err(InvalidUuid("debugging going on".to_string()))
        //resp
        //    .error_for_status()
        //    .map_err(|err| MojangError(err))?
        //    .error_for_no_content()?
        //    .json().await
        //    .map_err(|err| MojangError(err))
    }

    pub async fn get_image_bytes(&self, url: String) -> Result<Bytes, XenosError> {
        self.client
            .get(url.clone())
            .send_retry(self.max_tries).await
            .map_err(|err| MojangError(err))?
            .error_for_status()
            .map_err(|err| MojangError(err))?
            .error_for_no_content()?
            .bytes().await
            .map_err(|err| MojangError(err))
    }
}

impl Profile {
    pub fn get_textures(&self) -> Result<TexturesProperty, XenosError> {
        let prop = self.properties
            .iter()
            .find(|prop| prop.name == "textures".to_string())
            .ok_or(XenosError::MojangInvalidProfileTextures("missing".to_string()))?;
        Profile::parse_texture_prop(prop.value.clone())
    }

    fn parse_texture_prop(b64: String) -> Result<TexturesProperty, XenosError> {
        let json = BASE64_STANDARD.decode(b64)
            .map_err(|_err| XenosError::MojangInvalidProfileTextures("base64 decode failed".to_string()))?;
        serde_json::from_slice::<TexturesProperty>(&json)
            .map_err(|_err| XenosError::MojangInvalidProfileTextures("json decode failed".to_string()))
    }
}

impl TexturesProperty {
    pub fn get_skin_url(&self) -> Option<String> {
        self.textures
            .get("SKIN")
            .map(|texture| texture.url.clone())
    }
}
