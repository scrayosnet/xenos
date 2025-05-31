use crate::metrics::{MOJANG_REQ, MOJANG_REQ_LAT, MojangLatLabels, MojangReqLabels};
use crate::mojang::ApiError::{NotFound, Unavailable};
use crate::mojang::{ApiError, Mojang, Profile, TextureBytes, UsernameResolved};
use metrics::MetricsEvent;
use reqwest::StatusCode;
use std::error::Error;
use std::sync::LazyLock;
use tracing::{error, warn};
use uuid::Uuid;

/// The shared http client with connection pool, uses arc internally
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .build()
        .expect("failed to build http client")
});

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
    MOJANG_REQ_LAT
        .get_or_create(&MojangLatLabels {
            request_type,
            status,
        })
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

        MOJANG_REQ
            .get_or_create(&MojangReqLabels {
                request_type: "uuids_chunk",
                status: response.status().to_string(),
            })
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

        MOJANG_REQ
            .get_or_create(&MojangReqLabels {
                request_type: "uuid",
                status: response.status().to_string(),
            })
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

        MOJANG_REQ
            .get_or_create(&MojangReqLabels {
                request_type: "profile",
                status: response.status().to_string(),
            })
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

        MOJANG_REQ
            .get_or_create(&MojangReqLabels {
                request_type: "bytes",
                status: response.status().to_string(),
            })
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
