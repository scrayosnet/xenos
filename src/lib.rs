//! Xenos is a Minecraft profile data proxy that can be used as an ultra-fast replacement for the official Mojang API.
//!
//! # Usage
//!
//! Start the application by first initializing [sentry] and [tracing] and then calling [start] with
//! the [application configuration](settings).
//!
//! # Configuration
//!
//! See [settings] for a description on how to create the application configuration.

use crate::cache::level::moka::MokaCache;
use crate::cache::level::no::NoCache;
#[cfg(feature = "redis")]
use crate::cache::level::redis::RedisCache;
use crate::cache::level::CacheLevel;
use crate::cache::Cache;
use crate::grpc_services::GrpcProfileService;
#[cfg(not(feature = "static-testing"))]
use crate::mojang::api::MojangApi;
#[cfg(feature = "static-testing")]
use crate::mojang::testing::MojangTestingApi;
use crate::mojang::Mojang;
use crate::proto::profile_server::ProfileServer;
use crate::service::Service;
use crate::settings::Settings;
use axum::routing::{post, MethodRouter};
use axum::{routing::get, Extension, Router};
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use tracing::info;

pub mod cache;
pub mod error;
mod grpc_services;
pub mod mojang;
pub mod proto;
mod rest_services;
pub mod service;
pub mod settings;

/// [OptionalRoute] is a utility used to add optional routers to a [Router]. Routes can be disabled
/// on construction based on application settings.
trait OptionalRoute<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Adds a [route](MethodRouter) with a path to the [Router] if enabled. See [Router::route].
    fn optional_route(self, enabled: bool, path: &str, method_router: MethodRouter<S>) -> Self;
}

impl<S> OptionalRoute<S> for Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn optional_route(self, enabled: bool, path: &str, method_router: MethodRouter<S>) -> Self {
        if enabled {
            self.route(path, method_router)
        } else {
            self
        }
    }
}

/// Starts Xenos with the provided [application configuration](settings). It expects that [sentry] and
/// [tracing] was configured beforehand. It blocks until a shutdown signal is received (graceful shutdown).
#[tracing::instrument(skip(settings))]
pub async fn start(settings: Arc<Settings>) -> Result<(), Box<dyn std::error::Error>> {
    info!("starting xenos …");

    // build cache with selected cache levels
    info!("building multi-level cache");
    let cache = Cache::new(
        settings.cache.entries.clone(),
        {
            info!("building moka cache");
            MokaCache::new(settings.cache.moka.clone())
        },
        // the remote cache should be selected using feature flags
        {
            #[cfg(feature = "redis")]
            {
                info!("building redis cache");
                let cs = &settings.cache;
                let redis_client = redis::Client::open(cs.redis.address.clone())?;
                let redis_manager = redis_client.get_connection_manager().await?;
                RedisCache::new(redis_manager, &settings.cache.redis)
            }
            #[cfg(not(feature = "redis"))]
            {
                info!("disabling remote cache");
                NoCache::new()
            }
        },
    );

    // build mojang api
    // it is either the actual mojang api or a testing api for integration tests
    info!("building mojang api");
    #[cfg(not(feature = "static-testing"))]
    let mojang = MojangApi::new();
    #[cfg(feature = "static-testing")]
    let mojang = MojangTestingApi::with_profiles();

    // build xenos service from cache and mojang api
    // the service is then shared by the grpc and rest servers
    info!("building shared xenos service");
    let service = Arc::new(Service::new(settings.clone(), cache, mojang));

    try_join!(
        serve_rest_server(Arc::clone(&service)),
        serve_grpc_server(Arc::clone(&service)),
    )?;
    info!("xenos stopped successfully");
    Ok(())
}

/// Tries to start the rest server. The rest server is started if either the rest gateway or the
/// metrics service is enabled. Blocks until shutdown (graceful shutdown).
#[tracing::instrument(skip_all)]
async fn serve_rest_server<L, R, M>(
    service: Arc<Service<L, R, M>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    L: CacheLevel + 'static,
    R: CacheLevel + 'static,
    M: Mojang + 'static,
{
    let settings = service.settings();
    let address = settings.rest_server.address;
    let metrics_enabled = settings.metrics.enabled;
    let gateway_enabled = settings.rest_server.rest_gateway;

    // check if rest server should be started
    if !metrics_enabled && !gateway_enabled {
        info!("rest server is disabled (enable either metrics or rest gateway)");
        return Ok(());
    }

    // build rest server
    let rest_app = Router::new()
        .optional_route(
            metrics_enabled,
            "/metrics",
            get(rest_services::metrics::<L, R, M>),
        )
        .optional_route(
            gateway_enabled,
            "/uuid",
            post(rest_services::uuid::<L, R, M>),
        )
        .optional_route(
            gateway_enabled,
            "/uuids",
            post(rest_services::uuids::<L, R, M>),
        )
        .optional_route(
            gateway_enabled,
            "/profile",
            post(rest_services::profile::<L, R, M>),
        )
        .optional_route(
            gateway_enabled,
            "/skin",
            post(rest_services::skin::<L, R, M>),
        )
        .optional_route(
            gateway_enabled,
            "/cape",
            post(rest_services::cape::<L, R, M>),
        )
        .optional_route(
            gateway_enabled,
            "/head",
            post(rest_services::head::<L, R, M>),
        )
        .layer(Extension(Arc::clone(&service)))
        .with_state(());

    // register shutdown signal (as future)
    let shutdown = tokio::signal::ctrl_c().map(|_| ());

    info!(
        address = address.to_string(),
        metrics = metrics_enabled,
        rest_gateway = gateway_enabled,
        "rest server listening on {}",
        address
    );
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    axum::serve(listener, rest_app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
    info!("rest server stopped successfully");
    Ok(())
}

/// Tries to start the grpc server. The grpc server is started if it is enabled. It also starts the
/// health reporter. Blocks until shutdown (graceful shutdown).
#[tracing::instrument(skip_all)]
async fn serve_grpc_server<L, R, M>(
    service: Arc<Service<L, R, M>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    L: CacheLevel + 'static,
    R: CacheLevel + 'static,
    M: Mojang + 'static,
{
    let settings = service.settings();
    let address = settings.grpc_server.address;
    let health_enabled = settings.grpc_server.health_enabled;
    let profile_enabled = settings.grpc_server.profile_enabled;

    // check if grpc server should be started
    if !profile_enabled && !health_enabled {
        info!("gRPC server is disabled (enable either health or profile)");
        return Ok(());
    }

    // build profile server
    let mut profile_server = None;
    if profile_enabled {
        let server = ProfileServer::new(GrpcProfileService::new(Arc::clone(&service)));
        profile_server = Some(server);
    }

    // build health server
    let mut health_server = None;
    if health_enabled {
        let (mut reporter, server) = health_reporter();
        reporter
            .set_serving::<ProfileServer<GrpcProfileService<L, R, M>>>()
            .await;
        health_server = Some(server)
    }

    // register shutdown signal (as future)
    let shutdown = tokio::signal::ctrl_c().map(|_| ());

    info!(
        address = address.to_string(),
        health = health_enabled,
        profile = profile_enabled,
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
