use crate::mojang::ApiError::{NotFound, Unavailable};
use crate::mojang::{ApiError, Mojang, Profile, UsernameResolved};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use reqwest::StatusCode;
use std::future::Future;
use std::time::Instant;
use tracing::{error, warn};
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
    ) -> Result<Vec<UsernameResolved>, ApiError> {
        let response = monitor_reqwest("uuids", || {
            HTTP_CLIENT
                .post("https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname")
                .json(usernames)
                .send()
        })
        .await
        .map_err(|err| {
            warn!("failed to fetch uuids: {}", err);
            Unavailable
        })?;

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Ok(vec![]),
            StatusCode::OK => response.json().await.map_err(|err| {
                error!("failed to read body (username_resolved): {}", err);
                Unavailable
            }),
            code => {
                warn!(status = code.as_str(), "{:?}", response.text().await.ok());
                Err(Unavailable)
            }
        }
    }
}

#[async_trait]
impl Mojang for MojangApi {
    #[tracing::instrument(skip(self))]
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, ApiError> {
        // split into requests with ten or fewer usernames
        let mut resolved = vec![];
        let chunks = usernames.chunks(10);
        for chunk in chunks {
            resolved.extend(self.fetch_uuids_chunk(chunk).await?)
        }
        Ok(resolved)
    }

    #[tracing::instrument(skip(self))]
    async fn fetch_profile(&self, uuid: &Uuid, signed: bool) -> Result<Profile, ApiError> {
        let response = monitor_reqwest("profile", || {
            HTTP_CLIENT
                .get(format!(
                    "https://sessionserver.mojang.com/session/minecraft/profile/{}?unsigned={}",
                    uuid.simple(),
                    !signed,
                ))
                .send()
        })
        .await
        .map_err(|err| {
            warn!("failed to fetch profile: {}", err);
            Unavailable
        })?;

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::OK => response.json().await.map_err(|err| {
                error!("failed to read body (profile): {}", err);
                Unavailable
            }),
            code => {
                warn!(status = code.as_str(), "{:?}", response.text().await.ok());
                Err(Unavailable)
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn fetch_image_bytes(&self, url: String, resource_tag: &str) -> Result<Bytes, ApiError> {
        let response = monitor_reqwest(resource_tag, || HTTP_CLIENT.get(url).send())
            .await
            .map_err(|err| {
                warn!("failed to fetch {} bytes: {}", resource_tag, err);
                Unavailable
            })?;

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::OK => response.bytes().await.map_err(|err| {
                error!("failed to read body (bytes): {}", err);
                Unavailable
            }),
            code => {
                warn!(status = code.as_str(), "{:?}", response.text().await.ok());
                Err(Unavailable)
            }
        }
    }
}
