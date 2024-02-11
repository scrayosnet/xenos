//! Xenos is a Minecraft profile data proxy that can be used as an ultra-fast replacement for the official Mojang API.
//!
//! This library provides all components used to build the application.
//!
//! # Usage
//!
//! At the center of the application is the [xenos service](service::Service). It encapsulates an implementation
//! of a [cache](cache::XenosCache) and a [Mojang api](mojang::MojangApi). Currently, the library supports
//! [redis](cache::redis::RedisCache) and [in-memory](cache::memory::MemoryCache) caching.
//! The service is intended to be shared by the exposing servers. Currently, the library supports
//! [grpc](grpc_services) and [rest](http_services).
//!
//! ```rs
//! let cache = Box::new(Mutex::new(MemoryCache::with_cache_time(3000)))
//! let mojang = Box::new(Mojang {});
//! let service = Arc::new(Service { cache, mojang });
//! ```
//!
//! # Configuration
//!
//! The application is configured with the [config crate](config). The config definition can be found
//! in the [settings module](settings). Configurations can be provided with the configuration files (in order)
//! `/config/[default|local]` and with environment variables.
//! Nested settings like `redis_cache.enabled` can be overwritten by the environment variable `XENOS__REDIS_CACHE__ENABLED`.
//! The env prefix `XENOS` can be altered by setting `ENV_PREFIX`.
//!
//! ```rs
//! let settings = Setting::new()
//! ```
//!
//! # Data Formats
//!
//! The library uses three distinct data formats. Firstly, the data format provided by the [Mojang api](mojang::MojangApi).
//! Secondly, the format used by the [cache](cache::XenosCache). This format is used for all internal data handling.
//! And lastly, the data transfer format used by the exposing servers. This format is defined in the
//! `.proto` files and exported into the [proto module](proto).
//!
//! # Errors
//!
//! Xenos provides its own [error type](error::XenosError). These errors are converted to appropriate
//! external error responses (for http and grpc server). See [XenosError](error::XenosError) for detailed information.
//!

pub mod cache;
pub mod error;
pub mod grpc_services;
pub mod http_services;
pub mod mojang;
pub mod proto;
pub mod service;
pub mod settings;
