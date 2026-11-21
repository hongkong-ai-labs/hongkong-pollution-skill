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

pub mod aqhi;
pub mod epd_app;
pub mod error;
pub mod http;
pub mod mcp;
pub mod model;
pub mod parse;
pub mod pollutants;
pub mod praise;
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
    }
}
