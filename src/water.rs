//! EPD recent river and marine water quality.
//!
//! Both feeds are monthly monitoring CSVs published by EPD: one row per
//! station, sampling month, dissolved oxygen and 5-day biochemical oxygen
//! demand. They are not live readings — the report names the newest sampling
//! month it holds rather than implying "now".

use std::collections::BTreeMap;

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::now_hkt_iso8601;

/// Recent river water quality, one row per station and month.
pub const RIVER_URL: &str =
    "https://cd.epic.epd.gov.hk/riverpsi/en/riverrecent/river-recent-en.csv";

/// Recent marine water quality, one row per station and month.
pub const MARINE_URL: &str =
    "https://cd.epic.epd.gov.hk/marinepsi/en/marinerecent/marine-recent-en.csv";

/// Monitoring updates monthly; six hours of cache is plenty.
const CACHE_TTL_SECS: u64 = 21_600;

/// Which EPD water quality network to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Inland rivers and streams, grouped by water control zone.
    River,
    /// Marine stations, grouped by water control zone.
    Marine,
}

impl Kind {
    /// Command name, also used as the report's `kind`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::River => "river",
            Self::Marine => "marine",
        }
    }

    /// Feed URL.
    #[must_use]
    pub fn url(self) -> &'static str {
        match self {
            Self::River => RIVER_URL,
            Self::Marine => MARINE_URL,
        }
    }

    /// Parse `river` or `marine`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Usage`] for any other value.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "river" | "rivers" | "stream" | "streams" => Ok(Self::River),
            "marine" | "sea" | "coastal" => Ok(Self::Marine),
            other => Err(Error::usage(format!(
                "unknown water kind '{other}'; use river or marine"
            ))),
        }
    }
}

/// One monthly sample.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WaterSample {
    /// Sampling month, `YYYY-MM`.
    pub month: String,
    /// Dissolved oxygen in mg/L.
    pub dissolved_oxygen_mg_l: Option<f64>,
    /// Five-day biochemical oxygen demand in mg/L.
    pub bod5_mg_l: Option<f64>,
}

/// One monitoring station and its recent months.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WaterStation {
    /// Water control zone, for example `Deep Bay`.
    pub control_zone: String,
    /// River name, `null` for marine stations.
    pub watercourse: Option<String>,
    /// Station code, for example `GR1` or `DM3`.
    pub station: String,
    /// Newest sample.
    pub latest: WaterSample,
    /// Kept months, newest first.
    pub series: Vec<WaterSample>,
    /// Mean dissolved oxygen over the kept months, mg/L.
    pub mean_dissolved_oxygen_mg_l: Option<f64>,
    /// Mean BOD5 over the kept months, mg/L.
    pub mean_bod5_mg_l: Option<f64>,
}

