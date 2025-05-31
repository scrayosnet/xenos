use crate::cache::entry::Cached::{Expired, Hit, Miss};
use crate::config;
use crate::mojang::Profile;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::time::SystemTime;
use uuid::Uuid;

/// The time anchor for tests. Allows for `tokio::time::pause()` to be used.
#[cfg(test)]
static TIME_ANCHOR: std::sync::LazyLock<tokio::time::Instant> =
    std::sync::LazyLock::new(|| tokio::time::Instant::now());

/// [Dated] associates some data to its creation time. It provides a measure of relevancy of the
/// data by how up-to-date the data is. In general, the time at which the data is fetched from the
/// mojang api is used as its creation time.
///
/// Use the utility [Dated::from] to created a [Dated] from some data with the current time as its
/// creation time. Use [Dated::current_age] to retrieve its current age.
///
/// ```rs
/// let dated = Dated::from(data);
/// assert!(0, dated.current_age())
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Dated<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// The creation time in seconds.
    pub timestamp: u64,

    /// The expiry offset time factor (maps to -0.5-0.5).
    #[serde(default)]
    pub offset: i8,

    /// The created data.
    pub data: D,
}

impl<D> Dated<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// Gets the current age of the [Dated]. The age of a [Dated] is the relative time from which
    /// the cache entry was created **until now**.
    pub fn current_age(&self) -> u64 {
        now_seconds() - self.timestamp
    }
}

impl<D> From<D> for Dated<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// Creates a new [Dated] from its data, using the current time as its creation time.
    fn from(value: D) -> Self {
        Dated {
            timestamp: now_seconds(),
            offset: generate_offset(),
            data: value,
        }
    }
}

pub fn generate_offset() -> i8 {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        // take the last byte of timestamp as pseudo-random
        Ok(n) => n.as_nanos() as i8,
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}

/// An [Entry] is a [Dated] that contains [optional](Option) data. It is primarily used to indicate
/// whether some resource exists or not.
///
/// An `Entry::from(None)` indicates, that a resource does not exist, while also storing something
/// (that can expire) into the cache.
///
/// ```rs
/// let fetched: Option<...> = fetch();
/// let entry = Entry::from(fetched)
/// cache.set("key", entry);
///
/// let cached: Entry<...> = cache.get("key");
/// ```
pub type Entry<D> = Dated<Option<D>>;

impl<D> Entry<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// Checks whether the [Entry] has some data.
    pub fn has_some(&self) -> bool {
        self.data.is_some()
    }

    /// Checks whether the [Entry] has no data.
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
                offset: self.offset,
                data,
            }),
        }
    }

    /// Checks whether the [Entry] has **now** expired. An [Entry] is expired if its [Entry::current_age]
    /// is **greater or equal** the provided expiry.
    pub fn is_expired(&self, expiry: &config::CacheEntry) -> bool {
        let exp = match &self.data {
            None => expiry.exp_empty,
            Some(_) => expiry.exp,
        };
        let offset = (self.offset as f32) / (i8::MAX as f32);
        let exp_secs = exp.as_secs_f32() + (expiry.offset.as_secs_f32() * offset);
        self.current_age() >= exp_secs.round() as u64
    }
}

/// [Cached] is a wrapper for an [Entry]. It is used by the cache as the primary (get) response type.
/// It differentiates between [Hit], [Expired] and [Miss].
/// - [Hit] is used if a cache entry could be found that is **not expired**.
/// - [Expired] is used if a cache entry could be found that is **expired**.
/// - [Miss] is used if no cache entry could be found.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Cached<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// Some none expired [Entry].
    Hit(Entry<D>),

    /// Some expired [Entry].
    Expired(Entry<D>),

    /// No [Entry].
    Miss,
}

impl<D> Cached<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    /// Creates a new [Cached] from an [Entry] using some expiry. It uses [Entry::is_expired] to decide
    /// whether an [Entry] has expired.
    pub fn with_expiry(opt: Option<Entry<D>>, expiry: &config::CacheEntry) -> Cached<D> {
        match opt {
            None => Miss,
            Some(entry) if entry.is_expired(expiry) => Expired(entry),
            Some(entry) => Hit(entry),
        }
    }
}

impl<D> From<Option<Entry<D>>> for Cached<D>
where
    D: Clone + Debug + Eq + PartialEq,
{
    fn from(opt: Option<Entry<D>>) -> Self {
        match opt {
            None => Miss,
            Some(entry) if entry.timestamp < now_seconds() => Hit(entry),
            Some(entry) => Expired(entry),
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

/// Gets the current time in seconds. When running tests, it uses `tokio::time` with a fixed anchor
/// so that `tokio::time::pause()` can be used.
#[cfg(test)]
pub fn now_seconds() -> u64 {
    TIME_ANCHOR.elapsed().as_secs()
}

/// Gets the current time in seconds.
#[cfg(not(test))]
pub fn now_seconds() -> u64 {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_secs(),
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn call_now_seconds() {
        _ = now_seconds();
        _ = now_seconds();
        _ = now_seconds();
    }

    #[tokio::test]
    async fn check_is_expired_no_offset() {
        tokio::time::pause();

        let entry = Entry::from(Some(()));

        tokio::time::advance(Duration::from_secs(9)).await;

        assert!(!entry.is_expired(&config::CacheEntry {
            exp: Duration::from_secs(10),
            exp_empty: Duration::from_secs(10),
            offset: Duration::from_secs(0),
        }));
    }

    #[tokio::test]
    async fn check_is_expired_below_offset() {
        tokio::time::pause();

        let entry = Entry::from(Some(()));

        tokio::time::advance(Duration::from_secs(7)).await;

        assert!(!entry.is_expired(&config::CacheEntry {
            exp: Duration::from_secs(10),
            exp_empty: Duration::from_secs(10),
            offset: Duration::from_secs(2),
        }));
    }

    #[tokio::test]
    async fn check_is_expired_over_offset() {
        tokio::time::pause();

        let entry = Entry::from(Some(()));

        tokio::time::advance(Duration::from_secs(13)).await;

        assert!(entry.is_expired(&config::CacheEntry {
            exp: Duration::from_secs(10),
            exp_empty: Duration::from_secs(10),
            offset: Duration::from_secs(2),
        }));
    }
}
