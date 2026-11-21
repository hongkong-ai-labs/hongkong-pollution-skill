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

/// Freshness of a cached AQHI body, in seconds.
const CACHE_TTL_SECS: u64 = 300;

/// Fetch current AQHI for all 18 stations.
///
/// The JSON endpoint is tried first; on any HTTP or parse failure the RSS feed
/// is used instead. The returned report always names `EPD` as its source.
///
/// # Errors
///
/// Returns [`Error::Http`] or [`Error::Parse`] when both sources fail.
pub fn fetch() -> Result<AqhiReport> {
    let primary = http::get_cached(AQHI_JSON_URL, CACHE_TTL_SECS)
        .and_then(|body| parse::parse_aqhi_json(&body));
    let stations = match primary {
        Ok(readings) => readings,
        Err(json_error) => {
            let body = http::get_cached(AQHI_RSS_URL, CACHE_TTL_SECS).map_err(|rss_error| {
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
    // Keep a local observation log for `stats`; this is best-effort and never
    // changes the report.
    crate::store::record(&stations, crate::now_unix_seconds());
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
        trend: None,
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

/// AQHI change at `station` over the last `hours` hourly readings.
///
/// Uses the EPD app API history, which publishes 23 hourly points. Returns a
/// steady trend when fewer than two points are available.
///
/// # Errors
///
/// Returns the usual HTTP/parse/usage errors from [`epd_app::fetch_history`].
pub fn trend(station: &str, hours: u32) -> Result<crate::model::Trend> {
    let report =
        crate::epd_app::fetch_history(Some(station), hours.clamp(2, crate::epd_app::MAX_HOURS))?;
    let series: Vec<_> = report
        .stations
        .first()
        .map(|entry| entry.series.clone())
        .unwrap_or_default()
        .into_iter()
        .take(hours.max(2) as usize)
        .collect();
    let newest = series.iter().find_map(|reading| reading.aqhi);
    let oldest = series
        .iter()
        .rev()
        .find_map(|reading| reading.aqhi)
        .or(newest);
    let (Some(to), Some(from)) = (newest, oldest) else {
        return Err(crate::error::Error::parse(format!(
            "no AQHI history available for {station}"
        )));
    };
    let delta = to - from;
    Ok(crate::model::Trend {
        station: station.to_string(),
        hours: u32::try_from(series.len().saturating_sub(1))
            .unwrap_or(hours)
            .max(1),
        delta,
        direction: match delta {
            0 => "steady".to_string(),
            value if value > 0 => "rising".to_string(),
            _ => "falling".to_string(),
        },
        from,
        to,
    })
}
