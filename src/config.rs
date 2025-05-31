//! The config module defines the application configuration. It is based on [config], a layered
//! configuration system for Rust applications (with strong support for 12-factor applications).
//!
//! # Layers
//!
//! The configuration consists of up to three layers. Upper layers overwrite lower layer configurations
//! (e.g., environment variables overwrite the default configuration).
//!
//! ## Layer 1 (Environment variables) \[optional\]
//!
//! The environment variables are the top most layer. They can be used to overwrite any previous configuration.
//! Environment variables have the format `[ENV_PREFIX]_[field]_[sub_field]` where `ENV_PREFIX` is
//! an environment variable defaulting to `XENOS`. That means the nested config field `cache.redis.enabled`
//! can be overwritten by the environment variable `XENOS_CACHE_REDIS_ENABLED`.
//!
//! ## Layer 2 (Custom configuration) \[optional\]
//!
//! The next layer is an optional configuration file intended to be used by deployments and local testing. The file
//! location can be configured using the `CONFIG_FILE` environment variable, defaulting to `config/config`.
//! It can be of any file type supported by [config] (e.g. `config/config.toml`). The file should not be
//! published by git as its configuration is context-dependent (e.g., local/cluster) and probably contains
//! secrets.
//!
//! ## Layer 3 (Default configuration)
//!
//! The default configuration provides the default value for all config fields. It is loaded from
//! `config/default.toml` at compile time.
//!
//! # Usage
//!
//! The application configuration can be created by using [Config::new]. This loads/overrides the
//! configuration fields layer-by-layer.
//!
//! ```rs
//! let config: Config = Config::new()?;
//! ```

use config::{ConfigError, Environment, File, FileFormat};
use serde::Deserialize;
use serde::Deserializer;
use serde::de::{Error, Unexpected, Visitor};
use std::env;
use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

/// [Cache] hold the service cache configurations. The different caches are accumulated by the
/// [Cache](crate::cache::Cache). If no cache is `enabled`, caching is effectively disabled.
///
/// In general, there should always be a local cache (e.g. [moka](MokaCache)) enabled and optionally
/// a remote cache (e.g. [redis](RedisCache)).
#[derive(Debug, Clone, Deserialize)]
pub struct Cache {
    pub entries: CacheEntries<CacheEntry>,

    /// The [redis] cache configuration.
    #[cfg(feature = "redis")]
    pub redis: RedisCache,

    /// The [moka] cache configuration.
    pub moka: MokaCache,
}

/// [MokaCache] hold the [moka] cache configuration. Moka is a fast in-memory (local) cache. It
/// supports [MokaCacheEntry] `ttl` and `tti` and `cap` per cache entry type.
#[derive(Debug, Clone, Deserialize)]
pub struct MokaCache {
    /// The configuration for the cache entries.
    pub entries: CacheEntries<MokaCacheEntry>,
}

/// [RedisCache] hold the [redis] cache configuration. Redis is a fast remote cache. It supports
/// [RedisCacheEntry] `ttl` per cache entry type but not `tti` and `cap`.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisCache {
    /// The address of the redis instance (e.g. `redis://username:password@example.com/0`). Only used
    /// if redis is enabled.
    pub address: String,

    /// The configuration for the cache entries.
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
    /// The cache entry expiration duration. If elapsed, then the cache entry is marked as expired
    /// but not deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub exp: Duration,

    /// The cache entry expiration duration for empty cache entries (e.g., username not found). If
    /// elapsed, then the cache entry is marked as expired but not deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub exp_empty: Duration,

    /// The cache entry expiration duration offset for randomness.
    #[serde(deserialize_with = "parse_duration", default)]
    pub offset: Duration,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MokaCacheEntry {
    /// The cache max capacity. May be supported by cache.
    pub cap: u64,

    /// The cache entry time-to-life. If elapsed, then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl: Duration,

    /// The cache entry time-to-life for empty cache entries (e.g., username not found). If elapsed,
    /// then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl_empty: Duration,

    /// The cache entry time-to-idle. If elapsed, then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub tti: Duration,

    /// The cache entry time-to-idle for empty cache entries (e.g., username not found). If elapsed,
    /// then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub tti_empty: Duration,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisCacheEntry {
    /// The cache entry time-to-life. If elapsed, then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl: Duration,

    /// The cache entry time-to-life for empty cache entries (e.g., username not found). If elapsed,
    /// then the cache entry is deleted.
    #[serde(deserialize_with = "parse_duration")]
    pub ttl_empty: Duration,
}

/// [RestServer] holds the rest server configuration. The rest server is implicitly enabled if either
/// the rest gateway of the metrics service is enabled. If enabled, the rest server also exposes the
/// metrics service at `/metrics`.
///
/// The rest gateway exposes the grpc service api over HTTP REST.
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

    /// The basic auth username. Override the default configuration if basic auth is enabled.
    pub username: String,

    /// The basic auth password. Override the default configuration if basic auth is enabled.
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

/// [Config] holds all configuration for the application. I.g. one immutable instance is created
/// on startup and then shared among the application components.
///
/// If both the grpc and rest server are disabled, the application will exit immediately after startup
/// with status ok.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Whether the profiles should be requested with a signature.
    pub signed_profiles: bool,
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

impl Config {
    /// Creates a new application configuration as described in the [module documentation](crate::config).
    pub fn new() -> Result<Self, ConfigError> {
        // the environment prefix for all `Config` fields
        let env_prefix = env::var("ENV_PREFIX").unwrap_or("xenos".into());
        // the path of the custom configuration file
        let config_file = env::var("CONFIG_FILE").unwrap_or("config/config".into());

        let s = config::Config::builder()
            // load default configuration (embedded at compile time)
            .add_source(File::from_str(
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/default.toml")),
                FileFormat::Toml,
            ))
            // load custom configuration from file (at runtime)
            .add_source(File::with_name(&config_file).required(false))
            // add in config from the environment (with a prefix of APP)
            // e.g. `XENOS_DEBUG=1` would set the `debug` key, on the other hand,
            // `XENOS_CACHE_REDIS_ENABLED=1` would enable the redis cache.
            .add_source(Environment::with_prefix(&env_prefix).separator("_"))
            .build()?;

        // you can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}

impl Default for Config {
    fn default() -> Self {
        let s = config::Config::builder()
            // load default configuration (embedded at compile time)
            .add_source(File::from_str(
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/default.toml")),
                FileFormat::Toml,
            ))
            .build()
            .expect("expected default configuration to be available");

        // you can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
            .expect("expected default configuration to be deserializable")
    }
}

/// Deserializer that parses an [iso8601] duration string or number of seconds to a [Duration].
/// E.g. `PT1M` or `60` is a duration of one minute.
pub fn parse_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    struct DurationVisitor;

    impl Visitor<'_> for DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "an iso duration or number of seconds")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            match u64::try_from(v) {
                Ok(u) => self.visit_u64(u),
                Err(_) => Err(Error::invalid_type(
                    Unexpected::Signed(v),
                    &"a positive number of seconds",
                )),
            }
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Duration::from_secs(v))
        }

        fn visit_str<E>(self, value: &str) -> Result<Duration, E>
        where
            E: Error,
        {
            match iso8601::Duration::from_str(value) {
                Ok(iso) => Ok(Duration::from(iso)),
                Err(_) => Err(Error::invalid_value(
                    Unexpected::Str(value),
                    &"an iso duration",
                )),
            }
        }
    }

    deserializer.deserialize_any(DurationVisitor)
}
