use crate::error::XenosError;
use crate::error::XenosError::{NotFound, NotRetrieved, UuidParse};
use crate::proto::{
    profile_server::Profile, HeadRequest, HeadResponse, ProfileRequest, ProfileResponse,
    SkinRequest, SkinResponse, UuidRequest, UuidResponse,
};
use crate::service::Service;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

type GrpcResult<T> = Result<Response<T>, Status>;

impl From<XenosError> for Status {
    fn from(value: XenosError) -> Self {
        match value {
            UuidParse(_) => Status::invalid_argument("invalid uuid"),
            NotRetrieved => Status::unavailable("unable to retrieve"),
            NotFound => Status::not_found("resource not found"),
            err => Status::internal(err.to_string()),
        }
    }
}

pub struct GrpcProfileService {
    pub service: Arc<Service>,
}

#[tonic::async_trait]
impl Profile for GrpcProfileService {
    async fn get_uuids(&self, request: Request<UuidRequest>) -> GrpcResult<UuidResponse> {
        let usernames = request.into_inner().usernames;
        let uuids = self.service.get_uuids(&usernames).await?;
        Ok(Response::new(uuids.into()))
    }

    async fn get_profile(
        &self,
        request: Request<ProfileRequest>,
    ) -> Result<Response<ProfileResponse>, Status> {
        let uuid = Uuid::try_parse(&request.into_inner().uuid).map_err(UuidParse)?;
        let profile = self.service.get_profile(&uuid).await?;
        Ok(Response::new(profile.into()))
    }

    async fn get_skin(
        &self,
        request: Request<SkinRequest>,
    ) -> Result<Response<SkinResponse>, Status> {
        let uuid = Uuid::try_parse(&request.into_inner().uuid).map_err(UuidParse)?;
        let skin = self.service.get_skin(&uuid).await?;
        Ok(Response::new(skin.into()))
    }

    async fn get_head(
        &self,
        request: Request<HeadRequest>,
    ) -> Result<Response<HeadResponse>, Status> {
        let req = request.into_inner();
        let overlay = &req.overlay;
        let uuid = Uuid::try_parse(&req.uuid).map_err(UuidParse)?;
        let head = self.service.get_head(&uuid, overlay).await?;
        Ok(Response::new(head.into()))
    }
}
