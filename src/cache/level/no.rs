use crate::cache::entry::{CapeData, Entry, HeadData, ProfileData, SkinData, UuidData};
use crate::cache::level::CacheLevel;
use async_trait::async_trait;
use uuid::Uuid;

/// [No Cache](NoCache) is a [CacheLevel] implementation that does nothing. It can be used to disable
/// an otherwise mandatory [CacheLevel].
#[derive(Debug)]
pub struct NoCache;

impl NoCache {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl CacheLevel for NoCache {
    async fn get_uuid(&self, _: &str) -> Option<Entry<UuidData>> {
        None
    }

    async fn set_uuid(&self, _: &str, _: Entry<UuidData>) {}

    async fn get_profile(&self, _: &Uuid) -> Option<Entry<ProfileData>> {
        None
    }

    async fn set_profile(&self, _: &Uuid, _: Entry<ProfileData>) {}

    async fn get_skin(&self, _: &Uuid) -> Option<Entry<SkinData>> {
        None
    }

    async fn set_skin(&self, _: &Uuid, _: Entry<SkinData>) {}

    async fn get_cape(&self, _: &Uuid) -> Option<Entry<CapeData>> {
        None
    }

    async fn set_cape(&self, _: &Uuid, _: Entry<CapeData>) {}

    async fn get_head(&self, _: &(Uuid, bool)) -> Option<Entry<HeadData>> {
        None
    }

    async fn set_head(&self, _: &(Uuid, bool), _: Entry<HeadData>) {}
}
