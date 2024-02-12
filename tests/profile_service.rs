mod common;

use crate::common::ServiceBuilder;
use uuid::Uuid;

/// Tests that uuids for multiple usernames can be found successfully.
#[tokio::test]
async fn get_uuids_found() {
    // given
    let uuid = Uuid::new_v4();
    let service = ServiceBuilder::new()
        .with_username("Hydrofin", uuid)
        .with_username("Scrayos", Uuid::new_v4())
        .build();

    // when
    let result = service
        .get_uuids(&["Hydrofin".to_string(), "scrayos".to_string()])
        .await
        .unwrap();

    // then
    assert_eq!(2, result.len());
    assert_eq!(
        "Hydrofin",
        result["hydrofin"].data.as_ref().unwrap().username
    );
    assert_eq!(uuid, result["hydrofin"].data.as_ref().unwrap().uuid);
    assert_eq!("Scrayos", result["scrayos"].data.as_ref().unwrap().username);
}
