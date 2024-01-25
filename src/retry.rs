use async_std::task;
use reqwest::{Response, Result};
use std::time::Duration;

pub(crate) trait Retry {
    async fn send_retry(self, retries: u8) -> Result<Response>;
}

impl Retry for reqwest::RequestBuilder {
    async fn send_retry(self, max_tries: u8) -> Result<Response> {
        let mut tries = 0;
        loop {
            tries += 1;
            let response = self.try_clone().unwrap().send().await;
            if response.is_ok() || tries >= max_tries {
                return response;
            }
            // sleep for 100ms (per iteration) before trying again
            task::sleep(Duration::from_millis(tries as u64 * 100)).await;
        }
    }
}
