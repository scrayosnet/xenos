use crate::error::XenosError;
use crate::proto::{
    HeadRequest, HeadResponse, ProfileRequest, ProfileResponse, SkinRequest, SkinResponse,
    UuidRequest, UuidResponse,
};
use crate::service::Service;
use crate::settings::Settings;
use actix_web::body::BoxBody;
use actix_web::error::ErrorUnauthorized;
use actix_web::http::header::CONTENT_TYPE;
use actix_web::http::StatusCode;
use actix_web::{error, get, post, web, HttpRequest, HttpResponse, Responder};
use actix_web_httpauth::extractors::basic::BasicAuth;
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use uuid::Uuid;

// TODO add documentation and use axum

impl error::ResponseError for XenosError {
    fn status_code(&self) -> StatusCode {
        match self {
            XenosError::NotRetrieved => StatusCode::SERVICE_UNAVAILABLE,
            XenosError::NotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Responder for XenosError {
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        match self {
            XenosError::NotRetrieved => {
                HttpResponse::ServiceUnavailable().body("unable to retrieve")
            }
            XenosError::NotFound => HttpResponse::NotFound().body("resource not found"),
            err => HttpResponse::InternalServerError().body(err.to_string()),
        }
    }
}

/// `State` is the http application (root level) data. It is shared between all http service calls.
#[derive(Clone)]
pub struct State {
    pub settings: Arc<Settings>,
    pub service: Arc<Service>,
}

/// Configures the metrics service(s).
///
/// It ignores whether the metrics service should be [enabled](settings::Metrics.enabled).
/// The metrics basic auth configuration is read at runtime at every call.
///
/// This configuration can be used in along with the [rest gateway](configure_rest_gateway).
pub fn configure_metrics(cfg: &mut web::ServiceConfig) {
    // basic auth is validated by the metrics service itself
    cfg.service(metrics);
}

/// Configures the rest gateway service(s).
///
/// It ignores whether the gateway service should be [enabled](settings::HttpServer.rest_gateway).
///
/// This configuration can be used in along with the [metrics](configure_metrics).
pub fn configure_rest_gateway(cfg: &mut web::ServiceConfig) {
    cfg.service(get_uuids)
        .service(get_profile)
        .service(get_skin)
        .service(get_head);
}

#[get("/metrics")]
async fn metrics(data: web::Data<State>, auth: Option<BasicAuth>) -> impl Responder {
    // validate auth
    let ms = &data.settings.metrics;
    if ms.auth_enabled {
        if let Some(auth) = auth {
            if auth.user_id() != ms.username || auth.password() != Some(&ms.password) {
                return Err(ErrorUnauthorized("invalid credentials"));
            }
        } else {
            return Err(ErrorUnauthorized("missing credentials"));
        }
    }

    // build metrics
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    Ok(HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, encoder.format_type()))
        .body(buffer))
}

#[post("/uuids")]
async fn get_uuids(
    data: web::Data<State>,
    json: web::Json<UuidRequest>,
) -> Result<impl Responder, XenosError> {
    let usernames = json.usernames.clone();
    let response: UuidResponse = data.service.get_uuids(&usernames).await?.into();
    Ok(web::Json(response))
}

#[post("/profile")]
async fn get_profile(
    data: web::Data<State>,
    json: web::Json<ProfileRequest>,
) -> Result<impl Responder, XenosError> {
    let uuid = Uuid::try_parse(&json.uuid)?;
    let response: ProfileResponse = data.service.get_profile(&uuid).await?.into();
    Ok(web::Json(response))
}

#[post("/skin")]
async fn get_skin(
    data: web::Data<State>,
    json: web::Json<SkinRequest>,
) -> Result<impl Responder, XenosError> {
    let uuid = Uuid::try_parse(&json.uuid)?;
    let response: SkinResponse = data.service.get_skin(&uuid).await?.into();
    Ok(web::Json(response))
}

#[post("/head")]
async fn get_head(
    data: web::Data<State>,
    json: web::Json<HeadRequest>,
) -> Result<impl Responder, XenosError> {
    let uuid = Uuid::try_parse(&json.uuid)?;
    let response: HeadResponse = data.service.get_head(&uuid, &json.overlay).await?.into();
    Ok(web::Json(response))
}
