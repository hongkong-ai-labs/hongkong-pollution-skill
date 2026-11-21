//! PRAISE-HK street-level AQHI and pollutant forecast from HKUST.
//!
//! `praise-web.ust.hk:8443/api/pollutant_index` is an undocumented app API.
//! It is additive: the documented EPD feeds stay the primary source for
//! `aqhi`, `pollutants`, `summary` and `weather`. Requests go through the
//! shared [`crate::http`] timeouts and User-Agent, and reports are labelled
//! `"source": "PRAISE-HK"`.

use crate::error::{Error, Result};
use crate::http;
use crate::model::{StreetLocation, StreetReading, StreetReport};
use crate::parse;
use crate::{now_hkt_iso8601, now_hkt_iso8601_plus_hours};

/// Street-level pollutant index endpoint.
pub const POLLUTANT_INDEX_URL: &str = "https://praise-web.ust.hk:8443/api/pollutant_index";

/// Current weather at a coordinate.
pub const WEATHER_URL: &str = "https://praise-web.ust.hk:8443/api/weather";

/// Freshness of a cached `pollutant_index` body, in seconds.
const CACHE_TTL_SECS: u64 = 300;

/// Series requested from the endpoint, in the order the plan specifies.
pub const PIDS: &str =
    "AQHIBN,AQHIER,AQHIPM,AQHIPM10,AQHIPM25,AQHINO2,AQHIO3,AQHISO2,NO2,PM2.5,PM10,O3,SO2";

/// Source label used by street-level reports.
pub const SOURCE: &str = "PRAISE-HK";

/// Largest accepted `--hours` window.
pub const MAX_HOURS: u32 = 48;

/// Hourly rows shown when `--hours` is omitted.
pub const DEFAULT_HOURS: u32 = 24;

/// Accepted latitude range for a Hong Kong street-level query.
pub const LAT_RANGE: (f64, f64) = (22.1, 22.6);

/// Accepted longitude range for a Hong Kong street-level query.
pub const LNG_RANGE: (f64, f64) = (113.8, 114.5);

/// Length of the forecast window requested from the endpoint.
const FORECAST_HOURS: i64 = 48;

/// Fetch the street-level forecast for a Hong Kong coordinate.
///
/// The window runs from now to now + 48 h, so the payload always contains the
/// current hour plus 48 forecast hours; `hours` only trims what is reported.
///
/// # Errors
///
/// Returns [`Error::Usage`] when the coordinate is outside Hong Kong or
/// `hours` is out of range, and the usual HTTP/parse errors otherwise.
pub fn fetch(lat: f64, lng: f64, hours: u32) -> Result<StreetReport> {
    validate_location(lat, lng)?;
    check_hours(hours)?;
    let url = pollutant_index_url(lat, lng);
    let body = http::get_cached(&url, CACHE_TTL_SECS)?;
    let series = parse::parse_praise_json(&body)?;
    build(series, lat, lng, hours)
}

/// Full `pollutant_index` URL for a coordinate, over the 48-hour window.
#[must_use]
pub fn pollutant_index_url(lat: f64, lng: f64) -> String {
    let t0 = now_hkt_iso8601();
    let t1 = now_hkt_iso8601_plus_hours(FORECAST_HOURS);
    format!(
        "{POLLUTANT_INDEX_URL}?lat={}&lng={}&pids={}&t0={}&t1={}",
        http::encode_query_value(&lat.to_string()),
        http::encode_query_value(&lng.to_string()),
        http::encode_query_value(PIDS),
        http::encode_query_value(&t0),
        http::encode_query_value(&t1),
    )
}

/// Full `weather` URL for a coordinate.
#[must_use]
pub fn weather_url(lat: f64, lng: f64) -> String {
    format!(
        "{WEATHER_URL}?lat={}&lng={}",
        http::encode_query_value(&lat.to_string()),
        http::encode_query_value(&lng.to_string()),
    )
}

/// Trim a parsed series to the requested window and assemble the report.
///
/// `current` is the first entry, which is the current hour in the upstream
/// series.
///
/// # Errors
///
/// Returns [`Error::Usage`] when the coordinate or `hours` is invalid.
pub fn build(
    mut series: Vec<StreetReading>,
    lat: f64,
    lng: f64,
    hours: u32,
) -> Result<StreetReport> {
    validate_location(lat, lng)?;
    check_hours(hours)?;
    series.truncate(hours as usize);
    let current = series.first().cloned();
    Ok(StreetReport {
        location: StreetLocation { lat, lng },
        current,
        series,
        source: SOURCE.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Check that a coordinate lies inside the Hong Kong bounds.
///
/// # Errors
///
/// Returns [`Error::Usage`] for a non-finite or out-of-bounds coordinate.
pub fn validate_location(lat: f64, lng: f64) -> Result<()> {
    if !lat.is_finite() || !(LAT_RANGE.0..=LAT_RANGE.1).contains(&lat) {
        return Err(Error::usage(format!(
            "--lat must be between {} and {} (Hong Kong bounds), got {lat}",
            LAT_RANGE.0, LAT_RANGE.1
        )));
    }
    if !lng.is_finite() || !(LNG_RANGE.0..=LNG_RANGE.1).contains(&lng) {
        return Err(Error::usage(format!(
            "--lng must be between {} and {} (Hong Kong bounds), got {lng}",
            LNG_RANGE.0, LNG_RANGE.1
        )));
    }
    Ok(())
}

fn check_hours(hours: u32) -> Result<()> {
    if hours == 0 || hours > MAX_HOURS {
        return Err(Error::usage(format!(
            "--hours must be between 1 and {MAX_HOURS}, got {hours}"
        )));
    }
    Ok(())
}
