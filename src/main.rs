use std::env;
use tokio::sync::Mutex;
use tonic::transport::Server;
use xenos::cache::RedisCache;
use xenos::mojang::Mojang;
use xenos::service::pb::xenos_server::XenosServer;
use xenos::service::XenosService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read configuration from environment
    let addr_str = env::var("SERVER_ADDR").unwrap_or("[::1]:50051".to_string());
    let redis_str = env::var("REDIS_ADDR").expect("redis address required");

    // build redis client and cache
    let redis_client = redis::Client::open(redis_str)?;
    let redis_manager = redis_client.get_connection_manager().await?;
    let cache = Box::new(Mutex::new(RedisCache { redis_manager }));

    // build mojang api
    let mojang = Box::new(Mojang {});

    // build grpc service
    let service = XenosService { cache, mojang };
    let svc = XenosServer::new(service);

    let addr = addr_str.parse().unwrap();
    println!("XenosServer listening on {}", addr);
    Server::builder().add_service(svc).serve(addr).await?;
    Ok(())
}
