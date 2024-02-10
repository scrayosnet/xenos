use tokio::sync::Mutex;
use tonic::{Code, Request};
use xenos::cache::uncached::Uncached;
use xenos::mojang::stub::StubMojang;
use xenos::profile_service::pb::profile_server::Profile;
use xenos::profile_service::pb::ProfileRequest;
use xenos::profile_service::ProfileService;

#[tokio::test]
async fn get_profile_not_found() {
    // given
    let mojang = StubMojang {
        uuids: Default::default(),
        profiles: Default::default(),
        images: Default::default(),
    };
    let uncached = Uncached::default();
    let service = ProfileService {
        cache: Box::new(Mutex::new(uncached)),
        mojang: Box::new(mojang),
    };
    let request: Request<ProfileRequest> = Request::<ProfileRequest>::new(ProfileRequest {
        uuid: uuid::Uuid::new_v4().to_string(),
    });

    // when
    let response = service.get_profile(request).await;

    // then
    assert!(response.is_err());
    assert_eq!(Code::NotFound, response.expect_err("").code());
}
