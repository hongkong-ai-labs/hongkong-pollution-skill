//! AQHI readings: the per-station JSON endpoint with the RSS feed as fallback,
//! plus the rollup used by `summary`.

use crate::error::{Error, Result};
use crate::http;
use crate::model::{AqhiReading, AqhiReport, SummaryReport, rollup};
use crate::parse;
use crate::{now_hkt_iso8601, pollutants, weather};

/// Per-station AQHI JSON (primary source).
pub const AQHI_JSON_URL: &str = "https://dashboard.data.gov.hk/api/aqhi-individual?format=json";

/// Per-station AQHI RSS (fallback when the JSON host fails).
pub const AQHI_RSS_URL: &str = "https://www.aqhi.gov.hk/epd/ddata/html/out/aqhi_ind_rss_Eng.xml";

/// Fetch current AQHI for all 18 stations.
///
/// The JSON endpoint is tried first; on any HTTP or parse failure the RSS feed
/// is used instead. The returned report always names `EPD` as its source.
///
/// # Errors
///
/// Returns [`Error::Http`] or [`Error::Parse`] when both sources fail.
pub fn fetch() -> Result<AqhiReport> {
    let primary = http::get_text(AQHI_JSON_URL).and_then(|body| parse::parse_aqhi_json(&body));
    let stations = match primary {
        Ok(readings) => readings,
        Err(json_error) => {
            let body = http::get_text(AQHI_RSS_URL).map_err(|rss_error| {
                Error::parse(format!(
                    "AQHI JSON source failed ({json_error}); RSS fallback also failed ({rss_error})"
                ))
            })?;
            parse::parse_aqhi_rss(&body).map_err(|rss_error| {
                Error::parse(format!(
                    "AQHI JSON source failed ({json_error}); RSS fallback unreadable ({rss_error})"
                ))
            })?
        }
    };
    Ok(AqhiReport {
        stations,
        source: "EPD".to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Build the `summary` payload: AQHI rollup, pollutant extremes and optional
/// HKO weather.
///
/// # Errors
///
/// Propagates network and parse failures from the EPD and HKO endpoints.
pub fn summary(with_weather: bool) -> Result<SummaryReport> {
    let aqhi_report = fetch()?;
    let pollutant_report = pollutants::fetch(1)?;
    let rolled = rollup(&aqhi_report.stations);
    let weather_report = if with_weather {
        Some(weather::fetch()?)
    } else {
        None
    };
    Ok(SummaryReport {
        fetched_at: aqhi_report.fetched_at,
        stations: aqhi_report.stations,
        worst: rolled.worst,
        mean_aqhi: rolled.mean_aqhi,
        risk_counts: rolled.risk_counts,
        worst_pm25: pollutants::worst_pollutant(&pollutant_report.stations, |reading| reading.pm25),
        worst_no2: pollutants::worst_pollutant(&pollutant_report.stations, |reading| reading.no2),
        weather: weather_report,
        source: if with_weather {
            "EPD / HKO".to_string()
        } else {
            "EPD".to_string()
        },
    })
}

/// Retain only readings whose station matches one of `names`
/// (case-insensitive, exact station match).
pub fn retain_stations(readings: &mut Vec<AqhiReading>, names: &[String]) {
    readings.retain(|reading| {
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&reading.station))
    });
}
