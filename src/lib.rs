//! Xenos is a Minecraft profile data proxy that can be used as an ultra-fast replacement for the official Mojang API.
//!
//! This library provides all components used to build the application.
//!
//! # Usage
//!
//! At the center of the application is the [xenos service](service::Service). It encapsulates an implementation
//! of a [cache](cache::XenosCache) and a [Mojang api](mojang::Mojang). Currently, the library supports
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
//! An additional, optional config file can be applied by setting the `CONFIG_FILE` environment variable.
//! By default, it is set to `config/config`, loading any supported file type. It is intended to be supplied
//! by the deployment.
//!
//! ```rs
//! let settings = Setting::new()
//! ```
//!
//! # Data Formats
//!
//! The library uses three distinct data formats. Firstly, the data format provided by the [Mojang api](mojang::Mojang).
//! Secondly, the format used by the [cache](cache::XenosCache). This format is used for all internal data handling.
//! And lastly, the data transfer format used by the exposing servers. This format is defined in the
//! `.proto` files and exported into the [proto module](proto).
//!
//! # Errors
//!
//! Xenos provides its own [error type](error::XenosError). These errors are converted to appropriate
//! external error responses (for http and grpc server). See [XenosError](error::XenosError) for detailed information.
//!

use crate::cache::chaining::ChainingCache;
use crate::cache::moka::MokaCache;
use crate::cache::redis::RedisCache;
use crate::cache::XenosCache;
use crate::grpc_services::GrpcProfileService;
use crate::http_services::{configure_metrics, configure_rest_gateway};
use crate::mojang::mojang::MojangApi;
use crate::mojang::testing::MojangTestingApi;
use crate::mojang::Mojang;
use crate::proto::profile_server::ProfileServer;
use crate::service::Service;
use crate::settings::Settings;
use actix_web::{web, App, HttpServer};
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use tracing::info;

pub mod cache;
pub mod error;
pub mod grpc_services;
pub mod http_services;
pub mod mojang;
pub mod proto;
pub mod service;
pub mod settings;

/// Starts Xenos, should only be called once in `main` after sentry and logging have be initialized.
/// It blocks until all started services complete (or after a graceful shutdown when a shutdown signal
/// is received)
#[tracing::instrument(skip(settings))]
pub async fn start(settings: Arc<Settings>) -> Result<(), Box<dyn std::error::Error>> {
    info!(debug = settings.debug, "starting Xenos...");

    // build chaining cache with selected caches
    // it consists of a local and remote cache
    info!("building chaining cache from caches");
    let cache = Box::new(
        ChainingCache::new()
            // the top most layer is a local (in-memory) cache, in this case a moka cache
            .add_cache(true /* TODO enable from settings */, || async {
                info!("adding moka cache to chaining cache");
                let cache = MokaCache::new(&settings.cache.moka);
                Ok(Box::new(cache) as Box<dyn XenosCache>)
            })
            .await?
            // the next layer is a remote cache, in this case a redis cache
            .add_cache(true /* TODO enable from settings */, || async {
                info!("adding redis cache to chaining cache");
                let cs = &settings.cache;
                let redis_client = redis::Client::open(cs.redis.address.clone())?;
                let redis_manager = redis_client.get_connection_manager().await?;
                let cache = RedisCache::new(redis_manager, &settings.cache.redis);
                Ok(Box::new(cache) as Box<dyn XenosCache>)
            })
            .await?,
    );

    // build mojang api
    // it is either the actual mojang api or a testing api for integration tests
    info!("building mojang api");
    let mut mojang: Box<dyn Mojang> = Box::new(MojangApi::new());
    if settings.testing {
        info!("replacing mojang api with testing api, DISABLE IN PRODUCTION!");
        mojang = Box::new(MojangTestingApi::new())
    }

    // build xenos service from cache and mojang api
    // the service is then shared by the grpc and rest servers
    info!("building shared xenos service");
    let service = Arc::new(Service::new(cache, mojang));

    // try to start grpc and rest servers (disabled servers return directly)
    try_join!(
        run_grpc(service.clone(), settings.clone()),
        run_http(service.clone(), settings.clone()),
    )?;
    info!("xenos stopped successfully");
    Ok(())
}

// TODO refactor and use axum
/// Tries to start the http server. The http server is started if either the rest gateway or the
/// metrics service is enabled. Blocks until shutdown (graceful shutdown).
#[tracing::instrument(skip_all)]
async fn run_http(
    service: Arc<Service>,
    settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !settings.metrics.enabled && !settings.http_server.rest_gateway {
        info!("http server is disabled (enable either metrics or rest gateway)");
        return Ok(());
    }

    let address = settings.http_server.address;
    let metrics_enabled = settings.metrics.enabled;
    let rest_gateway_enabled = settings.http_server.rest_gateway;
    info!(
        address = settings.http_server.address.to_string(),
        metrics = metrics_enabled,
        rest_gateway = rest_gateway_enabled,
        "http server listening on {}",
        settings.http_server.address
    );
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(http_services::State {
                settings: settings.clone(),
                service: service.clone(),
            }))
            .configure(|cfg| {
                if metrics_enabled {
                    configure_metrics(cfg)
                }
            })
            .configure(|cfg| {
                if rest_gateway_enabled {
                    configure_rest_gateway(cfg)
                }
            })
    })
    .bind(address)?
    .run()
    .await?;
    info!("http server stopped successfully");
    Ok(())
}

// TODO refactor
/// Tries to start the grpc server. The grpc server is started if it is enabled. It also starts the
/// health reporter. Blocks until shutdown (graceful shutdown).
#[tracing::instrument(skip_all)]
async fn run_grpc(
    service: Arc<Service>,
    settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !settings.grpc_server.profile_enabled && !settings.grpc_server.health_enabled {
        info!("gRPC server is disabled (enable either health or profile)");
        return Ok(());
    }

    // build profile server
    let profile_server = settings
        .grpc_server
        .profile_enabled
        .then(|| ProfileServer::new(GrpcProfileService { service }));

    // build health server
    let mut health_server = None;
    if settings.grpc_server.health_enabled {
        info!("initializing gRPC health reporter");
        let (mut reporter, server) = health_reporter();
        reporter
            .set_serving::<ProfileServer<GrpcProfileService>>()
            .await;
        health_server = Some(server)
    }

    // shutdown signal (future)
    let shutdown = tokio::signal::ctrl_c().map(|_| ());

    info!(
        address = settings.grpc_server.address.to_string(),
        health = settings.grpc_server.health_enabled,
        profile = settings.grpc_server.profile_enabled,
        "gRPC server listening on {}",
        settings.grpc_server.address
    );
    Server::builder()
        .add_optional_service(health_server)
        .add_optional_service(profile_server)
        .serve_with_shutdown(settings.grpc_server.address, shutdown)
        .await?;
    info!("gRPC server stopped successfully");
    Ok(())
}
