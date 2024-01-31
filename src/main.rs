use std::env;
use tokio::sync::Mutex;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use xenos::cache::RedisCache;
use xenos::mojang::Mojang;
use xenos::service::pb::profile_server::ProfileServer;
use xenos::service::XenosService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read configuration from environment
    let addr_str = env::var("SERVER_ADDR").unwrap_or("0.0.0.0:50051".to_string());
    let redis_str = env::var("REDIS_ADDR").expect("redis address required");

    // build redis client and cache
    let redis_client = redis::Client::open(redis_str)?;
    let redis_manager = redis_client.get_connection_manager().await?;
    let cache = Box::new(Mutex::new(RedisCache { redis_manager }));

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

    let addr = addr_str.parse().expect("listen address invalid format");
    println!("Xenos listening on {}", addr);
    Server::builder()
        .add_service(health_service)
        .add_service(profile_service)
        .serve(addr)
        .await?;
    Ok(())
}
