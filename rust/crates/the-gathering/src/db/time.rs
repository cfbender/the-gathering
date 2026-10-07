//! Timestamps and dates as the database stores them.
//!
//! Rows written by the server hold `2026-10-06T21:21:40Z`; rows inserted by raw SQL
//! migrations use SQLite's `CURRENT_TIMESTAMP` (`2026-10-06 21:21:40`). Both decode;
//! encoding always uses the first form, which every existing row also uses.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::sqlite::{Sqlite, SqliteArgumentsBuffer, SqliteTypeInfo, SqliteValueRef};
use sqlx::{Decode, Encode, Type};
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime, PrimitiveDateTime, UtcOffset};

const DB_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
const DATE_FORMAT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

/// A UTC instant truncated to whole seconds (`:utc_datetime`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcDateTime(OffsetDateTime);

impl UtcDateTime {
    /// The current time, truncated to seconds like `DateTime.utc_now() |> DateTime.truncate(:second)`.
    pub fn now() -> Self {
        Self::from_offset(OffsetDateTime::now_utc())
    }

    /// Converts and truncates to seconds.
    pub fn from_offset(value: OffsetDateTime) -> Self {
        let utc = value.to_offset(UtcOffset::UTC);
        Self(utc.replace_nanosecond(0).unwrap_or(utc))
    }

    /// From Unix seconds.
    pub fn from_unix(seconds: i64) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp(seconds).ok().map(Self)
    }

    /// The underlying instant.
    pub fn inner(self) -> OffsetDateTime {
        self.0
    }

    /// Unix seconds.
    pub fn unix(self) -> i64 {
        self.0.unix_timestamp()
    }

    /// Adds a (possibly negative) duration.
    #[must_use]
    pub fn plus(self, duration: Duration) -> Self {
        self.0.checked_add(duration).map_or(self, Self)
    }

    /// The UTC calendar date.
    pub fn date(self) -> Date {
        self.0.date()
    }

    /// Parses every timestamp form the database holds: `2026-10-06T21:21:40Z`,
    /// with an offset, without a zone (taken as UTC), with a space separator, with fractional
    /// seconds, or without seconds (HTML `datetime-local`).
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Ok(parsed) =
            OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        {
            return Some(Self::from_offset(parsed));
        }
        let normalized = value.replacen(' ', "T", 1);
        let naive = normalized.strip_suffix('Z').unwrap_or(&normalized);
        let formats: [&[FormatItem<'static>]; 3] = [
            format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]"),
            format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]"),
            format_description!("[year]-[month]-[day]T[hour]:[minute]"),
        ];
        formats
            .iter()
            .find_map(|format| PrimitiveDateTime::parse(naive, format).ok())
            .map(|primitive| Self::from_offset(primitive.assume_utc()))
            .or_else(|| {
                // Offsets without seconds or a colon, which RFC 3339 rejects but ISO 8601 allows.
                OffsetDateTime::parse(
                    value,
                    &time::format_description::well_known::Iso8601::DEFAULT,
                )
                .ok()
                .map(Self::from_offset)
            })
    }

    /// `2026-10-06T21:21:40Z`.
    pub fn to_db_string(self) -> String {
        self.0.format(DB_FORMAT).unwrap_or_default()
    }
}

impl Default for UtcDateTime {
    /// The Unix epoch (a placeholder for structs built in memory).
    fn default() -> Self {
        Self(OffsetDateTime::UNIX_EPOCH)
    }
}

impl fmt::Display for UtcDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_db_string())
    }
}

impl From<OffsetDateTime> for UtcDateTime {
    fn from(value: OffsetDateTime) -> Self {
        Self::from_offset(value)
    }
}

impl Serialize for UtcDateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_db_string())
    }
}

impl<'de> Deserialize<'de> for UtcDateTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("invalid datetime"))
    }
}

impl Type<Sqlite> for UtcDateTime {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }

    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <String as Type<Sqlite>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Sqlite> for UtcDateTime {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, BoxDynError> {
        let text = <&str as Decode<Sqlite>>::decode(value)?;
        Self::parse(text).ok_or_else(|| format!("invalid utc_datetime {text:?}").into())
    }
}

impl Encode<'_, Sqlite> for UtcDateTime {
    fn encode_by_ref(&self, buf: &mut SqliteArgumentsBuffer) -> Result<IsNull, BoxDynError> {
        <String as Encode<Sqlite>>::encode(self.to_db_string(), buf)
    }
}

/// A calendar date (`:date`), stored and rendered as `2026-10-06`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IsoDate(pub Date);

impl IsoDate {
    /// Parses `YYYY-MM-DD`.
    pub fn parse(value: &str) -> Option<Self> {
        Date::parse(value.trim(), DATE_FORMAT).ok().map(Self)
    }
}

impl fmt::Display for IsoDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.format(DATE_FORMAT).unwrap_or_default())
    }
}

impl Serialize for IsoDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for IsoDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("invalid date"))
    }
}

impl Type<Sqlite> for IsoDate {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }

    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <String as Type<Sqlite>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Sqlite> for IsoDate {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, BoxDynError> {
        let text = <&str as Decode<Sqlite>>::decode(value)?;
        let date_part = text.get(..10).unwrap_or(text);
        Self::parse(date_part).ok_or_else(|| format!("invalid date {text:?}").into())
    }
}

impl Encode<'_, Sqlite> for IsoDate {
    fn encode_by_ref(&self, buf: &mut SqliteArgumentsBuffer) -> Result<IsNull, BoxDynError> {
        <String as Encode<Sqlite>>::encode(self.to_string(), buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_stored_and_submitted_form() {
        let expected = "2026-10-06T21:21:40Z";
        for input in [
            "2026-10-06T21:21:40Z",
            "2026-10-06 21:21:40",
            "2026-10-06T21:21:40",
            "2026-10-06T21:21:40.123456Z",
            "2026-10-06T17:21:40-04:00",
        ] {
            assert_eq!(
                UtcDateTime::parse(input).unwrap().to_string(),
                expected,
                "{input}"
            );
        }
        assert_eq!(
            UtcDateTime::parse("2026-10-06T21:21").unwrap().to_string(),
            "2026-10-06T21:21:00Z"
        );
        assert!(UtcDateTime::parse("yesterday").is_none());
    }
}
