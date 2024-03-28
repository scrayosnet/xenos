use serde::de::{Error, Unexpected, Visitor};
use serde::Deserializer;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use tracing::level_filters::LevelFilter;

/// Deserializer for [LevelFilter] from string. E.g. `info`.
pub fn parse_level_filter<'de, D>(deserializer: D) -> Result<LevelFilter, D::Error>
where
    D: Deserializer<'de>,
{
    struct LevelFilterVisitor;

    impl<'de> Visitor<'de> for LevelFilterVisitor {
        type Value = LevelFilter;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a log level name or number")
        }

        fn visit_str<E>(self, value: &str) -> Result<LevelFilter, E>
        where
            E: Error,
        {
            match LevelFilter::from_str(value) {
                Ok(filter) => Ok(filter),
                Err(_) => Err(Error::invalid_value(
                    Unexpected::Str(value),
                    &"log level string or number",
                )),
            }
        }
    }

    deserializer.deserialize_str(LevelFilterVisitor)
}

/// Deserializer that parses an [iso8601] duration string or number of seconds to a [Duration].
/// E.g. `PT1M` or `60` is a duration of one minute.
pub fn parse_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    struct DurationVisitor;

    impl<'de> Visitor<'de> for DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "an iso duration or number of seconds")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            match u64::try_from(v) {
                Ok(u) => self.visit_u64(u),
                Err(_) => Err(Error::invalid_type(
                    Unexpected::Signed(v),
                    &"a positive number of seconds",
                )),
            }
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Duration::from_secs(v))
        }

        fn visit_str<E>(self, value: &str) -> Result<Duration, E>
        where
            E: Error,
        {
            match iso8601::Duration::from_str(value) {
                Ok(iso) => Ok(Duration::from(iso)),
                Err(_) => Err(Error::invalid_value(
                    Unexpected::Str(value),
                    &"an iso duration",
                )),
            }
        }
    }

    deserializer.deserialize_any(DurationVisitor)
}
