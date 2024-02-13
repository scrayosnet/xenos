use actix_web::{web, App, HttpServer};
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use tracing::info;
use tracing_subscriber;
use xenos::cache::memory::MemoryCache;
use xenos::cache::redis::RedisCache;
use xenos::cache::uncached::Uncached;
use xenos::cache::XenosCache;
use xenos::grpc_services::GrpcProfileService;
use xenos::http_services;
use xenos::http_services::{get_head, get_profile, get_skin, get_uuids, metrics};
use xenos::mojang::api::Mojang;
use xenos::proto::profile_server::ProfileServer;
use xenos::service::Service;
use xenos::settings::{CacheVariant, Settings};

#[tokio::main]
#[tracing::instrument]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // install global subscriber configured based on RUST_LOG envvar.
    tracing_subscriber::fmt::init();
    info!("Starting Xenos...");

    // read settings from config files and environment variables
    let settings = Settings::new()?;
    if settings.debug {
        info!("Debug mode enabled");
    }

    // select and build cache
    info!(
        cache = settings.cache.variant.to_string(),
        "Initializing cache"
    );
    let cache: Box<Mutex<dyn XenosCache>> = match settings.cache.variant {
        CacheVariant::Redis => {
            info!("Initializing redis client and connection pool");
            let address = settings.cache.redis.address.clone();
            let redis_client = redis::Client::open(address)?;
            let redis_manager = redis_client.get_connection_manager().await?;
            Box::new(Mutex::new(RedisCache {
                settings: settings.cache.clone(),
                redis_manager,
            }))
        }
        CacheVariant::Memory => Box::new(Mutex::new(MemoryCache::new(settings.cache.clone()))),
        CacheVariant::None => Box::new(Mutex::new(Uncached::default())),
    };

    // build service
    info!("Initializing mojang api");
    let mojang = Box::new(Mojang {});
    info!("Building Xenos service");
    let service = Arc::new(Service { cache, mojang });

    // try to start all servers (disabled servers return directly)
    try_join!(
        run_grpc(service.clone(), &settings),
        run_http(service.clone(), &settings),
    )?;
    info!("Xenos stopped successfully");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_http(
    service: Arc<Service>,
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    if !settings.metrics.enabled && !settings.http_server.rest_gateway {
        info!("Http server is disabled (enable either metrics or rest gateway)");
        return Ok(());
    }

    let rest_gateway_enabled = settings.metrics.enabled;
    let metrics_enabled = settings.http_server.rest_gateway;
    info!(
        address = settings.http_server.address.to_string(),
        metrics = metrics_enabled,
        rest_gateway = rest_gateway_enabled,
        "Http server listening on {}",
        settings.http_server.address
    );
    HttpServer::new(move || {
        let mut app = App::new().app_data(web::Data::new(http_services::State {
            service: service.clone(),
        }));
        // add rest gateway services
        if rest_gateway_enabled {
            app = app
                .service(get_uuids)
                .service(get_profile)
                .service(get_skin)
                .service(get_head)
        }
        // add metrics service
        if metrics_enabled {
            app = app.service(metrics)
        }
        app
    })
    .bind(settings.http_server.address)?
    .run()
    .await?;
    info!("Http server stopped successfully");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_grpc(
    service: Arc<Service>,
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    if !settings.grpc_server.enabled {
        info!("GRPC server is disabled");
        return Ok(());
    }

    // build grpc service
    let profile_service = GrpcProfileService { service };
    let profile_server = ProfileServer::new(profile_service);

    // build grpc health reporter
    info!("Initializing gRPC health reporter");
    let (mut health_reporter, health_server) = health_reporter();
    health_reporter
        .set_serving::<ProfileServer<GrpcProfileService>>()
        .await;

    // shutdown signal (future)
    let shutdown = tokio::signal::ctrl_c().map(|_| ());

    info!(
        address = settings.grpc_server.address.to_string(),
        "GRPC server listening on {}", settings.grpc_server.address
    );
    Server::builder()
        .add_service(health_server)
        .add_service(profile_server)
        .serve_with_shutdown(settings.grpc_server.address, shutdown)
        .await?;
    info!("GRPC server stopped successfully");
    Ok(())
}
