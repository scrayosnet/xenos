#[derive(thiserror::Error, Debug)]
pub enum XenosError {
    #[error(transparent)]
    Redis(#[from] redis::RedisError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("mojang responded with no content")]
    NoContent,
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error("invalid profile textures: {0}")]
    InvalidTextures(String),
    #[error("resource not retrieved")]
    NotRetrieved,
    #[error("resource not found")]
    NotFound,
}
