#[derive(thiserror::Error, Debug)]
pub enum XenosError {
    #[error(transparent)]
    Redis(#[from] redis::RedisError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error("invalid profile textures: {0}")]
    InvalidTextures(String),
    #[error("resource not retrieved")]
    NotRetrieved,
    #[error("resource not found")]
    NotFound,
}
