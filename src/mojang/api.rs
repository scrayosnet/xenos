use crate::error::XenosError;
use crate::error::XenosError::{NotFound, NotRetrieved};
use crate::mojang::{MojangApi, Profile, UsernameResolved};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec};
use reqwest::StatusCode;
use uuid::Uuid;

lazy_static! {
    // shared http client with connection pool, uses arc internally
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();
}

lazy_static! {
    static ref MOJANG_REQ_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xenos_mojang_requests_total",
        "Total number of requests to mojang.",
        &["request_type", "status"],
    )
    .unwrap();
    static ref MOJANG_REQ_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "xenos_mojang_request_duration_seconds",
        "The mojang request latencies in seconds.",
        &["request_type"],
        vec![0.020, 0.030, 0.040, 0.050, 0.060, 0.070, 0.080, 0.090, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

pub struct Mojang;

impl Mojang {
    #[tracing::instrument(skip(self))]
    async fn fetch_uuids_chunk(
        &self,
        usernames: &[String],
    ) -> Result<Vec<UsernameResolved>, XenosError> {
        let _timer = MOJANG_REQ_HISTOGRAM
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
}

#[async_trait]
impl MojangApi for Mojang {
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
        let url = format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{}",
            uuid.simple()
        );
        let _timer = MOJANG_REQ_HISTOGRAM
            .with_label_values(&["profile"])
            .start_timer();
        // make request
        let response = HTTP_CLIENT.get(url).send().await?;
        // update metrics
        MOJANG_REQ_TOTAL
            .with_label_values(&["profile", response.status().as_str()])
            .inc();
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

    #[tracing::instrument(skip(self))]
    async fn fetch_image_bytes(
        &self,
        url: String,
        resource_tag: &str,
    ) -> Result<Bytes, XenosError> {
        let _timer = MOJANG_REQ_HISTOGRAM
            .with_label_values(&[&format!("texture_{resource_tag}")])
            .start_timer();
        // make request
        let response = HTTP_CLIENT.get(url).send().await?;
        // update metrics
        MOJANG_REQ_TOTAL
            .with_label_values(&[
                &format!("texture_{resource_tag}"),
                response.status().as_str(),
            ])
            .inc();
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
