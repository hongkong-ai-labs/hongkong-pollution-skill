//! Current HKO weather readings (`rhrread`).

use crate::error::Result;
use crate::http;
use crate::model::WeatherReport;
use crate::parse;

/// HKO current readings, English.
pub const WEATHER_URL: &str =
    "https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en";

/// Fetch temperature, humidity and active warnings from the HKO.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no readings.
pub fn fetch() -> Result<WeatherReport> {
    let body = http::get_text(WEATHER_URL)?;
    parse::parse_weather_json(&body)
}
