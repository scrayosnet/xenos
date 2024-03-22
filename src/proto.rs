//! The proto module [includes](tonic::include_proto!) the rust protobuf definition for both the gRPC
//! and REST services. It also provides implementations for converting into these definitions from
//! internal result formats.

use crate::cache::entry::{CapeData, Dated, Entry, HeadData, ProfileData, SkinData, UuidData};
use std::collections::HashMap;

// includes the rust protobuf definitions
tonic::include_proto!("scrayosnet.xenos");

// conversion utility for converting service results into response data
impl From<HashMap<String, Entry<UuidData>>> for UuidsResponse {
    fn from(value: HashMap<String, Entry<UuidData>>) -> Self {
        UuidsResponse {
            resolved: value
                .into_iter()
                .filter(|(_, v)| v.data.is_some())
                .map(|(k, v)| (k, v.unwrap().into()))
                .collect(),
        }
    }
}

// conversion utility for converting service results into response data
impl From<Dated<UuidData>> for UuidResponse {
    fn from(value: Dated<UuidData>) -> Self {
        UuidResponse {
            timestamp: value.timestamp,
            username: value.data.username,
            uuid: value.data.uuid.hyphenated().to_string(),
        }
    }
}

// conversion utility for converting service results into response data
impl From<Dated<ProfileData>> for ProfileResponse {
    fn from(value: Dated<ProfileData>) -> Self {
        ProfileResponse {
            timestamp: value.timestamp,
            uuid: value.data.id.hyphenated().to_string(),
            name: value.data.name,
            properties: value
                .data
                .properties
                .into_iter()
                .map(|prop| ProfileProperty {
                    name: prop.name,
                    value: prop.value,
                    signature: prop.signature,
                })
                .collect(),
            profile_actions: value.data.profile_actions,
        }
    }
}

// conversion utility for converting service results into response data
impl From<Dated<SkinData>> for SkinResponse {
    fn from(value: Dated<SkinData>) -> Self {
        SkinResponse {
            timestamp: value.timestamp,
            model: value.data.model,
            bytes: value.data.bytes,
            default: value.data.default,
        }
    }
}

// conversion utility for converting service results into response data
impl From<Dated<CapeData>> for CapeResponse {
    fn from(value: Dated<CapeData>) -> Self {
        CapeResponse {
            timestamp: value.timestamp,
            bytes: value.data.bytes,
        }
    }
}

// conversion utility for converting service results into response data
impl From<Dated<HeadData>> for HeadResponse {
    fn from(value: Dated<HeadData>) -> Self {
        HeadResponse {
            timestamp: value.timestamp,
            bytes: value.data.bytes,
            default: value.data.default,
        }
    }
}
