//! The settings module defines the application configuration. It is based on [config], a layered
//! configuration system for Rust applications (with strong support for 12-factor applications).
//!
//! # Layers
//!
//! The configuration consists of up to three layers. Upper layers overwrite lower layer configurations
//! (e.g. environment variables overwrite the default configuration).
//!
//! ## Layer 1 (Environment variables) \[optional\]
//!
//! The environment variables are the top most layer. They can be used to overwrite any previous configuration.
//! Environment variables have the format `[ENV_PREFIX]__[field]__[sub_field]` where `ENV_PREFIX` is
//! an environment variable defaulting to `XENOS`. That means, the nested settings field `cache.redis.enabled`
//! can be overwritten by the environment variable `XENOS__CACHE__REDIS__ENABLED`.
//!
//! ## Layer 2 (Custom configuration) \[optional\]
//!
//! The next layer is an optional configuration file intended to be used by deployments and local testing. The file
//! location can be configured using the `CONFIG_CUSTOM_FILE` environment variable, defaulting to `config/custom`.
//! It can be of any file type supported by [config] (e.g. `config/custom.toml`). The file should not be
//! published by git as its configuration is context dependent (e.g. local/cluster) and probably contains
//! secrets.
//!
//! ## Layer 3 (Default configuration)
//!
//! The base layer is a configuration file confining the default configuration. The file
//! location can be configured using the `CONFIG_DEFAULT_FILE` environment variable, defaulting to `config/default`.
//! The file can be any file type supported by [config] (e.g. `config/default.toml`). It provides a
//! default value for all settings fields. It is published by git and SHOULD NEVER contain secrets.
//!
//! # Usage
//!
//! The application configuration can be created by using [Settings::new]. This loads/overrides the
//! configuration fields layer-by-layer.
//!
//! ```rs
//! let settings: Settings = Settings::new()?;
//! ```

use std::env;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use config::{Config, ConfigError, Environment, File};
use serde::de::Unexpected;
use serde::{Deserialize, Deserializer};
use tracing::metadata::LevelFilter;

/// [Cache] hold the service cache configurations. The different caches are accumulated by the
/// [ChainingCache](crate::cache::chaining::ChainingCache). If no cache is `enabled`, caching is
/// effectively disabled.
///
/// In general, there should always be a local cache (e.g. [moka](MokaCache)) enabled and optionally
/// a remote cache (e.g. [redis](RedisCache)).
#[derive(Debug, Clone, Deserialize)]
pub struct Cache {
    pub entries: CacheEntries<CacheEntry>,

    /// The [redis] cache configuration.
    pub redis: RedisCache,

    /// The [moka] cache configuration.
    pub moka: MokaCache,
}

/// [MokaCache] hold the [moka] cache configuration. Moka is a fast in-memory (local) cache. It
/// supports [MokaCacheEntry] `ttl` and `tti` and `cap` per cache entry type.
#[derive(Debug, Clone, Deserialize)]
pub struct MokaCache {
    /// Whether the cache should be used by the [ChainingCache](crate::cache::chaining::ChainingCache).
    pub enabled: bool,

    pub entries: CacheEntries<MokaCacheEntry>,
}

/// [RedisCache] hold the [redis] cache configuration. Redis is a fast remote cache. It supports
/// [RedisCacheEntry] `ttl` per cache entry type but not `tti` and `cap`.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisCache {
    /// Whether the cache should be used by the [ChainingCache](crate::cache::chaining::ChainingCache).
    pub enabled: bool,

    /// The address of the redis instance (e.g. `redis://username:password@example.com/0`). Only used
    /// if redis is enabled.
    pub address: String,

    pub entries: CacheEntries<RedisCacheEntry>,
}

/// [CacheEntries] is a wrapper for configuring [MokaCacheEntry] for all cache entry types.
#[derive(Debug, Clone, Deserialize)]
pub struct CacheEntries<D> {
    /// The cache entry type for username to uuid resolve.
    pub uuid: D,

    /// The cache entry type for uuid to profile resolve.
    pub profile: D,

    /// The cache entry type for uuid to skin resolve.
    pub skin: D,

    /// The cache entry type for uuid to cape resolve.
    pub cape: D,

    /// The cache entry type for uuid to head resolve.
    pub head: D,
}

/// [CacheEntry] holds the general configuration for a single cache entry type.
#[derive(Debug, Clone, Deserialize)]
pub struct CacheEntry {
    /// The cache entry expiration duration. If elapsed, then the cache entry is marked as expired,
    /// but not deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub exp: Duration,

    /// The cache entry expiration duration for empty cache entries (e.g. username not found). If
    /// elapsed, then the cache entry is marked as expired, but not deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub exp_empty: Duration,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MokaCacheEntry {
    /// The cache max capacity. May be supported by cache.
    pub cap: u64,

    /// The cache entry time-to-life. If elapsed, then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl: Duration,

    /// The cache entry time-to-life for empty cache entries (e.g. username not found). If elapsed,
    /// then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl_empty: Duration,

    /// The cache entry time-to-idle. If elapsed, then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub tti: Duration,

    /// The cache entry time-to-idle for empty cache entries (e.g. username not found). If elapsed,
    /// then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub tti_empty: Duration,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisCacheEntry {
    /// The cache entry time-to-life. If elapsed, then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl: Duration,

