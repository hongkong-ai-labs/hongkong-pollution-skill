//! HKO ultraviolet index.
//!
//! The Observatory publishes the past 15-minute mean UV index for King's Park
//! as a two-column CSV, updated every 15 minutes during daylight. The band
//! thresholds follow the World Health Organization scale that HKO publishes
//! with its UV advisory.

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::now_hkt_iso8601;

/// Latest 15-minute mean UV index.
pub const UV_URL: &str =
    "https://data.weather.gov.hk/weatherAPI/hko_data/regional-weather/latest_15min_uvindex.csv";

/// Updated every 15 minutes; five minutes of cache is enough.
const CACHE_TTL_SECS: u64 = 300;

/// The `uv` payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UvReport {
    /// Observation time, ISO-8601 with `+08:00`.
    pub time: String,
    /// UV index, one decimal.
    pub index: f64,
    /// WHO exposure band: low, moderate, high, very-high or extreme.
    pub band: String,
    /// Source URL.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// WHO UV exposure band for an index value.
#[must_use]
pub fn band(index: f64) -> &'static str {
    if index < 3.0 {
        "low"
    } else if index < 6.0 {
        "moderate"
    } else if index < 8.0 {
        "high"
    } else if index < 11.0 {
        "very-high"
    } else {
        "extreme"
    }
}

/// Fetch the latest 15-minute mean UV index.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no parsable reading.
pub fn fetch() -> Result<UvReport> {
    let body = http::get_cached(UV_URL, CACHE_TTL_SECS)?;
    let mut report = parse(&body)?;
    report.fetched_at = now_hkt_iso8601();
    Ok(report)
}

fn parse(body: &str) -> Result<UvReport> {
    let (header, rows) = csv::rows(body);
    let time = csv::column(&header, "date time")
        .ok_or_else(|| Error::parse("UV CSV has no date time column"))?;
    let value = csv::column(&header, "uv index")
        .ok_or_else(|| Error::parse("UV CSV has no UV index column"))?;
    let mut newest: Option<(String, f64)> = None;
    for fields in rows {
        let (Some(raw_time), Some(raw_value)) = (fields.get(time), fields.get(value)) else {
            continue;
        };
        let Ok(index) = raw_value.parse::<f64>() else {
            continue;
        };
        let Some(observed) = compact_time(raw_time) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(time, _)| observed > *time) {
            newest = Some((observed, index));
        }
    }
    let (time, index) = newest.ok_or_else(|| Error::parse("UV CSV has no parsable reading"))?;
    Ok(UvReport {
        time,
        index,
        band: band(index).to_string(),
        source: UV_URL.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// `202611211800` to `2026-11-21T18:00:00+08:00`.
fn compact_time(raw: &str) -> Option<String> {
    let digits = raw.trim();
    if digits.len() != 12 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (year, month, day, hour, minute) = (
        &digits[0..4],
        &digits[4..6],
        &digits[6..8],
        &digits[8..10],
        &digits[10..12],
    );
    let month_number: u32 = month.parse().ok()?;
    let day_number: u32 = day.parse().ok()?;
    let hour_number: u32 = hour.parse().ok()?;
    let minute_number: u32 = minute.parse().ok()?;
    if !(1..=12).contains(&month_number)
        || !(1..=31).contains(&day_number)
        || hour_number > 23
        || minute_number > 59
    {
        return None;
    }
    Some(format!("{year}-{month}-{day}T{hour}:{minute}:00+08:00"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_latest_reading() {
        let report = parse(
            "Date time,past 15-minute mean UV Index\r\n202611211800,0.0\r\n202611211815,6.4\r\n",
        )
        .unwrap();
        assert_eq!(report.index, 6.4);
        assert_eq!(report.time, "2026-11-21T18:15:00+08:00");
        assert_eq!(report.band, "high");
    }

    #[test]
    fn bands_follow_the_who_scale() {
        assert_eq!(band(0.0), "low");
        assert_eq!(band(2.9), "low");
        assert_eq!(band(3.0), "moderate");
        assert_eq!(band(5.9), "moderate");
        assert_eq!(band(6.0), "high");
        assert_eq!(band(8.0), "very-high");
        assert_eq!(band(11.0), "extreme");
    }

    #[test]
    fn rejects_a_non_csv_body() {
        assert!(parse("Not found").is_err());
    }
}
