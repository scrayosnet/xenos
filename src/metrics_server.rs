use actix_web::http::header::CONTENT_TYPE;
use actix_web::{get, HttpResponse, Responder};
use lazy_static::lazy_static;
use prometheus::{
    register_histogram, register_int_counter, Encoder, Histogram, IntCounter, TextEncoder,
};

lazy_static! {
    static ref METRICS_REQ_TOTAL: IntCounter = register_int_counter!(
        "metrics_requests_total",
        "Total number of requests to the metrics endpoint."
    )
    .unwrap();
    static ref METRICS_REQ_HISTOGRAM: Histogram = register_histogram!(
        "metrics_request_duration_seconds",
        "The metrics request latencies in seconds.",
        vec![0.003, 0.005, 0.010, 0.015, 0.025, 0.050, 0.075, 0.100, 0.150, 0.200]
    )
    .unwrap();
}

#[get("/metrics")]
pub async fn metrics() -> impl Responder {
    METRICS_REQ_TOTAL.inc();
    let timer = METRICS_REQ_HISTOGRAM.start_timer();

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let response = HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, encoder.format_type()))
        .body(buffer);

    timer.observe_duration();
    response
}
