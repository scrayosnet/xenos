use crate::cache::level::CacheLevel;
use crate::error::ServiceError;
use crate::error::ServiceError::{NotFound, Unavailable, UuidError};
use crate::mojang::Mojang;
use crate::proto::{
    profile_server::Profile, CapeRequest, CapeResponse, HeadRequest, HeadResponse, ProfileRequest,
    ProfileResponse, SkinRequest, SkinResponse, UuidRequest, UuidResponse, UuidsRequest,
    UuidsResponse,
};
use crate::service::Service;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

/// [GrpcResult] is an alias for grpc result [Response] and [Status].
type GrpcResult<T> = Result<Response<T>, Status>;

// utility that allows the usage of ServiceError in result with auto conversion to (tonic) response status
impl From<ServiceError> for Status {
    fn from(value: ServiceError) -> Self {
        match value {
            UuidError(_) => Status::invalid_argument("invalid uuid"),
            Unavailable => Status::unavailable("unable to request resource from mojang api"),
            NotFound => Status::not_found("resource not found"),
            err => Status::internal(err.to_string()),
        }
    }
}

/// A [GrpcProfileService] wraps [Service] and implements the grpc [Profile] service.
pub struct GrpcProfileService<L, R, M>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    service: Arc<Service<L, R, M>>,
}

impl<L, R, M> GrpcProfileService<L, R, M>
where
    L: CacheLevel,
    R: CacheLevel,
    M: Mojang,
{
    /// Creates a new [GrpcProfileService] wrapping the provided [Service] reference.
    pub fn new(service: Arc<Service<L, R, M>>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl<L, R, M> Profile for GrpcProfileService<L, R, M>
where
    L: CacheLevel + Sync + 'static,
    R: CacheLevel + Sync + 'static,
    M: Mojang + Sync + 'static,
{
    async fn get_uuid(&self, request: Request<UuidRequest>) -> GrpcResult<UuidResponse> {
        let username = request.into_inner().username;
        let uuid = self.service.get_uuid(&username).await?;
        Ok(Response::new(uuid.into()))
    }

    async fn get_uuids(&self, request: Request<UuidsRequest>) -> GrpcResult<UuidsResponse> {
        let usernames = request.into_inner().usernames;
        let uuids = self.service.get_uuids(&usernames).await?;
        Ok(Response::new(uuids.into()))
    }

    async fn get_profile(&self, request: Request<ProfileRequest>) -> GrpcResult<ProfileResponse> {
        let uuid = Uuid::try_parse(&request.into_inner().uuid).map_err(UuidError)?;
        let profile = self.service.get_profile(&uuid).await?;
        Ok(Response::new(profile.into()))
    }

    async fn get_skin(&self, request: Request<SkinRequest>) -> GrpcResult<SkinResponse> {
        let req = request.into_inner();
        let uuid = Uuid::try_parse(&req.uuid).map_err(UuidError)?;
        let skin = self.service.get_skin(&uuid).await?;
        Ok(Response::new(skin.into()))
    }

    async fn get_cape(&self, request: Request<CapeRequest>) -> GrpcResult<CapeResponse> {
        let uuid = Uuid::try_parse(&request.into_inner().uuid).map_err(UuidError)?;
        let cape = self.service.get_cape(&uuid).await?;
        Ok(Response::new(cape.into()))
    }

    async fn get_head(&self, request: Request<HeadRequest>) -> GrpcResult<HeadResponse> {
        let req = request.into_inner();
        let overlay = req.overlay;
        let uuid = Uuid::try_parse(&req.uuid).map_err(UuidError)?;
        let head = self.service.get_head(&uuid, overlay).await?;
        Ok(Response::new(head.into()))
    }
}
