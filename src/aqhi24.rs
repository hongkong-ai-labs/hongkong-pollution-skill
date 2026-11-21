//! Documented past-24-hour AQHI per station.
//!
//! `aqhi.gov.hk` publishes the last 24 hourly AQHI values for every station as
//! an XML feed. Unlike the app API, this is a documented open-data endpoint, so
//! it is the fallback for `history` and the first-run source for `stats`.

use serde::Serialize;

use crate::error::{Error, Result};
use crate::http;
use crate::model::{
    HistoryReading, HistoryReport, StationHistory, resolve_station, station_category,
};
use crate::now_hkt_iso8601;
use crate::parse;

/// Past 24-hour AQHI, one item per station-hour.
pub const AQHI_24H_URL: &str = "https://www.aqhi.gov.hk/epd/ddata/html/out/24aqhi_Eng.xml";

/// Updated hourly; ten minutes of cache is enough.
const CACHE_TTL_SECS: u64 = 600;

/// One station-hour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HourlyReading {
    /// Canonical station name.
    pub station: String,
    /// Reading time, ISO-8601 with `+08:00`.
    pub time: String,
    /// Air Quality Health Index.
    pub aqhi: Option<i64>,
}

/// The parsed documented feed.
#[derive(Debug, Clone, Serialize)]
pub struct Aqhi24Report {
    /// `lastBuildDate` of the feed, ISO-8601 when it parsed.
    pub last_build: Option<String>,
    /// Readings for every station, newest first.
    pub readings: Vec<HourlyReading>,
}

/// Fetch and parse the documented past-24-hour AQHI feed.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no station-hour item.
pub fn fetch() -> Result<Aqhi24Report> {
    let body = http::get_cached(AQHI_24H_URL, CACHE_TTL_SECS)?;
    parse(&body)
}

fn parse(body: &str) -> Result<Aqhi24Report> {
    let last_build = parse::inner_text(body, "lastBuildDate").and_then(parse::rfc2822_to_iso);
    let mut readings = Vec::new();
    for item in parse::blocks(body, "item") {
        let station = parse::inner_text(item, "StationName")
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        let time = parse::inner_text(item, "DateTime").and_then(parse::rfc2822_to_iso);
        let aqhi = parse::inner_text(item, "aqhi")
            .and_then(|raw| raw.trim().parse::<i64>().ok())
            .filter(|value| (1..=11).contains(value));
        if let (Some(station), Some(time)) = (station, time) {
            readings.push(HourlyReading {
                station,
                time,
                aqhi,
            });
        }
    }
    if readings.is_empty() {
        return Err(Error::parse(
            "past 24-hour AQHI feed has no parsable station rows",
        ));
    }
    readings.sort_by(|left, right| {
        left.station
            .cmp(&right.station)
            .then_with(|| right.time.cmp(&left.time))
    });
    Ok(Aqhi24Report {
        last_build,
        readings,
    })
}

/// Rebuild the documented feed as a [`HistoryReport`], so `history` keeps its
/// schema when the app API is unavailable.
///
/// # Errors
///
/// Returns [`Error::Usage`] for `hours` outside 1..=23, [`Error::NoMatch`] when
/// nothing matches `query`, and the usual HTTP/parse errors.
pub fn history_report(query: Option<&str>, hours: u32) -> Result<HistoryReport> {
    if hours == 0 || hours > crate::epd_app::MAX_HOURS {
        return Err(Error::usage(format!(
            "--hours must be between 1 and {}",
            crate::epd_app::MAX_HOURS
        )));
    }
    let names = match query {
        Some(query) => Some(resolve_station(query)?),
        None => None,
    };
    let report = fetch()?;
    let mut stations: Vec<StationHistory> = Vec::new();
    for name in names.as_deref().unwrap_or(&[]) {
        push_station(&mut stations, &report, name, hours);
    }
    if names.is_none() {
        let mut seen: Vec<String> = report
            .readings
            .iter()
            .map(|reading| reading.station.clone())
            .collect();
        seen.sort();
        seen.dedup();
        for name in &seen {
            push_station(&mut stations, &report, name, hours);
        }
    }
    if stations.is_empty() {
        return Err(Error::no_match("no station in the past 24-hour AQHI feed"));
    }
    Ok(HistoryReport {
        hours,
        stations,
        source: "EPD 24-hour AQHI".to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

fn push_station(stations: &mut Vec<StationHistory>, report: &Aqhi24Report, name: &str, hours: u32) {
    let mut series: Vec<HistoryReading> = report
        .readings
        .iter()
        .filter(|reading| reading.station.eq_ignore_ascii_case(name))
        .map(|reading| HistoryReading {
            time: reading.time.clone(),
            aqhi: reading.aqhi,
            no2: None,
            o3: None,
            so2: None,
            co: None,
            pm10: None,
            pm25: None,
        })
        .collect();
    if series.is_empty() {
        return;
    }
    series.sort_by(|left, right| right.time.cmp(&left.time));
    series.truncate(hours as usize);
    stations.push(StationHistory {
        station: name.to_string(),
        station_id: 0,
        category: station_category(name).to_string(),
        latitude: None,
        longitude: None,
        latest: series.first().cloned(),
        series,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<AQHI24HrReport>
      <lastBuildDate>Sun, 22 Nov 2026 01:30:00 +0800</lastBuildDate>
      <item><type>General Stations</type><StationName>Central/Western</StationName>
        <DateTime>Sat, 21 Nov 2026 02:00:00 +0800</DateTime><aqhi>3</aqhi></item>
      <item><type>General Stations</type><StationName>Central/Western</StationName>
        <DateTime>Sat, 21 Nov 2026 03:00:00 +0800</DateTime><aqhi>4</aqhi></item>
    </AQHI24HrReport>"#;

    #[test]
    fn parses_items_and_builds_history() {
        let report = parse(SAMPLE).unwrap();
        assert_eq!(report.readings.len(), 2);
        assert_eq!(
            report.last_build.as_deref(),
            Some("2026-11-22T01:30:00+08:00")
        );
        assert_eq!(report.readings[0].time, "2026-11-21T03:00:00+08:00");
    }

    #[test]
    fn rejects_an_empty_feed() {
        assert!(parse("<AQHI24HrReport></AQHI24HrReport>").is_err());
    }
}