/// The `river`/`marine` JSON payload.
#[derive(Debug, Clone, Serialize)]
pub struct WaterReport {
    /// `river` or `marine`.
    pub kind: String,
    /// Months kept per station.
    pub months: u32,
    /// Number of stations after filtering.
    pub count: usize,
    /// Newest sampling month across the kept samples.
    pub latest_month: Option<String>,
    /// Stations, sorted by control zone then station.
    pub stations: Vec<WaterStation>,
    /// Source URL.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Fetch and group recent river or marine monitoring results.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures, [`Error::Parse`] when the
/// payload is not the expected CSV, and [`Error::Usage`] when `months` is 0.
pub fn fetch(kind: Kind, months: u32) -> Result<WaterReport> {
    if months == 0 {
        return Err(Error::usage("--months must be at least 1"));
    }
    let body = http::get_cached(kind.url(), CACHE_TTL_SECS)?;
    let mut report = parse(kind, &body, months)?;
    report.fetched_at = now_hkt_iso8601();
    Ok(report)
}

fn parse(kind: Kind, body: &str, months: u32) -> Result<WaterReport> {
    let (header, rows) = csv::rows(body);
    let zone = csv::column(&header, "control zone")
        .ok_or_else(|| Error::parse("water quality CSV has no water control zone column"))?;
    let station = csv::column(&header, "station")
        .ok_or_else(|| Error::parse("water quality CSV has no station column"))?;
    let date = csv::column(&header, "sampling date")
        .ok_or_else(|| Error::parse("water quality CSV has no sampling date column"))?;
    let oxygen = csv::column(&header, "dissolved oxygen");
    let bod = csv::column(&header, "biochemical oxygen demand");
    let watercourse = csv::column(&header, "river");

    let mut grouped: BTreeMap<(String, Option<String>, String), Vec<WaterSample>> = BTreeMap::new();
    for fields in rows {
        let Some(month) = fields.get(date).and_then(|raw| month_key(raw)) else {
            continue;
        };
        let Some(zone_name) = fields.get(zone).filter(|value| !value.is_empty()) else {
            continue;
        };
        let Some(station_name) = fields.get(station).filter(|value| !value.is_empty()) else {
            continue;
        };
        let river_name = watercourse
            .and_then(|index| fields.get(index))
            .filter(|value| !value.is_empty())
            .cloned();
        grouped
            .entry((zone_name.clone(), river_name, station_name.clone()))
            .or_default()
            .push(WaterSample {
                month,
                dissolved_oxygen_mg_l: value(fields.get(oxygen.unwrap_or(usize::MAX))),
                bod5_mg_l: value(fields.get(bod.unwrap_or(usize::MAX))),
            });
    }
    if grouped.is_empty() {
        return Err(Error::parse(
            "water quality CSV has no parsable sampling rows",
        ));
    }

    let limit = months as usize;
    let mut stations: Vec<WaterStation> = grouped
        .into_iter()
        .map(|((control_zone, watercourse, station), mut series)| {
            series.sort_by(|left, right| right.month.cmp(&left.month));
            series.truncate(limit);
            let mean_oxygen = mean(
                series
                    .iter()
                    .filter_map(|sample| sample.dissolved_oxygen_mg_l),
            );
            let mean_bod = mean(series.iter().filter_map(|sample| sample.bod5_mg_l));
            WaterStation {
                control_zone,
                watercourse,
                station,
                latest: series[0].clone(),
                series,
                mean_dissolved_oxygen_mg_l: mean_oxygen,
                mean_bod5_mg_l: mean_bod,
            }
        })
        .collect();
    stations.sort_by(|left, right| {
        left.control_zone
            .cmp(&right.control_zone)
            .then_with(|| left.station.cmp(&right.station))
    });
    let latest_month = stations
        .iter()
        .map(|entry| entry.latest.month.clone())
        .max();
    Ok(WaterReport {
        kind: kind.name().to_string(),
        months,
        count: stations.len(),
        latest_month,
        stations,
        source: kind.url().to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Keep stations whose zone, watercourse or station code contains `query`.
pub fn retain_query(report: &mut WaterReport, query: &str) {
    let needle = query.trim().to_lowercase();
    report.stations.retain(|entry| {
        entry.control_zone.to_lowercase().contains(&needle)
            || entry.station.to_lowercase().contains(&needle)
            || entry
                .watercourse
                .as_ref()
                .is_some_and(|name| name.to_lowercase().contains(&needle))
    });
    report.count = report.stations.len();
}

/// Keep one control zone, matched case-insensitively against the full name.
pub fn retain_zone(report: &mut WaterReport, zone: &str) {
    let needle = zone.trim().to_lowercase();
    report
        .stations
        .retain(|entry| entry.control_zone.to_lowercase().contains(&needle));
    report.count = report.stations.len();
    report.latest_month = report.stations.iter().map(|e| e.latest.month.clone()).max();
}

fn value(raw: Option<&String>) -> Option<f64> {
    raw?.trim()
        .trim_start_matches('<')
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let collected: Vec<f64> = values.collect();
    if collected.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let count = collected.len() as f64;
    Some((collected.iter().sum::<f64>() / count * 10.0).round() / 10.0)
}

/// `08/2025` or `2025-08` to `2025-08`.
fn month_key(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some((year, month)) = raw.split_once('-') {
        let month: u32 = month.parse().ok()?;
        if !(1..=12).contains(&month) || year.len() != 4 {
            return None;
        }
        return Some(format!("{year}-{month:02}"));
    }
    let (month, year) = raw.split_once('/')?;
    let month: u32 = month.parse().ok()?;
    let year: u32 = year.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Some(format!("{year:04}-{month:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIVER: &str = "\u{feff}\"Water Control Zone\",\"River\",\"Station\",\"Sampling Date (MM/YYYY)\",\"Dissolved Oxygen (mg/L)\",\"5-Day Biochemical Oxygen Demand (mg/L)\"\r
\"Deep Bay\",\"River Ganges\",\"GR1\",\"11/2024\",\"7\",\"7.7\"\r
\"Deep Bay\",\"River Ganges\",\"GR1\",\"01/2025\",\"11.3\",\"2\"\r
\"Deep Bay\",\"River Ganges\",\"GR1\",\"02/2025\",\"11.4\",\"7.3\"\r
\"Deep Bay\",\"Tin Shui Wai\",\"TSW1\",\"01/2025\",\"<4\",\"3\"\r
";

    #[test]
    fn groups_months_newest_first_and_averages() {
        let report = parse(Kind::River, RIVER, 2).unwrap();
        assert_eq!(report.count, 2);
        assert_eq!(report.latest_month.as_deref(), Some("2025-02"));
        let ganges = report
            .stations
            .iter()
            .find(|entry| entry.station == "GR1")
            .unwrap();
        assert_eq!(ganges.watercourse.as_deref(), Some("River Ganges"));
        assert_eq!(ganges.series.len(), 2);
        assert_eq!(ganges.latest.month, "2025-02");
        assert_eq!(ganges.mean_bod5_mg_l, Some(4.7));
        let tin = report
            .stations
            .iter()
            .find(|entry| entry.station == "TSW1")
            .unwrap();
        assert_eq!(tin.latest.dissolved_oxygen_mg_l, Some(4.0));
    }

    #[test]
    fn rejects_a_body_without_rows() {
        assert!(parse(Kind::Marine, "Invalid Access / Page Not Found!", 3).is_err());
    }

    #[test]
    fn filters_by_zone_and_text() {
        let mut report = parse(Kind::River, RIVER, 3).unwrap();
        retain_query(&mut report, "ganges");
        assert_eq!(report.count, 1);
        let mut report = parse(Kind::River, RIVER, 3).unwrap();
        retain_zone(&mut report, "deep bay");
        assert_eq!(report.count, 2);
    }

    #[test]
    fn parses_month_keys() {
        assert_eq!(month_key("01/2025").as_deref(), Some("2025-01"));
        assert_eq!(month_key("2025-01").as_deref(), Some("2025-01"));
        assert_eq!(month_key("13/2025"), None);
    }
}
