use crate::cache::entry::Cached::{Expired, Hit, Miss};
use crate::mojang::Profile;
use crate::settings;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use uuid::Uuid;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Dated<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    pub timestamp: u64,
    pub data: D,
}

impl<D> Dated<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// Gets the age of the [Dated]. The age of a [Dated] is the relative time from which
    /// the cache entry was created until now.
    pub fn current_age(&self) -> u64 {
        now_seconds() - self.timestamp
    }
}

impl<D> From<D> for Dated<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    fn from(value: D) -> Self {
        Dated {
            timestamp: now_seconds(),
            data: value,
        }
    }
}

pub type Entry<D> = Dated<Option<D>>;

impl<D> Entry<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    pub fn has_some(&self) -> bool {
        self.data.is_some()
    }

    pub fn has_none(&self) -> bool {
        self.data.is_none()
    }

    /// Unwraps the inner data creating a [Dated] without optional data.
    pub fn unwrap(self) -> Dated<D> {
        self.some_or(()).unwrap()
    }

    /// Unwraps the inner data creating a [Dated] without optional data. If the inner data
    /// is [None], it returns the error.
    pub fn some_or<E>(self, err: E) -> Result<Dated<D>, E> {
        match self.data {
            None => Err(err),
            Some(data) => Ok(Dated {
                timestamp: self.timestamp,
                data,
            }),
        }
    }

    pub fn is_expired(&self, expiry: &settings::Expiry) -> bool {
        let exp = match &self.data {
            None => expiry.exp_na,
            Some(_) => expiry.exp,
        };
        now_seconds() - &self.timestamp > exp.as_secs()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Cached<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    Hit(Entry<D>),
    Expired(Entry<D>),
    Miss,
}

impl<D> Cached<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    pub fn with_expiry(opt: Option<Entry<D>>, expiry: &settings::Expiry) -> Cached<D> {
        match opt {
            None => Miss,
            Some(entry) if entry.is_expired(expiry) => Expired(entry),
            Some(entry) => Hit(entry),
        }
    }
}

/// A [UuidData] is a resolved username (case-sensitive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UuidData {
    pub username: String,
    pub uuid: Uuid,
}

/// A [ProfileData] is a [Profile].
pub type ProfileData = Profile;

/// A [SkinData] is a profile skin with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkinData {
    pub bytes: Vec<u8>,
    pub model: String,
    pub default: bool,
}

/// A [CapeData] is a profile cape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapeData {
    pub bytes: Vec<u8>,
}

/// A [HeadData] is a profile skin's head.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeadData {
    pub bytes: Vec<u8>,
    pub default: bool,
}

/// Gets the current time in seconds.
pub fn now_seconds() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap()
}