    /// The cache entry time-to-life for empty cache entries (e.g. username not found). If elapsed,
    /// then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl_empty: Duration,
}

/// [RestServer] holds the rest server configuration. The rest server is implicitly enabled if either
/// the rest gateway of the metrics service is enabled. If enabled, the rest server also exposes the
/// metrics service at `/metrics`.
///
/// The rest gateway exposes the grpc service api over rest.
#[derive(Debug, Clone, Deserialize)]
pub struct RestServer {
    /// Whether the rest gateway should be enabled.
    pub rest_gateway: bool,

    /// The address of the rest server. E.g. `0.0.0.0:9990` for running with an exposed port.
    pub address: SocketAddr,
}

/// [Metrics] holds the metrics service configuration. The metrics service is part of the rest server.
/// The rest server will be, if not already so, implicitly enabled if the metrics service is enabled.
/// If enabled, it is exposed at the rest server at `/metrics`.
///
/// Metrics will always be aggregated by the application. This option is only used to expose the metrics
/// service. The service supports basic auth that can be enabled. Make sure to override the default
/// username and password in that case.
#[derive(Debug, Clone, Deserialize)]
pub struct Metrics {
    /// Whether the metrics service should be enabled.
    pub enabled: bool,

    /// Whether the metrics service should use basic auth.
    pub auth_enabled: bool,

    /// The basic auth username. Override default configuration if basic auth is enabled.
    pub username: String,

    /// The basic auth password. Override default configuration if basic auth is enabled.
    pub password: String,
}

/// [GrpcServer] holds the grpc server configuration. The grpc server is implicitly enabled if either
/// the health reports or the profile api is enabled.
#[derive(Debug, Clone, Deserialize)]
pub struct GrpcServer {
    /// Whether grpc health service should be enabled.
    pub health_enabled: bool,

    /// Whether grpc profile api service should be enabled.
    pub profile_enabled: bool,

    /// The address of the grpc server. E.g. `0.0.0.0:50051` for running with an exposed port.
    pub address: SocketAddr,
}

/// [Sentry] hold the sentry configuration. The release is automatically inferred from cargo.
#[derive(Debug, Clone, Deserialize)]
pub struct Sentry {
    /// Whether sentry should be enabled.
    pub enabled: bool,

    /// Whether sentry should have debug enabled.
    pub debug: bool,

    /// The address of the sentry instance. This can either be the official sentry or a self-hosted instance.
    /// The address has to bes event if sentry is disabled. In that case, the address can be any non-nil value.
    pub address: String,

    /// The environment of the application that should be communicated to sentry.
    pub environment: String,
}

/// [Logging] hold the log configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    /// The log level that should be printed.
    #[serde(deserialize_with = "parse_level_filter")]
    pub level: LevelFilter,
}

/// [Settings] holds all configuration for the application. I.g. one immutable instance is created
/// on startup and then shared among the application components.
///
/// If both the grpc and rest server are disabled, the application will exit immediately after startup
/// with status ok.
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    /// Whether the profiles should be requested with a signature.
    pub unsigned_profile: bool,

    /// The logging configuration.
    pub logging: Logging,

    /// The service cache configuration.
    pub cache: Cache,

    /// The metrics configuration. The metrics service is part of the [RestServer].
    pub metrics: Metrics,

    /// The sentry configuration.
    pub sentry: Sentry,

    /// The rest server configuration. It will be enabled if either the rest gateway is enabled or the metrics.
    pub rest_server: RestServer,

    /// The grpc server configuration.
    pub grpc_server: GrpcServer,
}

impl Settings {
    /// Creates a new application configuration as described in the [module documentation](crate::settings).
    pub fn new() -> Result<Self, ConfigError> {
        // the environment prefix for all `Settings` fields
        let env_prefix = env::var("ENV_PREFIX").unwrap_or("xenos".into());
        // the name of the configuration files (used by the deployment)
        let default_file = env::var("CONFIG_DEFAULT_FILE").unwrap_or("config/default".into());
        let custom_file = env::var("CONFIG_CUSTOM_FILE").unwrap_or("config/custom".into());

        let s = Config::builder()
            // load configuration files: default -> custom
            .add_source(File::with_name(&default_file))
            .add_source(File::with_name(&custom_file).required(false))
            // add in settings from the environment (with a prefix of APP)
            // e.g. `XENOS__DEBUG=1` would set the `debug` key, on the other hand,
            // `XENOS__CACHE__REDIS__ENABLED=1` would enable the redis cache.
            .add_source(Environment::with_prefix(&env_prefix).separator("__"))
            .build()?;

        // you can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}

/// Deserializer that parses an [iso8601] duration string to a [Duration]. E.g. `PT1M` is a duration
/// of one minute.
pub fn parse_duration<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    let iso: iso8601::Duration = Deserialize::deserialize(deserializer)?;
    Ok(Duration::from(iso))
}

/// Deserializer for [LevelFilter] from string. E.g. `info`.
pub fn parse_level_filter<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<LevelFilter, D::Error> {
    let level: String = Deserialize::deserialize(deserializer)?;
    LevelFilter::from_str(&level)
        .map_err(|_| serde::de::Error::invalid_value(Unexpected::Str(&level), &"log level string"))
}
