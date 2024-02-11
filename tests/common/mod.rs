//! The common module provides common utilities and implementation for the integration tests.
//!
//! The biggest contribution is the [Mojang Stub](StubMojang) that allows predictable test results
//! by "hard coding" responses.

use crate::common::mojang_stub::StubMojang;
use std::string::ToString;
use tokio::sync::Mutex;
use uuid::Uuid;
use xenos::cache::memory::MemoryCache;
use xenos::mojang::UsernameResolved;
use xenos::service::Service;

pub mod mojang_stub;

#[derive(Default)]
pub struct ServiceBuilder {
    cache: MemoryCache,
    mojang: StubMojang,
}

#[allow(dead_code)]
impl ServiceBuilder {
    pub fn with_username(mut self, username: &str, uuid: Uuid) -> Self {
        self.mojang.uuids.insert(
            username.to_lowercase(),
            UsernameResolved {
                id: uuid,
                name: username.to_string(),
            },
        );
        self
    }

    pub fn build(self) -> Service {
        Service {
            cache: Box::new(Mutex::new(self.cache)),
            mojang: Box::new(self.mojang),
        }
    }
}
