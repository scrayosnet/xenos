//! The proto module [includes](tonic::include_proto!) the rust protobuf definition for both the gRPC
//! and REST services. It also provides implementations for converting into these definitions from
//! internal result formats.

use crate::cache::{CapeEntry, HeadEntry, ProfileEntry, SkinEntry, UuidEntry};
use std::collections::HashMap;

// includes the rust protobuf definitions
tonic::include_proto!("scrayosnet.xenos");

// conversion utility for converting service results into response data
impl From<HashMap<String, UuidEntry>> for UuidsResponse {
    fn from(value: HashMap<String, UuidEntry>) -> Self {
        UuidsResponse {
            resolved: value
                .into_iter()
                .filter(|(_, v)| v.data.is_some())
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

// conversion utility for converting service results into response data
impl From<UuidEntry> for UuidResponse {
    fn from(value: UuidEntry) -> Self {
        match value.data {
            None => UuidResponse {
                timestamp: value.timestamp,
                username: "".to_string(),
                uuid: "".to_string(),
            },
            Some(data) => UuidResponse {
                timestamp: value.timestamp,
                username: data.username,
                uuid: data.uuid.hyphenated().to_string(),
            },
        }
    }
}

// conversion utility for converting service results into response data
impl From<ProfileEntry> for ProfileResponse {
    fn from(value: ProfileEntry) -> Self {
        match value.data {
            None => ProfileResponse {
                timestamp: value.timestamp,
                uuid: "".to_string(),
                name: "".to_string(),
                properties: vec![],
                profile_actions: vec![],
            },
            Some(data) => ProfileResponse {
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
            },
        }
    }
}

// conversion utility for converting service results into response data
impl From<SkinEntry> for SkinResponse {
    fn from(value: SkinEntry) -> Self {
        match value.data {
            None => SkinResponse {
                timestamp: value.timestamp,
                model: "".to_string(),
                bytes: vec![],
                default: false,
            },
            Some(data) => SkinResponse {
                timestamp: value.timestamp,
                model: data.model,
                bytes: data.bytes,
                default: data.default,
            },
        }
    }
}

// conversion utility for converting service results into response data
impl From<CapeEntry> for CapeResponse {
    fn from(value: CapeEntry) -> Self {
        match value.data {
            None => CapeResponse {
                timestamp: value.timestamp,
                bytes: vec![],
            },
            Some(data) => CapeResponse {
                timestamp: value.timestamp,
                bytes: data.bytes,
            },
        }
    }
}

// conversion utility for converting service results into response data
impl From<HeadEntry> for HeadResponse {
    fn from(value: HeadEntry) -> Self {
        match value.data {
            None => HeadResponse {
                timestamp: value.timestamp,
                bytes: vec![],
                default: false,
            },
            Some(data) => HeadResponse {
                timestamp: value.timestamp,
                bytes: data.bytes,
                default: data.default,
            },
        }
    }
}
