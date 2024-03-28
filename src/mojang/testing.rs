use crate::error::XenosError;
use crate::mojang::{
    encode_texture_prop, Mojang, Profile, ProfileProperty, Texture, Textures, TexturesProperty,
    UsernameResolved,
};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use std::collections::HashMap;
use uuid::{uuid, Uuid};

lazy_static! {
    /// The mojang profile of Hydrofin.
    pub static ref HYDROFIN: TestingProfile = TestingProfile::new(
        uuid!("09879557e47945a9b434a56377674627"),
        "Hydrofin",
        Some(Bytes::from_static(include_bytes!("../../resources/profiles/hydrofin_skin.png"))),
        None,
    );

    /// The mojang profile of Scrayos.
    pub static ref SCRAYOS: TestingProfile = TestingProfile::new(
        uuid!("9c09eef4f68d4387975172bbff53d5a0"),
        "Scrayos",
        Some(Bytes::from_static(include_bytes!("../../resources/profiles/scrayos_skin.png"))),
        None,
    );

    /// The mojang profile of Herbert. He is oldschool and has no custom skin.
    pub static ref HERBERT: TestingProfile = TestingProfile::new(
        uuid!("1119fff4f68d4388875172bbff53d5a0"),
        "Herbert",
        None,
        None,
    );
}

/// A [TestingProfile] represents a mojang profile to be used for testing Xenos. It is used to fill
/// the [MojangTestingApi] with valid data.
#[derive(Debug)]
pub struct TestingProfile {
    pub profile: Profile,
    pub skin: Option<Bytes>,
    pub cape: Option<Bytes>,
}

impl TestingProfile {
    /// Creates a new valid [TestingProfile] with minimal information.
    pub fn new(id: Uuid, name: &str, skin: Option<Bytes>, cape: Option<Bytes>) -> Self {
        let textures = TexturesProperty {
            timestamp: 0,
            profile_id: id,
            profile_name: name.to_string(),
            signature_required: None,
            textures: Textures {
                skin: skin.is_some().then(|| Texture {
                    url: format!("skin_{}", id.hyphenated()),
                    metadata: None,
                }),
                cape: cape.is_some().then(|| Texture {
                    url: format!("cape_{}", id.hyphenated()),
                    metadata: None,
                }),
            },
        };
        TestingProfile {
            profile: Profile {
                id,
                name: name.to_string(),
                properties: vec![ProfileProperty {
                    name: "textures".to_string(),
                    value: encode_texture_prop(&textures)
                        .expect("expected textures to serializable"),
                    signature: None,
                }],
                profile_actions: vec![],
            },
            skin,
            cape,
        }
    }
}

/// The [MojangTestingApi] is a [mojang api](Mojang) implementation that uses predefined static data
/// instead of actually accessing the mojang api. It is primarily used for in- and external **integration
/// testing**. As such, **it should not be used in production**.
#[derive(Default, Debug)]
pub struct MojangTestingApi<'a> {
    uuids: HashMap<String, UsernameResolved>,
    profiles: HashMap<Uuid, Profile>,
    images: HashMap<String, &'a Bytes>,
}

impl<'a> MojangTestingApi<'a> {
    /// Creates a new empty [MojangTestingApi].
    pub fn new() -> Self {
        MojangTestingApi {
            uuids: Default::default(),
            profiles: Default::default(),
            images: Default::default(),
        }
    }

    /// Creates a new [MojangTestingApi] with default profiles.
    pub fn with_profiles() -> Self {
        Self::new()
            .add_profile(&HYDROFIN)
            .add_profile(&SCRAYOS)
            .add_profile(&HERBERT)
    }

    /// Adds a profile to the [api](MojangTestingApi) using a [TestingProfile]. The profile is expected
    /// to a valid textures property.
    pub fn add_profile(mut self, profile: &'a TestingProfile) -> Self {
        let textures = profile
            .profile
            .get_textures()
            .expect("expected testing profile to provide textures");
        self.uuids.insert(
            profile.profile.name.to_lowercase(),
            UsernameResolved {
                id: profile.profile.id,
                name: profile.profile.name.clone(),
            },
        );
        self.profiles
            .insert(profile.profile.id, profile.profile.clone());
        if let Some(skin) = &profile.skin {
            self.images
                .insert(textures.textures.skin.unwrap().url, skin);
        }
        if let Some(cape) = &profile.cape {
            self.images
                .insert(textures.textures.cape.unwrap().url, cape);
        }
        self
    }
}

#[async_trait]
impl<'a> Mojang for MojangTestingApi<'a> {
    async fn fetch_uuids(&self, usernames: &[String]) -> Result<Vec<UsernameResolved>, XenosError> {
        let uuids = usernames
            .iter()
            .filter_map(|username| self.uuids.get(&username.to_lowercase()))
            .cloned()
            .collect();
        Ok(uuids)
    }

    async fn fetch_profile(&self, uuid: &Uuid, _signed: bool) -> Result<Profile, XenosError> {
        self.profiles.get(uuid).cloned().ok_or(XenosError::NotFound)
    }

    async fn fetch_image_bytes(&self, url: String, _: &str) -> Result<Bytes, XenosError> {
        self.images
            .get(&url)
            .cloned()
            .cloned()
            .ok_or(XenosError::NotFound)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn new_empty() {
        // given
        let api = MojangTestingApi::new();

        // when
        let result = api
            .fetch_uuids(&[
                "Hydrofin".to_string(),
                "scrayos".to_string(),
                "xXSlayer42Xx".to_string(),
            ])
            .await;

        // then
        assert!(result.is_ok());
        assert!(result.is_ok_and(|res| res.is_empty()));
    }

    #[tokio::test]
    async fn with_profiles() {
        // given
        let api = MojangTestingApi::with_profiles();

        // when

        // then
        assert_eq!(3, api.uuids.len());
        assert_eq!(3, api.profiles.len());
        assert_eq!(2, api.images.len());
    }

    #[tokio::test]
    async fn resolve_hydrofin_uuid() {
        // given
        let api = MojangTestingApi::with_profiles();

        // when
        let resolved = api
            .fetch_uuids(&[HYDROFIN.profile.name.to_lowercase()])
            .await;

        // then
        match resolved {
            Ok(resolved) => {
                assert_eq!(1, resolved.len());
                assert_eq!(&HYDROFIN.profile.id, &resolved[0].id);
            }
            Err(_) => panic!("failed to resolve hydrofin uuid"),
        }
    }
}
