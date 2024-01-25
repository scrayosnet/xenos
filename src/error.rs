use reqwest::StatusCode;

#[derive(thiserror::Error, Debug)]
pub enum XenosError {
    // binding errors
    #[error("invalid uuid: {0}")]
    InvalidUuid(String),
    // mojang related errors
    #[error("mojang: too many requests")]
    MojangTooManyRequests(),
    #[error("mojang: profile not found")]
    MojangNotFound(),
    #[error("mojang: request failed")]
    MojangError(#[from] reqwest::Error),
    #[error("invalid profile textures: {0}")]
    MojangInvalidProfileTextures(String),
    // cache errors
    #[error("cache retrieve error")]
    CacheRetrieve(#[from] worker::Error),
    #[error("cache error")]
    Cache(#[from] worker::kv::KvError),
}

pub trait IntoResponse {
    fn into_response(self) -> worker::Result<worker::Response>;
}

impl IntoResponse for XenosError {
    fn into_response(self) -> worker::Result<worker::Response> {
        match self {
            XenosError::MojangTooManyRequests() => {
                worker::Response::error("too many requests", StatusCode::TOO_MANY_REQUESTS.as_u16())
            }
            XenosError::MojangNotFound() => {
                worker::Response::error("resource not found", StatusCode::NOT_FOUND.as_u16())
            }
            XenosError::MojangError(inner) => worker::Response::error(
                inner.to_string(),
                inner
                    .status()
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
                    .as_u16(),
            ),
            XenosError::InvalidUuid(str) => worker::Response::error(
                format!("invalid uuid: {}", str.to_string()),
                StatusCode::BAD_REQUEST.as_u16(),
            ),
            XenosError::MojangInvalidProfileTextures(str) => worker::Response::error(
                format!("invalid profile textures: {}", str.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ),
            XenosError::CacheRetrieve(err) => {
                worker::Response::error(err.to_string(), StatusCode::INTERNAL_SERVER_ERROR.as_u16())
            }
            XenosError::Cache(_) => {
                worker::Response::error("cache error", StatusCode::INTERNAL_SERVER_ERROR.as_u16())
            }
        }
    }
}
