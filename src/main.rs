use actix_web::{App, HttpServer};
use futures_util::FutureExt;
use tokio::sync::Mutex;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use xenos::cache::memory::MemoryCache;
use xenos::cache::no_cache::NoCache;
use xenos::cache::redis::RedisCache;
use xenos::cache::XenosCache;
use xenos::metrics_server::metrics;
use xenos::mojang::Mojang;
use xenos::service::pb::profile_server::ProfileServer;
use xenos::service::XenosService;
use xenos::settings::Settings;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read settings from config files and environment variables
    let settings = Settings::new()?;

    // start all (possible) services
    // disabled services won't start themselves
    try_join!(run_grpc(&settings), run_metrics(&settings))?;
    Ok(())
}

async fn run_metrics(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    if !settings.metrics_server.enabled {
        println!("Metrics are disabled");
        return Ok(());
    }

    // run metrics server
    println!("Metrics listening on {}", settings.metrics_server.address);
    HttpServer::new(|| App::new().service(metrics))
        .bind(settings.metrics_server.address)?
        .run()
        .await?;
    println!("Metrics server stopped");
    Ok(())
}

async fn run_grpc(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    // build selected cache, fallback to in-memory cache
    let cache: Box<Mutex<dyn XenosCache>> = if settings.redis_cache.enabled {
        // build redis client and cache
        println!("Using redis cache");
        let redis_client = redis::Client::open(settings.redis_cache.address.clone())?;
        let redis_manager = redis_client.get_connection_manager().await?;
        Box::new(Mutex::new(RedisCache {
            cache_time: settings.redis_cache.cache_time,
            redis_manager,
        }))
    } else if settings.memory_cache.enabled {
        println!("Using in-memory cache");
        Box::new(Mutex::new(MemoryCache::with_cache_time(
            settings.memory_cache.cache_time,
        )))
    } else {
        println!("Caching is disabled");
        Box::new(Mutex::new(NoCache::default()))
    };

    // build mojang api
    let mojang = Box::new(Mojang {});

    // build grpc service
    let service = XenosService { cache, mojang };
    let profile_service = ProfileServer::new(service);

    // build grpc health reporter
    let (mut health_reporter, health_service) = health_reporter();
    health_reporter
        .set_serving::<ProfileServer<XenosService>>()
        .await;

    // shutdown signal (future)
    let shutdown = tokio::signal::ctrl_c().map(|_| ());

    println!("Grpc listening on {}", settings.grpc_server.address);
    Server::builder()
        .add_service(health_service)
        .add_service(profile_service)
        .serve_with_shutdown(settings.grpc_server.address, shutdown)
        .await?;
    println!("Grpc server stopped");
    Ok(())
}
