use crate::error::XenosError;
use crate::error::XenosError::{NotFound, NotRetrieved};
use crate::mojang::{Mojang, Profile, UsernameResolved};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use reqwest::StatusCode;
use std::future::Future;
use std::time::Instant;
use uuid::Uuid;

// TODO update buckets
lazy_static! {
    /// The shared http client with connection pool, uses arc internally
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();

    /// A histogram for the mojang request status and request latencies in seconds. Use the
    /// [monitor_reqwest] utility for ease of use.
    static ref MOJANG_REQ_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_mojang_request_duration_seconds",
        "The mojang request latencies in seconds.",
        &["request_type", "status"],
        vec![0.020, 0.030, 0.040, 0.050, 0.060, 0.070, 0.080, 0.090, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

/// Monitors the inner [reqwest] request (to the mojang api).
async fn monitor_reqwest<F, Fut>(
    request_type: &str,
    f: F,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let start = Instant::now();
    let response = f().await;
    let status = match &response {
        Ok(response) => response.status().to_string(),
        Err(_) => "error".to_string(),
    };
    MOJANG_REQ_HISTOGRAM
        .with_label_values(&[request_type, &status])
        .observe(start.elapsed().as_secs_f64());
    response
}

/// [MojangApi] is stateless a wrapper for the official mojang api.
pub struct MojangApi;

impl Default for MojangApi {
    fn default() -> Self {
        Self::new()
    }
}

impl MojangApi {
    /// Creates a new [MojangApi].
    pub fn new() -> Self {
        Self {}
    }

    /// Implements [Mojang::fetch_uuids] but with the constraint that the usernames slice may not be
    /// larger than the mojang api allows (currently this constraint is ten).
    #[tracing::instrument(skip(self))]
    async fn fetch_uuids_chunk(
        &self,
        usernames: &[String],
    ) -> Result<Vec<UsernameResolved>, XenosError> {
        let response = monitor_reqwest("uuids", || {
            HTTP_CLIENT
                .post("https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname")
                .json(usernames)
                .send()
        })
        .await?;
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
}

#[async_trait]
impl Mojang for MojangApi {
    #[tracing::instrument(skip(self))]
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError> {
        // split into requests with ten or fewer usernames
        let mut resolved = vec![];
        let chunks = usernames.chunks(10);
        for chunk in chunks {
            resolved.extend(self.fetch_uuids_chunk(chunk).await?)
        }
        Ok(resolved)
    }

    #[tracing::instrument(skip(self))]
    async fn fetch_profile(&self, uuid: &Uuid) -> Result<Profile, XenosError> {
        let response = monitor_reqwest("profile", || {
            HTTP_CLIENT
                .get(format!(
                    "https://sessionserver.mojang.com/session/minecraft/profile/{}",
                    uuid.simple()
                ))
                .send()
        })
        .await?;
        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::TOO_MANY_REQUESTS => Err(NotRetrieved),
            _ => {
                let profile = response.error_for_status()?.json().await?;
                Ok(profile)
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn fetch_image_bytes(
        &self,
        url: String,
        resource_tag: &str,
    ) -> Result<Bytes, XenosError> {
        let response = monitor_reqwest(&format!("texture_{resource_tag}"), || {
            HTTP_CLIENT.get(url).send()
        })
        .await?;
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
