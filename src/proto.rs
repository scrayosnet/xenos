use crate::cache::{HeadEntry, ProfileEntry, SkinEntry, UuidEntry};
use std::collections::HashMap;

tonic::include_proto!("scrayosnet.xenos");

impl From<HashMap<String, UuidEntry>> for UuidResponse {
    fn from(value: HashMap<String, UuidEntry>) -> Self {
        UuidResponse {
            resolved: value.into_iter().map(|(k, v)| (k, v.into())).collect(),
        }
    }
}

impl From<UuidEntry> for UuidResult {
    fn from(value: UuidEntry) -> Self {
        UuidResult {
            timestamp: value.timestamp,
            data: value.data.map(|data| UuidData {
                username: data.username,
                uuid: data.uuid.hyphenated().to_string(),
            }),
        }
    }
}

impl From<ProfileEntry> for ProfileResponse {
    fn from(value: ProfileEntry) -> Self {
        if let Some(data) = value.data {
            return ProfileResponse {
                timestamp: value.timestamp,
                uuid: data.id.hyphenated().to_string(),
                name: data.name,
                properties: data
                    .properties
                    .into_iter()
                    .map(|prop| ProfileProperty {
                        name: prop.name,
                        value: prop.value,
                        signature: prop.signature,
                    })
                    .collect(),
                profile_actions: data.profile_actions,
            };
        }
        ProfileResponse {
            timestamp: value.timestamp,
            uuid: "".to_string(),
            name: "".to_string(),
            properties: vec![],
            profile_actions: vec![],
        }
    }
}

impl From<SkinEntry> for SkinResponse {
    fn from(value: SkinEntry) -> Self {
        SkinResponse {
            timestamp: value.timestamp,
            bytes: value.data.unwrap_or_default(),
        }
    }
}

impl From<HeadEntry> for HeadResponse {
    fn from(value: HeadEntry) -> Self {
        HeadResponse {
            timestamp: value.timestamp,
            bytes: value.data.unwrap_or_default(),
        }
    }
}
