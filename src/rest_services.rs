use crate::error::XenosError;
use crate::proto::{
    HeadRequest, HeadResponse, ProfileRequest, ProfileResponse, SkinRequest, SkinResponse,
    UuidRequest, UuidResponse,
};
use crate::service::Service;
use axum::{
    http,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use axum_auth::AuthBasic;
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use uuid::Uuid;

// TODO add documentation

// implement automatic XenosError to response conversion
// with that, XenosError can be returned in a result
impl IntoResponse for XenosError {
    fn into_response(self) -> Response {
        match self {
            XenosError::NotRetrieved => {
                (StatusCode::SERVICE_UNAVAILABLE, "mojang not reached").into_response()
            }
            XenosError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
        }
    }
}

pub async fn metrics(
    auth: Option<AuthBasic>,
    Extension(service): Extension<Arc<Service>>,
) -> Response {
    // check basic auth
    let ms = &service.settings().metrics;
    if ms.auth_enabled {
        if let Some(AuthBasic((username, password))) = auth {
            if username != ms.username || password != Some(ms.password.clone()) {
                return (StatusCode::UNAUTHORIZED, "invalid auth").into_response();
            }
        } else {
            return (StatusCode::UNAUTHORIZED, "missing basic auth").into_response();
        }
    }

    // get metrics
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    Response::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, encoder.format_type())
        .body(buffer.into())
        .expect("failed to build metrics response")
}

pub async fn uuids(
    Extension(service): Extension<Arc<Service>>,
    Json(payload): Json<UuidRequest>,
) -> Result<Json<UuidResponse>, XenosError> {
    let usernames = &payload.usernames;
    Ok(Json(service.get_uuids(usernames).await?.into()))
}

pub async fn profile(
    Extension(service): Extension<Arc<Service>>,
    Json(payload): Json<ProfileRequest>,
) -> Result<Json<ProfileResponse>, XenosError> {
    let uuid = Uuid::try_parse(&payload.uuid)?;
    Ok(Json(service.get_profile(&uuid).await?.into()))
}

pub async fn skin(
    Extension(service): Extension<Arc<Service>>,
    Json(payload): Json<SkinRequest>,
) -> Result<Json<SkinResponse>, XenosError> {
    let uuid = Uuid::try_parse(&payload.uuid)?;
    Ok(Json(service.get_skin(&uuid).await?.into()))
}

pub async fn head(
    Extension(service): Extension<Arc<Service>>,
    Json(payload): Json<HeadRequest>,
) -> Result<Json<HeadResponse>, XenosError> {
    let uuid = Uuid::try_parse(&payload.uuid)?;
    let overlay = &payload.overlay;
    Ok(Json(service.get_head(&uuid, overlay).await?.into()))
}
