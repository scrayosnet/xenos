mod common;

use crate::common::ServiceBuilder;
use uuid::Uuid;

/// Tests that uuids for multiple usernames can be found successfully.
#[tokio::test]
async fn get_uuids_found() {
    // given
    let uuid = Uuid::new_v4();
    let service = ServiceBuilder::default()
        .with_username("Hydrofin", uuid)
        .with_username("Scrayos", Uuid::new_v4())
        .build();

    // when
    let mut result = service
        .get_uuids(&["Hydrofin".to_string(), "scrayos".to_string()])
        .await
        .unwrap();
    result.sort_by_key(|e| e.username.clone());

    // then
    assert_eq!(2, result.len());
    assert_eq!("Hydrofin", result[0].username);
    assert_eq!(uuid, result[0].uuid);
    assert_eq!("Scrayos", result[1].username);
}
