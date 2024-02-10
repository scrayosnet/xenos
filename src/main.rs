use actix_web::{web, App, HttpServer};
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
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
use xenos::settings::Settings;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read settings from config files and environment variables
    let settings = Settings::new()?;

    // build service
    let cache: Box<Mutex<dyn XenosCache>> = if settings.redis_cache.enabled {
        println!("Using redis cache");
        let redis_client = redis::Client::open(settings.redis_cache.address.clone())?;
        let redis_manager = redis_client.get_connection_manager().await?;
        Box::new(Mutex::new(RedisCache {
            cache_time: settings.redis_cache.cache_time,
            expiration: settings.redis_cache.expiration,
            redis_manager,
        }))
    } else if settings.memory_cache.enabled {
        println!("Using in-memory cache");
        Box::new(Mutex::new(MemoryCache::with_cache_time(
            settings.memory_cache.cache_time,
        )))
    } else {
        println!("Cache is disabled");
        Box::new(Mutex::new(Uncached::default()))
    };
    let mojang = Box::new(Mojang {});
    let service = Arc::new(Service { cache, mojang });

    try_join!(
        run_grpc(service.clone(), &settings),
        run_http(service.clone(), &settings),
    )?;
    Ok(())
}

async fn run_http(
    service: Arc<Service>,
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Http server listening on {}",
        settings.metrics_server.address
    );
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(http_services::State {
                service: service.clone(),
            }))
            .service(get_uuids)
            .service(get_profile)
            .service(get_skin)
            .service(get_head)
            .service(metrics)
    })
    .bind(settings.metrics_server.address)?
    .run()
    .await?;
    println!("Http server stopped");
    Ok(())
}

async fn run_grpc(
    service: Arc<Service>,
    settings: &Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    // build grpc service
    let profile_service = GrpcProfileService { service };
    let profile_server = ProfileServer::new(profile_service);

    // build grpc health reporter
    let (mut health_reporter, health_server) = health_reporter();
    health_reporter
        .set_serving::<ProfileServer<GrpcProfileService>>()
        .await;

    // shutdown signal (future)
    let shutdown = tokio::signal::ctrl_c().map(|_| ());

    println!("Grpc listening on {}", settings.grpc_server.address);
    Server::builder()
        .add_service(health_server)
        .add_service(profile_server)
        .serve_with_shutdown(settings.grpc_server.address, shutdown)
        .await?;
    println!("Grpc server stopped");
    Ok(())
}
