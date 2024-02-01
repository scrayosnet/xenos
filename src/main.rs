use actix_web::{App, HttpServer};
use futures_util::FutureExt;
use std::env;
use std::net::SocketAddr;
use tokio::sync::Mutex;
use tokio::try_join;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use xenos::cache::redis::RedisCache;
use xenos::metrics_server::metrics;
use xenos::mojang::Mojang;
use xenos::service::pb::profile_server::ProfileServer;
use xenos::service::XenosService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read configuration from environment
    let grpc_addr_str = env::var("SERVER_ADDR").unwrap_or("0.0.0.0:50051".to_string());
    let redis_addr = env::var("REDIS_ADDR").expect("redis address required");
    let metrics_addr = env::var("METRICS_ADDR").unwrap_or("0.0.0.0:3000".to_string());
    let grpc_addr = grpc_addr_str
        .parse()
        .expect("listen address invalid format");

    try_join!(run_grpc(redis_addr, grpc_addr), run_metrics(metrics_addr))?;
    Ok(())
}

async fn run_metrics(metrics_addr: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("Metrics listening on {}", metrics_addr);
    HttpServer::new(|| App::new().service(metrics))
        .bind(metrics_addr)?
        .run()
        .await?;
    println!("Metrics server stopped");
    Ok(())
}

async fn run_grpc(
    redis_addr: String,
    grpc_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    // build redis client and cache
    let redis_client = redis::Client::open(redis_addr)?;
    let redis_manager = redis_client.get_connection_manager().await?;
    let cache = Box::new(Mutex::new(RedisCache {
        cache_time: 5 * 60,
        redis_manager,
    }));

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

    println!("Grpc listening on {}", grpc_addr);
    Server::builder()
        .add_service(health_service)
        .add_service(profile_service)
        .serve_with_shutdown(grpc_addr, shutdown)
        .await?;
    println!("Grpc server stopped");
    Ok(())
}
