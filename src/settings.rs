use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct RedisCache {
    pub address: String,
    pub ttl: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum CacheVariant {
    Redis,
    Memory,
    None,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cache {
    pub variant: CacheVariant,
    pub redis: RedisCache,
    pub expiry_uuid: u64,
    pub expiry_uuid_missing: u64,
    pub expiry_profile: u64,
    pub expiry_profile_missing: u64,
    pub expiry_skin: u64,
    pub expiry_skin_missing: u64,
    pub expiry_head: u64,
    pub expiry_head_missing: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpServer {
    pub rest_gateway: bool,
    pub address: SocketAddr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metrics {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrpcServer {
    pub enabled: bool,
    pub address: SocketAddr,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Settings {
    pub debug: bool,
    pub cache: Cache,
    pub metrics: Metrics,
    pub http_server: HttpServer,
    pub grpc_server: GrpcServer,
}

impl Display for CacheVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Settings {
    /// Loads and creates a new instance of the application settings.
    /// The settings are composed of the `config/default`, the `config/local`, and the environment variables.
    pub fn new() -> Result<Self, ConfigError> {
        let env_prefix = env::var("ENV_PREFIX").unwrap_or_else(|_| "xenos".into());
        info!(env_prefix = env_prefix, "Loading settings");

        let s = Config::builder()
            // Start off by merging in the "default" configuration file
            .add_source(File::with_name("config/default"))
            // Add in a local configuration file
            // This file shouldn't be checked in to git
            .add_source(File::with_name("config/local").required(false))
            // Add in settings from the environment (with a prefix of APP)
            // E.g. `XENOS__DEBUG=1` would set the `debug` key, on the other hand,
            // `XENOS__CACHE__VARIANT=redis` would enable the redis cache.
            .add_source(Environment::with_prefix(&env_prefix).separator("__"))
            .build()?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}
