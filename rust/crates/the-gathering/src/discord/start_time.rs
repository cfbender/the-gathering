//! Explicit, time-zone-aware start times for `/newgame` (`StartTime`); `None` means
//! "start when filled".

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{AmbiguousOffset, TimeZone};
use jiff::{Timestamp, ToSpan};

use crate::db::UtcDateTime;
use crate::regex::compile;

const SYNTAX: &str =
    "Use 8pm, 20:30, in 45m, tomorrow 7pm, or a future Discord <t:unix> timestamp.";

fn syntax() -> String {
    SYNTAX.to_owned()
}

/// `StartTime.parse/3`.
pub fn parse(
    input: Option<&str>,
    now: UtcDateTime,
    zone: &str,
) -> Result<Option<UtcDateTime>, String> {
    let Some(input) = input else {
        return Ok(None);
    };
    let input = input.trim().to_lowercase();
    let start = parse_input(&input, now, zone)?;
    if start.unix() > now.unix() {
        Ok(Some(start))
    } else {
        Err("The start time must be in the future.".into())
    }
}

fn parse_input(input: &str, now: UtcDateTime, zone: &str) -> Result<UtcDateTime, String> {
    if let Some(captures) = compile(r"(?i)^<t:(\d{1,11})(?::[tdfr])?>$").captures(input) {
        return captures
            .get(1)
            .and_then(|unix| unix.as_str().parse::<i64>().ok())
            .filter(|unix| Timestamp::from_second(*unix).is_ok())
            .and_then(UtcDateTime::from_unix)
            .ok_or_else(syntax);
    }
    if let Some(captures) = compile(r"^in (\d{1,6})\s*(m|h)$").captures(input) {
        let amount: i64 = captures
            .get(1)
            .and_then(|amount| amount.as_str().parse().ok())
            .ok_or_else(syntax)?;
        let unit = if captures.get(2).map(|unit| unit.as_str()) == Some("h") {
            3600
        } else {
            60
        };
        return UtcDateTime::from_unix(now.unix() + amount * unit).ok_or_else(syntax);
    }
    clock(input, now, zone)
}

fn clock(input: &str, now: UtcDateTime, zone: &str) -> Result<UtcDateTime, String> {
    let (tomorrow, clock) = match input.strip_prefix("tomorrow ") {
        Some(rest) => (true, rest),
        None => (false, input),
    };
    // Elixir's Tz is case-sensitive; jiff's lookup is not.
    let tz = TimeZone::get(zone)
        .ok()
        .filter(|tz| tz.iana_name() == Some(zone))
        .ok_or_else(syntax)?;
    let time = time(clock).ok_or_else(syntax)?;
    let now_ts = Timestamp::from_second(now.unix()).map_err(|_| syntax())?;
    let local = now_ts.to_zoned(tz.clone()).datetime();
    let mut date: Date = local.date();
    // Compare wall times before conversion: adding 24 UTC hours is wrong across DST.
    if tomorrow || time <= local.time() {
        date = date.checked_add(1.day()).map_err(|_| syntax())?;
    }
    let wall = DateTime::from_parts(date, time);
    let ambiguous = tz.to_ambiguous_timestamp(wall);
    match ambiguous.offset() {
        AmbiguousOffset::Unambiguous { .. } => {}
        AmbiguousOffset::Gap { .. } => {
            return Err(
                "That clock time does not exist due to DST. Use a Discord timestamp.".into(),
            );
        }
        AmbiguousOffset::Fold { .. } => {
            return Err("That clock time occurs twice due to DST. Use a Discord timestamp.".into());
        }
    }
    let timestamp = ambiguous.unambiguous().map_err(|_| syntax())?;
    UtcDateTime::from_unix(timestamp.as_second()).ok_or_else(syntax)
}

fn time(input: &str) -> Option<Time> {
    let captures = compile(r"^(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$").captures(input)?;
    let hour: i8 = captures.get(1)?.as_str().parse().ok()?;
    let minute: i8 = match captures.get(2) {
        Some(minute) => minute.as_str().parse().ok()?,
        None => 0,
    };
    let hour = match captures.get(3).map(|period| period.as_str()) {
        Some(period) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            hour % 12 + if period == "pm" { 12 } else { 0 }
        }
        None => hour,
    };
    Time::new(hour, minute, 0, 0).ok()
}
