use crate::error::XenosError;
use crate::proto::{
    HeadRequest, HeadResponse, ProfileRequest, ProfileResponse, SkinRequest, SkinResponse,
    UuidRequest, UuidResponse,
};
use crate::service::Service;
use actix_web::body::BoxBody;
use actix_web::http::header::CONTENT_TYPE;
use actix_web::http::StatusCode;
use actix_web::{error, get, post, web, HttpRequest, HttpResponse, Responder};
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use uuid::Uuid;

pub struct State {
    pub service: Arc<Service>,
}

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

#[post("/uuids")]
pub async fn get_uuids(
    data: web::Data<State>,
    json: web::Json<UuidRequest>,
) -> Result<impl Responder, XenosError> {
    let usernames = json.usernames.clone();
    let response: UuidResponse = data.service.get_uuids(&usernames).await?.into();
    Ok(web::Json(response))
}

#[post("/profile")]
pub async fn get_profile(
    data: web::Data<State>,
    json: web::Json<ProfileRequest>,
) -> Result<impl Responder, XenosError> {
    let uuid = Uuid::try_parse(&json.uuid)?;
    let response: ProfileResponse = data.service.get_profile(&uuid).await?.into();
    Ok(web::Json(response))
}

#[post("/skin")]
pub async fn get_skin(
    data: web::Data<State>,
    json: web::Json<SkinRequest>,
) -> Result<impl Responder, XenosError> {
    let uuid = Uuid::try_parse(&json.uuid)?;
    let response: SkinResponse = data.service.get_skin(&uuid).await?.into();
    Ok(web::Json(response))
}

#[post("/head")]
pub async fn get_head(
    data: web::Data<State>,
    json: web::Json<HeadRequest>,
) -> Result<impl Responder, XenosError> {
    let uuid = Uuid::try_parse(&json.uuid)?;
    let response: HeadResponse = data.service.get_head(&uuid, &json.overlay).await?.into();
    Ok(web::Json(response))
}
