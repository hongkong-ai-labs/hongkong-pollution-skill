//! Hong Kong air pollution data from official open-data endpoints.
//!
//! The crate is the library behind the `hkpollution` binary. It exposes the
//! [EPD](https://www.aqhi.gov.hk/) Air Quality Health Index (AQHI) and past-24-hour
//! pollutant concentrations together with [HKO](https://www.hko.gov.hk/) current
//! weather readings.
//!
//! Everything is split so that parsing is pure and testable offline:
//!
//! - [`http`] performs the (timeout-guarded) HTTP calls,
//! - [`parse`] turns raw payloads into [`model`] values without touching the network,
//! - [`aqhi`], [`pollutants`] and [`weather`] combine both for one data source each,
//! - [`epd_app`] adds the undocumented EPD app API forecast and station history,
//! - [`praise`] adds the PRAISE-HK street-level AQHI and pollutant forecast,
//! - [`mcp`] speaks JSON-RPC 2.0 over stdio for AI agents.
//!
//! ```
//! use hongkong_pollution_skill::model::resolve_station;
//!
//! // Station lookup is a case-insensitive substring match.
//! assert_eq!(resolve_station("mong kok").unwrap(), vec!["Mong Kok".to_string()]);
//! assert_eq!(resolve_station("CENTRAL").unwrap().len(), 2);
//! ```

pub mod advice;
pub mod aqhi;
pub mod aqhi24;
pub mod beach;
pub mod brief;
pub mod cache;
pub mod csv;
pub mod digest;
pub mod doctor;
pub mod epd_app;
pub mod error;
pub mod http;
pub mod mcp;
pub mod model;
pub mod parse;
pub mod pollutants;
pub mod praise;
pub mod radiation;
pub mod recycling;
pub mod redtide;
pub mod stats;
pub mod store;
pub mod uv;
pub mod watch;
pub mod water;
pub mod weather;

pub use error::{Error, Result};

/// Crate version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Current time as an ISO-8601 string in Hong Kong time (`+08:00`).
///
/// Hong Kong has had no daylight-saving transitions since 1979, so a fixed
/// `+08:00` offset is exact. The conversion uses the standard civil-from-days
/// algorithm instead of a date-time crate.
#[must_use]
pub fn now_hkt_iso8601() -> String {
    format_hkt(unix_seconds())
}

/// Current time plus `hours`, as an ISO-8601 string in Hong Kong time.
///
/// Used for the upper bound of the PRAISE-HK 48-hour forecast window; the same
/// fixed `+08:00` arithmetic as [`now_hkt_iso8601`] applies.
#[must_use]
pub fn now_hkt_iso8601_plus_hours(hours: i64) -> String {
    format_hkt(unix_seconds() + hours * 3_600)
}

/// Current UTC hour as `YYYY-MM-DDTHH:00:00.000Z`.
///
/// The PRAISE-HK tile endpoint wants an ISO-8601 timestamp and rejects a
/// malformed one, so the hour is floored and rendered without an offset.
#[must_use]
pub fn utc_hour_iso8601() -> String {
    let unix_seconds = unix_seconds();
    let hour_start = unix_seconds - unix_seconds.rem_euclid(3_600);
    let days = hour_start.div_euclid(86_400);
    let rem = hour_start.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:00:00.000Z")
}

/// Current Unix time in seconds.
#[must_use]
pub fn now_unix_seconds() -> i64 {
    unix_seconds()
}

/// Render a Unix timestamp as ISO-8601 in Hong Kong time.
#[must_use]
pub fn iso8601_from_unix(seconds: i64) -> String {
    format_hkt(seconds)
}

/// Render a Unix timestamp as an RFC 1123 HTTP date in GMT.
///
/// Used for `If-Modified-Since`, which the upstream feeds accept.
#[must_use]
pub fn rfc1123_from_unix(seconds: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = WEEKDAYS[days.rem_euclid(7) as usize];
    let month_name = MONTHS[(month - 1) as usize];
    format!(
        "{weekday}, {day:02} {month_name} {year:04} {:02}:{:02}:{:02} GMT",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn format_hkt(unix_seconds: i64) -> String {
    let local = unix_seconds + 8 * 3_600;
    let days = local.div_euclid(86_400);
    let rem = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+08:00")
}

/// Days since the Unix epoch for a civil `(year, month, day)` date.
///
/// The inverse of [`civil_from_days`], used to age dates from the CSVs without
/// a date-time dependency.
#[must_use]
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Parse a `YYYY-MM-DD` date into days since the Unix epoch.
#[must_use]
pub fn days_from_iso(iso: &str) -> Option<i64> {
    let mut parts = iso.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Today's date in Hong Kong as days since the Unix epoch.
#[must_use]
pub fn hkt_day_number() -> i64 {
    (unix_seconds() + 8 * 3_600).div_euclid(86_400)
}

/// Convert days since the Unix epoch to a `(year, month, day)` civil date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn converts_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(super::days_from_civil(1970, 1, 1), 0);
        assert_eq!(super::days_from_civil(2024, 10, 4), 20_000);
        assert_eq!(super::days_from_civil(1969, 12, 31), -1);
        assert_eq!(super::days_from_iso("2024-10-04"), Some(20_000));
        assert_eq!(super::days_from_iso("2024-13-04"), None);
    }

    #[test]
    fn renders_http_dates() {
        assert_eq!(super::rfc1123_from_unix(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(
            super::rfc1123_from_unix(1_700_000_000),
            "Tue, 14 Nov 2023 22:13:20 GMT"
        );
    }
}
