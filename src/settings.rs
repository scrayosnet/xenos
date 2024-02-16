use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;

/// `Redis` hold the redis cache configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisCache {
    /// The redis instance address. If redis is not selected, this field can be any value.
    pub address: String,
    /// The time-to-live for cache entries. This should be set higher than the expiry. It is used to
    /// "limit" the number of cache entries.
    pub ttl: Option<usize>,
}

/// The different supported cache variants.
#[derive(Debug, Clone, Deserialize)]
pub enum CacheVariant {
    /// Use redis caching.
    Redis,
    /// Store cache entries in-memory. Useful for testing and if no persistent storage can be used.
    /// To get the most out of Xenos, a persistent storage option should be chosen.
    Memory,
    /// Disable caching. This variant is primarily intended for local testing. It may be useful if
    /// Xenos is oly used to provide a consistent api interface for the mojang api. But generally
    /// this variant should be avoided.
    None,
}

impl Display for CacheVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// `Cache` hold the service cache configuration. The service supports multiple cache variants tht can
/// be selected with `variant`. The cache considers entry's to be expired if they reach a configured age.
/// The expiry can be configured for each cache resource type and if the cache entry indicates that,
/// for example, an uuid or name is not a valid profile id.
///
/// Nested fields are used for cache variant specific configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Cache {
    /// The variant of the cache that should be used.
    pub variant: CacheVariant,
    /// Redis specific configuration.
    pub redis: RedisCache,
    /// The expiry for username to uuid cache entries.
    pub expiry_uuid: u64,
    /// The expiry for not awarded username cache entries.
    pub expiry_uuid_missing: u64,
    /// The expiry for uuid to profile cache entries.
    pub expiry_profile: u64,
    /// The expiry for not awarded uuid cache entries.
    pub expiry_profile_missing: u64,
    /// The expiry for uuid to skin cache entries.
    pub expiry_skin: u64,
    /// The expiry for not awarded uuid cache entries.
    pub expiry_skin_missing: u64,
    /// The expiry for uuid to head cache entries.
    pub expiry_head: u64,
    /// The expiry for not awarded uuid cache entries.
    pub expiry_head_missing: u64,
}

/// `HttpServer` holds the http server configuration. The http server is implicitly enabled if either
/// the rest gateway of the metrics service is enabled. If enabled, the http server also exposes the
/// metrics service at `/metrics`.
///
/// The rest gateway exposes the grpc service api over rest.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpServer {
    /// Whether the rest gateway should be enabled.
    pub rest_gateway: bool,
    /// The address of the http server. E.g. `0.0.0.0:9990` for running with an exposed port.
    pub address: SocketAddr,
}

/// `Metrics` holds the metrics service configuration. The metrics service is part of the http server.
/// The http server will be, if not already so, implicitly enabled if the metrics service is enabled.
/// If enabled, it is exposed at the http server at `/metrics`.
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

/// `GrpcServer` holds the grpc server configuration. The grpc server is implicitly enabled if either
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

/// `Sentry` hold the sentry configuration. The release is automatically inferred from cargo.
#[derive(Debug, Clone, Deserialize)]
pub struct Sentry {
    /// Whether sentry should be enabled.
    pub enabled: bool,
    /// The address of the sentry instance. This can either be the official sentry or a self-hosted instance.
    /// The address has to bes event if sentry is disabled. In that case, the address can be any non-nil value.
    pub address: String,
    /// The environment of the application that should be communicated to sentry.
    pub environment: String,
}

/// `Settings` holds all configuration for the application. I.g. one immutable instance is created
/// on startup and then shared among the application components.
///
/// If both the grpc and http server are disabled, the application will exit immediately after startup
/// with status ok.
#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Settings {
    /// Whether the application should be in debug mode. Application components may provide additional
    /// functionalities or outputs in debug mode.
    pub debug: bool,
    /// The service cache configuration.
    pub cache: Cache,
    /// The metrics configuration. The metrics service is part of the [`http_server`](HttpServer).
    pub metrics: Metrics,
    /// The sentry configuration.
    pub sentry: Sentry,
    /// The http server configuration. The http server will be enabled if either the rest gateway is enabled or the metrics.
    pub http_server: HttpServer,
    /// The grpc server configuration.
    pub grpc_server: GrpcServer,
}

impl Settings {
    /// Loads and creates a new instance of the application settings.
    /// The settings are composed of the `config/default`, the `config/local`, the environment variables,
    /// and an optional configuration file at `CONFIG_FILE` (default: `config/config`).
    pub fn new() -> Result<Self, ConfigError> {
        // the environment prefix for all `Settings` fields
        let env_prefix = env::var("ENV_PREFIX").unwrap_or_else(|_| "xenos".into());
        // the name of an additional optional configuration file
        // it is intended to be used by the deployment
        let config_file = env::var("CONFIG_FILE").unwrap_or_else(|_| "config/config".into());

        let s = Config::builder()
            // start off by merging in the "default" configuration file
            .add_source(File::with_name("config/default"))
            // add in a local configuration file
            // this file shouldn't be checked in to git
            .add_source(File::with_name("config/local").required(false))
            // add in a configured configuration file
            // it is intended to be supplied by the deployment
            .add_source(File::with_name(&config_file).required(false))
            // add in settings from the environment (with a prefix of APP)
            // e.g. `XENOS__DEBUG=1` would set the `debug` key, on the other hand,
            // `XENOS__CACHE__VARIANT=redis` would enable the redis cache.
            .add_source(Environment::with_prefix(&env_prefix).separator("__"))
            .build()?;

        // you can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}
