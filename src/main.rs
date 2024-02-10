use actix_web::{App, HttpServer};
use futures_util::FutureExt;
use tokio::sync::Mutex;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
#[cfg(all(not(feature = "cache_redis"), feature = "cache_memory"))]
use xenos::cache::memory::MemoryCache;
#[cfg(feature = "cache_redis")]
use xenos::cache::redis::RedisCache;
#[cfg(all(not(feature = "cache_redis"), not(feature = "cache_memory")))]
use xenos::cache::uncached::Uncached;
use xenos::cache::XenosCache;
use xenos::metrics_server::metrics;
use xenos::mojang::api::Mojang;
use xenos::profile_service::pb::profile_server::ProfileServer;
use xenos::profile_service::ProfileService;
use xenos::settings::Settings;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read settings from config files and environment variables
    let settings = Settings::new()?;

    try_join!(run_grpc(&settings), run_metrics(&settings),)?;
    Ok(())
}

async fn run_metrics(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    if !settings.metrics_server.enabled {
        println!("Metrics server is disabled");
        return Ok(());
    }

    // run metrics server
    println!(
        "Metrics server listening on {}",
        settings.metrics_server.address
    );
    HttpServer::new(|| App::new().service(metrics))
        .bind(settings.metrics_server.address)?
        .run()
        .await?;
    println!("Metrics server stopped");
    Ok(())
}

async fn run_grpc(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    // select cache based on feature flags
    let cache: Box<Mutex<dyn XenosCache>>;
    cfg_if::cfg_if! {
        if #[cfg(feature = "cache_redis")] {
            println!("Using redis cache");
            let redis_client = redis::Client::open(settings.redis_cache.address.clone())?;
            let redis_manager = redis_client.get_connection_manager().await?;
            cache = Box::new(Mutex::new(RedisCache {
                cache_time: settings.redis_cache.cache_time,
                expiration: settings.redis_cache.expiration,
                redis_manager,
            }));
        } else if #[cfg(all(not(feature = "cache_redis"), feature = "cache_memory"))] {
            println!("Using in-memory cache");
            cache = Box::new(Mutex::new(MemoryCache::with_cache_time(
                settings.memory_cache.cache_time,
            )));
        } else if #[cfg(all(not(feature = "cache_redis"), not(feature = "cache_memory")))] {
            println!("Cache is disabled");
            cache = Box::new(Mutex::new(Uncached::default()));
            cache = Box::default()
        } else {
            compile_error!("Failed to select cache!");
        }
    }

    // build mojang api
    let mojang = Box::new(Mojang {});

    // build grpc service
    let profile_service = ProfileService { cache, mojang };
    let profile_server = ProfileServer::new(profile_service);

    // build grpc health reporter
    let (mut health_reporter, health_server) = health_reporter();
    health_reporter
        .set_serving::<ProfileServer<ProfileService>>()
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
