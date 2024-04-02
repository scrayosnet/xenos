use crate::error::ServiceError::{NotFound, Unavailable};
use crate::mojang;

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

    /// A [TextureError] wraps a [mojang::TextureError] (e.g. failed to parse textures form profile).
    #[error(transparent)]
    TextureError(#[from] mojang::TextureError),

    /// A [Unavailable] error indicates that a requested resource that was not cached and could not
    /// be retrieved from mojang because of rate limiting or (mojang) fault. It is not clear, if the
    /// requested resource exists or not.
    #[error("unable to request resource from mojang api")]
    Unavailable,

    /// A [NotFound] error indicates that a requested resource does not exist. Either marked in cache
    /// or from a mojang response.
    #[error("resource not found")]
    NotFound,
}

impl From<mojang::ApiError> for ServiceError {
    fn from(value: mojang::ApiError) -> Self {
        match value {
            mojang::ApiError::Unavailable => Unavailable,
            mojang::ApiError::NotFound => NotFound,
        }
    }
}
