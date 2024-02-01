use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use uuid::Uuid;
use xenos::error::XenosError;
use xenos::mojang::{MojangApi, Profile, UsernameResolved};

#[derive(Default)]
struct StubMojang {
    uuids: HashMap<String, UsernameResolved>,
    profiles: HashMap<Uuid, Profile>,
    images: HashMap<String, Bytes>,
}

#[async_trait]
impl MojangApi for StubMojang {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError> {
        let uuids = usernames
            .iter()
            .filter_map(|username| self.uuids.get(&username.to_lowercase()))
            .cloned()
            .collect();
        Ok(uuids)
    }

    async fn fetch_profile(&self, uuid: &Uuid) -> Result<Profile, XenosError> {
        self.profiles.get(uuid).cloned().ok_or(XenosError::NotFound)
    }

    async fn fetch_image_bytes(&self, url: String, _: &str) -> Result<Bytes, XenosError> {
        self.images.get(&url).cloned().ok_or(XenosError::NotFound)
    }
}

#[tokio::test]
async fn stub_mojang_uuids() {
    // given
    let mojang = StubMojang {
        uuids: HashMap::from([
            (
                "hydrofin".to_string(),
                UsernameResolved {
                    id: Uuid::new_v4(),
                    name: "Hydrofin".to_string(),
                },
            ),
            (
                "scrayos".to_string(),
                UsernameResolved {
                    id: Uuid::new_v4(),
                    name: "Scrayos".to_string(),
                },
            ),
        ]),
        profiles: Default::default(),
        images: Default::default(),
    };
    let usernames = [
        "Hydrofin".to_string(),
        "scrayos".to_string(),
        "herbert".to_string(),
    ];

    // when
    let retrieved = mojang.fetch_uuids(&usernames).await.unwrap();

    // then
    assert_eq!(2, retrieved.len())
}
