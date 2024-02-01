use actix_web::http::header::CONTENT_TYPE;
use actix_web::{get, HttpResponse, Responder};
use lazy_static::lazy_static;
use prometheus::{register_int_counter, Encoder, IntCounter, TextEncoder};

lazy_static! {
    static ref METRICS_REQUESTED_TOTAL: IntCounter = register_int_counter!(
        "metrics_requested_total",
        "Total number of requests to the metrics endpoint."
    )
    .unwrap();
}

#[get("/metrics")]
pub async fn metrics() -> impl Responder {
    METRICS_REQUESTED_TOTAL.inc();

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, encoder.format_type()))
        .body(buffer)
}
