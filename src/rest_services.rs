use crate::cache::level::CacheLevel;
use crate::error::ServiceError;
use crate::metrics::{REGISTRY, REQUEST, RequestsLabels};
use crate::mojang::Mojang;
use crate::proto::{
    CapeRequest, CapeResponse, HeadRequest, HeadResponse, ProfileRequest, ProfileResponse,
    SkinRequest, SkinResponse, UuidRequest, UuidResponse, UuidsRequest, UuidsResponse,
};
use crate::service::Service;
use axum::{
    Extension, Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_extra::TypedHeader;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::Basic;
use prometheus_client::encoding::text::encode;
use std::sync::Arc;
use uuid::Uuid;

/// [RestResult] is an alias for a rest [Json] result with [ServiceError]
type RestResult<T> = Result<Json<T>, ServiceError>;

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        match self {
            ServiceError::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "unable to request resource from mojang api",
            )
                .into_response(),
            ServiceError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
        }
    }
}

/// An [axum] handler for providing [prometheus] metrics. If enabled by the service, it validates
/// basic auth.
pub async fn metrics<L, R, M>(
    auth: Option<TypedHeader<Authorization<Basic>>>,
    Extension(service): Extension<Arc<Service<L, R, M>>>,
) -> Response
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    // check basic auth
    let ms = &service.config().metrics;
    if ms.auth_enabled {
        if let Some(TypedHeader(Authorization(creds))) = auth {
            if creds.username() != ms.username || creds.password() != ms.password {
                return (StatusCode::UNAUTHORIZED, "invalid auth").into_response();
            }
        } else {
            return (StatusCode::UNAUTHORIZED, "missing basic auth").into_response();
        }
    }

    // get metrics
    let mut buf = String::new();
    encode(&mut buf, &REGISTRY).expect("failed to encode metrics");
    Response::builder()
        .header(
            hyper::header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )
        .body(buf.into())
        .expect("failed to build response")
}

/// An [axum] handler for [UuidRequest] rest gateway.
pub async fn uuid<L, R, M>(
    Extension(service): Extension<Arc<Service<L, R, M>>>,
    Json(payload): Json<UuidRequest>,
) -> RestResult<UuidResponse>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    REQUEST
        .get_or_create(&RequestsLabels {
            request_type: "uuid",
            handler: "rest",
        })
        .inc();
    let username = &payload.username;
    Ok(Json(service.get_uuid(username).await?.into()))
}

/// An [axum] handler for [UuidsRequest] rest gateway.
pub async fn uuids<L, R, M>(
    Extension(service): Extension<Arc<Service<L, R, M>>>,
    Json(payload): Json<UuidsRequest>,
) -> RestResult<UuidsResponse>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    REQUEST
        .get_or_create(&RequestsLabels {
            request_type: "uuids",
            handler: "rest",
        })
        .inc();
    let usernames = &payload.usernames;
    Ok(Json(service.get_uuids(usernames).await?.into()))
}

/// An [axum] handler for [ProfileRequest] rest gateway.
pub async fn profile<L, R, M>(
    Extension(service): Extension<Arc<Service<L, R, M>>>,
    Json(payload): Json<ProfileRequest>,
) -> RestResult<ProfileResponse>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    REQUEST
        .get_or_create(&RequestsLabels {
            request_type: "profile",
            handler: "rest",
        })
        .inc();
    let uuid = Uuid::try_parse(&payload.uuid)?;
    Ok(Json(service.get_profile(&uuid).await?.into()))
}

/// An [axum] handler for [SkinRequest] rest gateway.
pub async fn skin<L, R, M>(
    Extension(service): Extension<Arc<Service<L, R, M>>>,
    Json(payload): Json<SkinRequest>,
) -> RestResult<SkinResponse>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    REQUEST
        .get_or_create(&RequestsLabels {
            request_type: "skin",
            handler: "rest",
        })
        .inc();
    let uuid = Uuid::try_parse(&payload.uuid)?;
    Ok(Json(service.get_skin(&uuid).await?.into()))
}

/// An [axum] handler for [CapeRequest] rest gateway.
pub async fn cape<L, R, M>(
    Extension(service): Extension<Arc<Service<L, R, M>>>,
    Json(payload): Json<CapeRequest>,
) -> RestResult<CapeResponse>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    REQUEST
        .get_or_create(&RequestsLabels {
            request_type: "cape",
            handler: "rest",
        })
        .inc();
    let uuid = Uuid::try_parse(&payload.uuid)?;
    Ok(Json(service.get_cape(&uuid).await?.into()))
}

/// An [axum] handler for [HeadRequest] rest gateway.
pub async fn head<L, R, M>(
    Extension(service): Extension<Arc<Service<L, R, M>>>,
    Json(payload): Json<HeadRequest>,
) -> RestResult<HeadResponse>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    REQUEST
        .get_or_create(&RequestsLabels {
            request_type: "head",
            handler: "rest",
        })
        .inc();
    let uuid = Uuid::try_parse(&payload.uuid)?;
    let overlay = payload.overlay;
    Ok(Json(service.get_head(&uuid, overlay).await?.into()))
}
