use crate::cache::{HeadEntry, ProfileEntry, SkinEntry, UuidEntry};

tonic::include_proto!("scrayosnet.xenos");

impl From<ProfileEntry> for ProfileResponse {
    fn from(value: ProfileEntry) -> Self {
        ProfileResponse {
            timestamp: value.timestamp,
            uuid: value.uuid.hyphenated().to_string(),
            name: value.name,
            properties: value
                .properties
                .into_iter()
                .map(|prop| ProfileProperty {
                    name: prop.name,
                    value: prop.value,
                    signature: prop.signature,
                })
                .collect(),
            profile_actions: value.profile_actions,
        }
    }
}

impl From<SkinEntry> for SkinResponse {
    fn from(value: SkinEntry) -> Self {
        SkinResponse {
            timestamp: value.timestamp,
            data: value.bytes,
        }
    }
}

impl From<HeadEntry> for HeadResponse {
    fn from(value: HeadEntry) -> Self {
        HeadResponse {
            timestamp: value.timestamp,
            data: value.bytes,
        }
    }
}

impl From<UuidEntry> for UuidResult {
    fn from(value: UuidEntry) -> Self {
        UuidResult {
            timestamp: value.timestamp,
            username: value.username,
            uuid: value.uuid.hyphenated().to_string(),
        }
    }
}

impl From<Vec<UuidEntry>> for UuidResponse {
    fn from(value: Vec<UuidEntry>) -> Self {
        UuidResponse {
            resolved: value.into_iter().map(|v| v.into()).collect(),
        }
    }
}
