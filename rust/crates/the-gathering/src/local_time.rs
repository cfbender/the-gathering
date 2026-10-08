//! Converting between stored UTC timestamps and a viewer's IANA time zone, so date, weekday, and hour filters match the local calendar
//! the browser shows. Zones come from jiff's bundled tzdb.

use jiff::Timestamp;
use jiff::civil;
use jiff::tz::TimeZone;
use time::Date;

use crate::db::UtcDateTime;

/// The zone name used when none (or an unknown one) is given.
pub const UTC: &str = "Etc/UTC";

/// A resolved IANA zone.
#[derive(Clone, Debug)]
pub struct Zone {
    name: String,
    tz: TimeZone,
}

impl Zone {
    /// UTC.
    pub fn utc() -> Self {
        Self {
            name: UTC.to_owned(),
            tz: TimeZone::UTC,
        }
    }

    /// `name` when it names a known IANA zone, otherwise UTC.
    ///
    /// jiff's lookup is case-insensitive, but a name only counts as known when it is spelled
    /// exactly as the database spells it.
    pub fn parse(name: Option<&str>) -> Self {
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            return Self::utc();
        };
        match TimeZone::get(name) {
            Ok(tz) if tz.iana_name() == Some(name) => Self {
                name: name.to_owned(),
                tz,
            },
            _ => Self::utc(),
        }
    }

    /// The zone's name (`Etc/UTC` for the fallback).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The UTC instant at which `date` begins in this zone. When
    /// DST skips local midnight the day starts at the first valid moment after the gap.
    pub fn start_of_day(&self, date: Date) -> Option<UtcDateTime> {
        let civil = civil::Date::new(
            i16::try_from(date.year()).ok()?,
            i8::try_from(u8::from(date.month())).ok()?,
            i8::try_from(date.day()).ok()?,
        )
        .ok()?;
        let zoned = civil.to_zoned(self.tz.clone()).ok()?;
        UtcDateTime::from_unix(zoned.timestamp().as_second())
    }

    /// The local wall clock of a UTC instant: Sunday-first weekday (0 = Sunday … 6 = Saturday,
    /// like JavaScript's `Date#getDay`) and hour.
    pub fn weekday_and_hour(&self, at: UtcDateTime) -> Option<(i64, i64)> {
        let zoned = Timestamp::from_second(at.unix())
            .ok()?
            .to_zoned(self.tz.clone());
        Some((
            i64::from(zoned.weekday().to_sunday_zero_offset()),
            i64::from(zoned.hour()),
        ))
    }
}

/// `Date.from_iso8601/1` for plain `YYYY-MM-DD` dates (no trimming).
pub fn parse_date(value: &str) -> Option<Date> {
    if value.len() != 10 || value.trim() != value {
        return None;
    }
    crate::db::IsoDate::parse(value).map(|date| date.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_zones_and_local_days() {
        assert_eq!(
            Zone::parse(Some("America/New_York")).name(),
            "America/New_York"
        );
        assert_eq!(Zone::parse(Some("america/new_york")).name(), UTC);
        assert_eq!(Zone::parse(Some("Not/AZone")).name(), UTC);
        assert_eq!(Zone::parse(None).name(), UTC);
        let ny = Zone::parse(Some("America/New_York"));
        let date = parse_date("2026-09-24").unwrap();
        assert_eq!(
            ny.start_of_day(date).unwrap().to_string(),
            "2026-09-24T04:00:00Z"
        );
        let evening = UtcDateTime::parse("2026-09-25T01:30:00Z").unwrap();
        assert_eq!(ny.weekday_and_hour(evening), Some((4, 21)));
        // Midnight skipped by DST (Santiago, 2026-09-06) starts at 01:00 local.
        let santiago = Zone::parse(Some("America/Santiago"));
        let gap = santiago
            .start_of_day(parse_date("2026-09-06").unwrap())
            .unwrap();
        assert_eq!(gap.to_string(), "2026-09-06T04:00:00Z");
        assert!(parse_date("2026-13-01").is_none());
        assert!(parse_date(" 2026-01-01").is_none());
    }
}
