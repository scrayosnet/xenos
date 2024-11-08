use crate::mojang::ApiError::{NotFound, Unavailable};
use crate::mojang::{ApiError, Mojang, Profile, TextureBytes, UsernameResolved};
use lazy_static::lazy_static;
use metrics::MetricsEvent;
use prometheus::{register_counter_vec, register_histogram_vec, CounterVec, HistogramVec};
use reqwest::StatusCode;
use std::error::Error;
use tracing::{error, warn};
use uuid::Uuid;

lazy_static! {
    /// The shared http client with connection pool, uses arc internally
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();

    /// A histogram for the mojang request status and request latencies in seconds. Use the
    /// [monitor_reqwest] utility for ease of use.
    static ref MOJANG_REQ_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_mojang_request_duration_seconds",
        "The mojang request latencies in seconds.",
        &["request_type", "status"],
        vec![0.05, 0.1, 0.175, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0]
    )
    .unwrap();

    /// A counter for the mojang request status.
    static ref MOJANG_REQ_COUNTER: CounterVec = register_counter_vec!(
        "xenos_mojang_request_status_total",
        "The mojang request status.",
        &["request_type", "status"]
    )
    .unwrap();
}

fn metrics_handler<T>(event: MetricsEvent<Result<T, ApiError>>) {
    let status = match event.result {
        Ok(_) => "ok",
        Err(Unavailable) => "unavailable",
        Err(NotFound) => "not_found",
    };
    let Some(request_type) = event.labels.get("request_type") else {
        warn!("Failed to retrieve label 'request_type' for metric!");
        return;
    };
    MOJANG_REQ_HISTOGRAM
        .with_label_values(&[request_type, status])
        .observe(event.time);
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
    #[metrics::metrics(
        metric = "mojang_api",
        labels(request_type = "uuids_chunk"),
        handler = metrics_handler,
    )]
    async fn fetch_uuids_chunk(
        &self,
        usernames: &[String],
    ) -> Result<Vec<UsernameResolved>, ApiError> {
        let response = HTTP_CLIENT
            .post("https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname")
            .json(usernames)
            .send()
            .await
            .map_err(|err| {
                warn!(error = %err, cause = err.source(), "failed to fetch uuids");
                Unavailable
            })?;

        MOJANG_REQ_COUNTER
            .with_label_values(&["uuids_chunk", response.status().as_str()])
            .inc();

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Ok(vec![]),
            StatusCode::OK => response.json().await.map_err(|err| {
                error!(error = %err, "failed to parse uuids body");
                Unavailable
            }),
            code => {
                let body = response.text().await.unwrap_or(String::new());
                warn!(
                    status = code.as_str(),
                    body = body,
                    "failed to read uuids: invalid status code"
                );
                Err(Unavailable)
            }
        }
    }
}

impl Mojang for MojangApi {
    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "mojang_api",
        labels(request_type = "uuid"),
        handler = metrics_handler,
    )]
    async fn fetch_uuid(&self, username: &str) -> Result<UsernameResolved, ApiError> {
        let response = HTTP_CLIENT
            .get(format!(
                "https://api.mojang.com/users/profiles/minecraft/{}",
                username
            ))
            .send()
            .await
            .map_err(|err| {
                warn!(error = %err, cause = err.source(), "failed to fetch uuid");
                Unavailable
            })?;

        MOJANG_REQ_COUNTER
            .with_label_values(&["uuid", response.status().as_str()])
            .inc();

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::OK => response.json().await.map_err(|err| {
                error!(error = %err, "failed to parse uuid body");
                Unavailable
            }),
            code => {
                let body = response.text().await.unwrap_or(String::new());
                warn!(
                    status = code.as_str(),
                    body = body,
                    "failed to read uuid: invalid status code"
                );
                Err(Unavailable)
            }
        }
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "mojang_api",
        labels(request_type = "uuids"),
        handler = metrics_handler,
    )]
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
    #[metrics::metrics(
        metric = "mojang_api",
        labels(request_type = "profile"),
        handler = metrics_handler,
    )]
    async fn fetch_profile(&self, uuid: &Uuid, signed: bool) -> Result<Profile, ApiError> {
        let response = HTTP_CLIENT
            .get(format!(
                "https://sessionserver.mojang.com/session/minecraft/profile/{}?unsigned={}",
                uuid.simple(),
                !signed,
            ))
            .send()
            .await
            .map_err(|err| {
                warn!(error = %err, cause = err.source(), "failed to fetch profile");
                Unavailable
            })?;

        MOJANG_REQ_COUNTER
            .with_label_values(&["profile", response.status().as_str()])
            .inc();

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::OK => response.json().await.map_err(|err| {
                error!(error = %err, "failed to parse profile body");
                Unavailable
            }),
            code => {
                let body = response.text().await.unwrap_or(String::new());
                warn!(
                    status = code.as_str(),
                    body = body,
                    "failed to read profile: invalid status code"
                );
                Err(Unavailable)
            }
        }
    }

    #[tracing::instrument(skip(self))]
    #[metrics::metrics(
        metric = "mojang_api",
        labels(request_type = "bytes"),
        handler = metrics_handler,
    )]
    async fn fetch_bytes(&self, url: String) -> Result<TextureBytes, ApiError> {
        let response = HTTP_CLIENT.get(url).send().await.map_err(|err| {
            warn!(error = %err, cause = err.source(), "failed to fetch bytes");
            Unavailable
        })?;

        MOJANG_REQ_COUNTER
            .with_label_values(&["bytes", response.status().as_str()])
            .inc();

        match response.status() {
            StatusCode::NOT_FOUND | StatusCode::NO_CONTENT => Err(NotFound),
            StatusCode::OK => response.bytes().await.map(TextureBytes).map_err(|err| {
                error!(error = %err, "failed to parse body bytes");
                Unavailable
            }),
            code => {
                let body = response.text().await.unwrap_or(String::new());
                warn!(
                    status = code.as_str(),
                    body = body,
                    "failed to read bytes: invalid status code"
                );
                Err(Unavailable)
            }
        }
    }
}
