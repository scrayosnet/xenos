use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::net::SocketAddr;

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct RedisCache {
    pub enabled: bool,
    pub cache_time: u64,
    pub expiration: Option<usize>,
    pub address: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct MemoryCache {
    pub enabled: bool,
    pub cache_time: u64,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct HttpServer {
    pub rest_gateway: bool,
    pub address: SocketAddr,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Metrics {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct GrpcServer {
    pub enabled: bool,
    pub address: SocketAddr,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Settings {
    pub debug: bool,
    pub memory_cache: MemoryCache,
    pub redis_cache: RedisCache,
    pub metrics: Metrics,
    pub http_server: HttpServer,
    pub grpc_server: GrpcServer,
}

impl Settings {
    /// Loads and creates a new instance of the application settings.
    /// The settings are composed of the `config/default`, the `config/local`, and the environment variables.
    pub fn new() -> Result<Self, ConfigError> {
        let env_prefix = env::var("ENV_PREFIX").unwrap_or_else(|_| "xenos".into());

        let s = Config::builder()
            // Start off by merging in the "default" configuration file
            .add_source(File::with_name("config/default"))
            // Add in a local configuration file
            // This file shouldn't be checked in to git
            .add_source(File::with_name("config/local").required(false))
            // Add in settings from the environment (with a prefix of APP)
            // E.g. `APP__DEBUG=1` would set the `debug` key, on the other hand,
            // `XENOS__REDIS_CACHE__ENABLED=0` would disable the redis cache.
            .add_source(Environment::with_prefix(&env_prefix).separator("__"))
            .build()?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}
