use crate::error::ServiceError::{NotFound, Unavailable};
use crate::mojang::ApiError;

/// [ServiceError] is the internal error type for xenos. Other crates might implement conversion traits
/// for these errors.
#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    /// A [UuidError] wraps a [uuid::Error] (e.g. failed to parse string to uuid).
    #[error(transparent)]
    UuidError(#[from] uuid::Error),

    /// A [ImageError] wraps a [image::ImageError] (e.g. failed to parse image from bytes).
    #[error(transparent)]
    ImageError(#[from] image::ImageError),

    /// A [InvalidTextures] error indicates that a mojang profile's textures could not be parsed.
    /// These textures are base64 encoded in a map.
    #[error("invalid profile textures: {0}")]
    InvalidTextures(String),

    /// A [NotRetrieved] error indicates that a requested resource that was not cached and could not
    /// be retrieved from mojang because of rate limiting or (mojang) fault. It is not clear, if the
    /// requested resource exists or not.
    #[error("unable to request resource from mojang api")]
    Unavailable,

    /// A [NotFound] error indicates that a requested resource does not exist. Either marked in cache
    /// or from a mojang response.
    #[error("resource not found")]
    NotFound,
}

impl From<ApiError> for ServiceError {
    fn from(value: ApiError) -> Self {
        match value {
            ApiError::Unavailable => Unavailable,
            ApiError::NotFound => NotFound,
        }
    }
}
