use actix_web::http::header::CONTENT_TYPE;
use actix_web::{get, HttpResponse, Responder};
use prometheus::{Encoder, TextEncoder};

#[get("/metrics")]
pub async fn metrics() -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, encoder.format_type()))
        .body(buffer)
}
