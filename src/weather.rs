//! Current HKO weather readings (`rhrread`).

use serde::Serialize;

use crate::error::Result;
use crate::http;
use crate::model::WeatherReport;
use crate::parse;

/// HKO current readings, English.
pub const WEATHER_URL: &str =
    "https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en";

/// HKO nine-day forecast, English. Probed by `doctor`; not a report yet.
pub const FORECAST_URL: &str =
    "https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=fnd&lang=en";

/// Freshness of a cached current-reading body, in seconds.
const CACHE_TTL_SECS: u64 = 300;

/// Fetch temperature, humidity and active warnings from the HKO.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no readings.
pub fn fetch() -> Result<WeatherReport> {
    let body = http::get_cached(WEATHER_URL, CACHE_TTL_SECS)?;
    parse::parse_weather_json(&body)
}

/// Nine-day forecast, English.
pub const FORECAST_CACHE_TTL_SECS: u64 = 900;

/// One day of the HKO nine-day forecast.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForecastDay {
    /// Forecast date, `YYYY-MM-DD`.
    pub date: String,
    /// Day of the week as published, for example `Sunday`.
    pub week: String,
    /// Short description, for example `Sunny periods`.
    pub weather: Option<String>,
    /// Forecast maximum temperature in degrees Celsius.
    pub max_temp: Option<i64>,
    /// Forecast minimum temperature in degrees Celsius.
    pub min_temp: Option<i64>,
    /// Forecast maximum relative humidity in percent.
    pub max_humidity: Option<i64>,
    /// Forecast minimum relative humidity in percent.
    pub min_humidity: Option<i64>,
    /// Probability of significant rain, as published (`Low`, `Medium`, `High`).
    pub psr: Option<String>,
}

/// The `weather --forecast --json` addition.
#[derive(Debug, Clone, Serialize)]
pub struct ForecastReport {
    /// General situation paragraph, when published.
    pub situation: Option<String>,
    /// Forecast days, soonest first.
    pub days: Vec<ForecastDay>,
    /// Data provider, always `HKO`.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Fetch the HKO nine-day forecast.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no forecast day.
pub fn fetch_forecast() -> Result<ForecastReport> {
    let body = http::get_cached(FORECAST_URL, FORECAST_CACHE_TTL_SECS)?;
    let parsed = parse::parse_hko_forecast_json(&body)?;
    Ok(ForecastReport {
        situation: parsed.situation,
        days: parsed.days,
        source: "HKO".to_string(),
        fetched_at: crate::now_hkt_iso8601(),
    })
}
