use crate::error::XenosError;
use crate::error::XenosError::{NotFound, NotRetrieved};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

lazy_static! {
    // shared http client with connection pool, uses arc internally
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();
}

lazy_static! {
    static ref MOJANG_REQ_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mojang_requests_total",
        "Total number of requests to mojang.",
        &["request_type", "status"],
    )
    .unwrap();
    static ref MOJANG_REQ_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "mojang_request_duration_seconds",
        "The mojang request latencies in seconds.",
        &["request_type"],
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
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
    async fn fetch_image_bytes(&self, url: String, resource_tag: &str)
        -> Result<Bytes, XenosError>;
}

pub struct Mojang;

#[async_trait]
impl MojangApi for Mojang {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError> {
        let timer = MOJANG_REQ_HISTOGRAM
            .with_label_values(&["uuids"])
            .start_timer();
        // make request
        let response = HTTP_CLIENT
            .post("https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname")
            .json(usernames)
            .send()
            .await?;
        // update metrics
        MOJANG_REQ_TOTAL
            .with_label_values(&["uuids", response.status().as_str()])
            .inc();
        timer.observe_duration();
        // get response
        match response.status() {
            StatusCode::NOT_FOUND => Err(NotFound),
            StatusCode::NO_CONTENT => Ok(vec![]),
            StatusCode::TOO_MANY_REQUESTS => Err(NotRetrieved),
            _ => {
                let resolved = response.error_for_status()?.json().await?;
                Ok(resolved)
            }
        }
    }

    async fn fetch_profile(&self, uuid: &Uuid) -> Result<Profile, XenosError> {
        let url = format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{}",
            uuid.simple()
        );
        let timer = MOJANG_REQ_HISTOGRAM
            .with_label_values(&["profile"])
            .start_timer();
        // make request
        let response = HTTP_CLIENT.get(url).send().await?;
        // update metrics
        MOJANG_REQ_TOTAL
            .with_label_values(&["profile", response.status().as_str()])
            .inc();
        timer.observe_duration();
        // get response
        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::TOO_MANY_REQUESTS => Err(NotRetrieved),
            _ => {
                let profile = response.error_for_status()?.json().await?;
                Ok(profile)
            }
        }
    }

    async fn fetch_image_bytes(
        &self,
        url: String,
        resource_tag: &str,
    ) -> Result<Bytes, XenosError> {
        let timer = MOJANG_REQ_HISTOGRAM
            .with_label_values(&[&format!("bytes_{resource_tag}")])
            .start_timer();
        // make request
        let response = HTTP_CLIENT.get(url).send().await?;
        // update metrics
        MOJANG_REQ_TOTAL
            .with_label_values(&["bytes", response.status().as_str()])
            .inc();
        timer.observe_duration();
        // get response
        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::TOO_MANY_REQUESTS => Err(NotRetrieved),
            _ => {
                let bytes = response.error_for_status()?.bytes().await?;
                Ok(bytes)
            }
        }
    }
}
