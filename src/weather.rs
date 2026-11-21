//! Current HKO weather readings (`rhrread`).

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
