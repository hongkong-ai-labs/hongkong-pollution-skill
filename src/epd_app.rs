//! The EPD AQHI app API (`app.epd-asmg.gov.hk/api/v2`): tomorrow's forecast and
//! per-station hourly history.
//!
//! These endpoints are undocumented and additive: the documented open-data
//! feeds stay the primary source for `aqhi`, `pollutants`, `summary` and
//! `weather`. Every request goes through the shared [`crate::http`] timeouts
//! and User-Agent, and reports `"source": "EPD app API"`.

use crate::error::{Error, Result};
use crate::http;
use crate::model::{
    ForecastReport, HistoryReport, MonitorDetail, MonitorStation, StationHistory,
    app_category_label, app_station_id, resolve_station,
};
use crate::now_hkt_iso8601;
use crate::parse;

/// Forecast for the next general/roadside periods.
pub const FORECAST_URL: &str = "https://app.epd-asmg.gov.hk/api/v2/forecast";

/// Current readings for every monitor; `/{id}` appends hourly histories.
pub const MONITORS_URL: &str = "https://app.epd-asmg.gov.hk/api/v2/monitors";

/// About-page sections, long-term air-quality link and general notes.
pub const ABOUTS_URL: &str = "https://app.epd-asmg.gov.hk/api/v2/abouts";

/// Source label used by the forecast and history reports.
pub const SOURCE: &str = "EPD app API";

/// Largest accepted `--hours` window for `history`.
pub const MAX_HOURS: u32 = 23;

/// Freshness of a cached `/forecast` body, in seconds.
const FORECAST_CACHE_TTL_SECS: u64 = 900;

/// Freshness of a cached `/monitors` body, in seconds.
const MONITORS_CACHE_TTL_SECS: u64 = 300;

/// Freshness of one cached `/monitors/{id}` body, in seconds.
const DETAIL_CACHE_TTL_SECS: u64 = 1_800;

/// Fetch the app API about-page sections.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no dated section.
pub fn fetch_abouts() -> Result<Vec<crate::parse::AboutSection>> {
    let body = http::get_cached(ABOUTS_URL, 86_400)?;
    parse::parse_abouts_json(&body)
}

/// Fetch and join tomorrow's AQHI forecast.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload cannot be read.
pub fn fetch_forecast() -> Result<ForecastReport> {
    let body = http::get_cached(FORECAST_URL, FORECAST_CACHE_TTL_SECS)?;
    let periods = parse::parse_forecast_json(&body)?;
    Ok(ForecastReport {
        periods,
        source: SOURCE.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Fetch per-station history, optionally narrowed by the usual `--station`
/// substring matcher.
///
/// One request to `/monitors` always happens; `/monitors/{id}` is requested per
/// selected station only when more than one hour is wanted.
///
/// # Errors
///
/// Returns [`Error::Usage`] when `hours` is out of range, [`Error::NoMatch`]
/// when the station filter matches no monitor, and the usual HTTP/parse errors.
pub fn fetch_history(query: Option<&str>, hours: u32) -> Result<HistoryReport> {
    check_hours(hours)?;
    let ids = match query {
        Some(query) => {
            let names = resolve_station(query)?;
            Some(
                names
                    .iter()
                    .filter_map(|name| app_station_id(name))
                    .collect::<Vec<i64>>(),
            )
        }
        None => None,
    };
    let body = http::get_cached(MONITORS_URL, MONITORS_CACHE_TTL_SECS)?;
    let mut monitors = parse::parse_monitors_json(&body)?;
    if let Some(ids) = &ids {
        monitors.retain(|monitor| ids.contains(&monitor.id));
        if monitors.is_empty() {
            return Err(Error::no_match(format!(
                "no app API monitor matches {query:?}"
            )));
        }
    }
    let details = if hours > 1 {
        fetch_details(&monitors)?
    } else {
        Vec::new()
    };
    build_history(monitors, details, hours)
}

fn fetch_details(monitors: &[MonitorStation]) -> Result<Vec<MonitorDetail>> {
    let mut details = Vec::with_capacity(monitors.len());
    for monitor in monitors {
        let url = format!("{MONITORS_URL}/{}", monitor.id);
        let body = http::get_cached(&url, DETAIL_CACHE_TTL_SECS)?;
        details.push(parse::parse_monitor_detail_json(&body)?);
    }
    Ok(details)
}

/// Assemble the public history report from parsed monitors and any fetched
/// details.
///
/// A station with a detail series keeps that full series (newest first); a
/// station without one falls back to its single newest reading.
///
/// # Errors
///
/// Returns [`Error::Usage`] when `hours` is `0` or greater than
/// [`MAX_HOURS`].
pub fn build_history(
    monitors: Vec<MonitorStation>,
    details: Vec<MonitorDetail>,
    hours: u32,
) -> Result<HistoryReport> {
    check_hours(hours)?;
    let stations = monitors
        .into_iter()
        .map(|monitor| {
            let detail = details.iter().find(|detail| detail.id == monitor.id);
            let (latest, series) = match detail {
                Some(detail) if !detail.series.is_empty() => (
                    detail.latest.clone().or_else(|| monitor.latest.clone()),
                    detail.series.clone(),
                ),
                _ => (
                    monitor.latest.clone(),
                    monitor.latest.iter().cloned().collect(),
                ),
            };
            StationHistory {
                station: monitor.station,
                station_id: monitor.id,
                category: app_category_label(monitor.category).to_string(),
                latitude: monitor.latitude,
                longitude: monitor.longitude,
                latest,
                series,
            }
        })
        .collect();
    Ok(HistoryReport {
        hours,
        stations,
        source: SOURCE.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

fn check_hours(hours: u32) -> Result<()> {
    if hours == 0 || hours > MAX_HOURS {
        return Err(Error::usage(format!(
            "--hours must be between 1 and {MAX_HOURS}, got {hours}"
        )));
    }
    Ok(())
}
