use crate::cache::Cached::Miss;
use crate::cache::{Cached, HeadEntry, ProfileEntry, SkinEntry, UuidEntry, XenosCache};
use crate::error::XenosError;
use async_trait::async_trait;
use uuid::Uuid;

#[allow(unused)]
#[derive(Default)]
pub struct Uncached {}

#[async_trait]
impl XenosCache for Uncached {
    async fn get_uuid_by_username(
        &mut self,
        _username: &str,
    ) -> Result<Cached<UuidEntry>, XenosError> {
        Ok(Miss)
    }

    async fn set_uuid_by_username(
        &mut self,
        _username: &str,
        _entry: UuidEntry,
    ) -> Result<(), XenosError> {
        Ok(())
    }

    async fn get_profile_by_uuid(
        &mut self,
        _uuid: &Uuid,
    ) -> Result<Cached<ProfileEntry>, XenosError> {
        Ok(Miss)
    }

    async fn set_profile_by_uuid(
        &mut self,
        _uuid: Uuid,
        _entry: ProfileEntry,
    ) -> Result<(), XenosError> {
        Ok(())
    }

    async fn get_skin_by_uuid(&mut self, _uuid: &Uuid) -> Result<Cached<SkinEntry>, XenosError> {
        Ok(Miss)
    }

    async fn set_skin_by_uuid(&mut self, _uuid: Uuid, _entry: SkinEntry) -> Result<(), XenosError> {
        Ok(())
    }

    async fn get_head_by_uuid(
        &mut self,
        _uuid: &Uuid,
        _overlay: &bool,
    ) -> Result<Cached<HeadEntry>, XenosError> {
        Ok(Miss)
    }

    async fn set_head_by_uuid(
        &mut self,
        _uuid: Uuid,
        _entry: HeadEntry,
        _overlay: &bool,
    ) -> Result<(), XenosError> {
        Ok(())
    }
}
