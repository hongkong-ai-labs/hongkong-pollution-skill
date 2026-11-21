//! `stats` and `compare`: history the upstream feeds do not keep.
//!
//! `stats` merges the local observation log with whatever the documented
//! 24-hour feed still holds, so the first run is already useful and coverage
//! grows with every call. `compare` ranks stations and separates general from
//! roadside monitoring, the comparison that is otherwise misleading.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::aqhi24;
use crate::error::{Error, Result};
use crate::model::{AqhiReading, station_category};
use crate::now_hkt_iso8601;
use crate::{aqhi, store};

/// AQHI at or above which an hour counts as an exceedance in `stats`.
pub const DEFAULT_THRESHOLD: i64 = 7;

/// Largest accepted window for `stats`, in days.
pub const MAX_DAYS: u32 = 365;

/// Statistics for one station.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StationStats {
    /// Canonical station name.
    pub station: String,
    /// `general` or `roadside`.
    pub category: String,
    /// Observations used.
    pub samples: usize,
    /// Lowest AQHI seen.
    pub min: i64,
    /// Highest AQHI seen.
    pub max: i64,
    /// Mean AQHI, one decimal.
    pub mean: f64,
    /// Hours at or above the threshold.
    pub hours_at_or_above: usize,
    /// Oldest observation used, ISO-8601.
    pub first_seen: String,
    /// Newest observation used, ISO-8601.
    pub last_seen: String,
}

/// The `stats` payload.
#[derive(Debug, Clone, Serialize)]
pub struct StatsReport {
    /// Window in days.
    pub days: u32,
    /// Exceedance threshold.
    pub threshold: i64,
    /// Observations used after deduplication.
    pub observations: usize,
    /// Earliest observation available locally, ISO-8601.
    pub coverage_from: Option<String>,
    /// Stations, worst maximum first.
    pub stations: Vec<StationStats>,
    /// `local log + EPD 24-hour AQHI`.
    pub source: String,
    /// When this process assembled the report, in Hong Kong time.
    pub fetched_at: String,
}

/// One row of `compare`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompareRow {
    /// Canonical station name.
    pub station: String,
    /// `general` or `roadside`.
    pub category: String,
    /// Current AQHI.
    pub aqhi: Option<i64>,
    /// Health risk band.
    pub risk: Option<String>,
    /// Difference from the mean of the same category, one decimal.
    pub vs_category_mean: Option<f64>,
}

/// The `compare` payload.
#[derive(Debug, Clone, Serialize)]
pub struct CompareReport {
    /// Mean AQHI of all general stations.
    pub general_mean: Option<f64>,
    /// Mean AQHI of all roadside stations.
    pub roadside_mean: Option<f64>,
    /// Mean AQHI across every station.
    pub network_mean: Option<f64>,
    /// Stations, worst first.
    pub stations: Vec<CompareRow>,
    /// `EPD`.
    pub source: String,
    /// When this process assembled the report, in Hong Kong time.
    pub fetched_at: String,
}

/// Build station statistics over the last `days` days.
///
/// # Errors
///
/// Returns [`Error::Usage`] for `days` outside 1..=[`MAX_DAYS`] or a threshold
/// outside 1..=11, and the usual HTTP/parse errors when the documented feed
/// cannot be read and nothing is stored locally.
pub fn build(days: u32, query: Option<&str>, threshold: i64) -> Result<StatsReport> {
    if days == 0 || days > MAX_DAYS {
        return Err(Error::usage(format!(
            "--days must be between 1 and {MAX_DAYS}"
        )));
    }
    if !(1..=11).contains(&threshold) {
        return Err(Error::usage("--threshold must be between 1 and 11"));
    }
    // The documented feed is the only history that survives a fresh install.
    let mut observations: BTreeMap<(String, i64), i64> = BTreeMap::new();
    let now = crate::now_unix_seconds();
    let cutoff = store::Observation::hour_of(now) - i64::from(days) * 24 * 3_600;
    for row in store::read_days(days) {
        observations.insert((row.station, row.hour), row.aqhi);
    }
    let feed = aqhi24::fetch().map(|report| {
        for reading in &report.readings {
            let Some(hour) = hour_from_iso(&reading.time) else {
                continue;
            };
            if hour < cutoff {
                continue;
            }
            if let Some(aqhi) = reading.aqhi {
                observations.insert((reading.station.clone(), hour), aqhi);
            }
        }
    });
    feed?;
    if observations.is_empty() {
        return Err(Error::parse(
            "no AQHI observations yet; run `hkpollution aqhi` or `stats` again later",
        ));
    }

    let mut grouped: BTreeMap<String, Vec<(i64, i64)>> = BTreeMap::new();
    for ((station, hour), aqhi) in observations {
        if let Some(query) = query {
            if !station
                .to_lowercase()
                .contains(&query.trim().to_lowercase())
            {
                continue;
            }
        }
        grouped.entry(station).or_default().push((hour, aqhi));
    }
    if grouped.is_empty() {
        return Err(Error::no_match(format!(
            "no station matches {:?}",
            query.unwrap_or_default()
        )));
    }

    let mut stations: Vec<StationStats> = Vec::new();
    let mut earliest: Option<i64> = None;
    for (station, mut rows) in grouped {
        rows.sort_by_key(|(hour, _)| *hour);
        let values: Vec<i64> = rows.iter().map(|(_, aqhi)| *aqhi).collect();
        let sum: i64 = values.iter().sum();
        #[allow(clippy::cast_precision_loss)]
        let mean = (sum as f64 / values.len() as f64 * 10.0).round() / 10.0;
        let first = rows.first().map(|(hour, _)| *hour).unwrap_or(now);
        let last = rows.last().map(|(hour, _)| *hour).unwrap_or(now);
        earliest = Some(earliest.map_or(first, |value| value.min(first)));
        stations.push(StationStats {
            category: station_category(&station).to_string(),
            station,
            samples: values.len(),
            min: values.iter().copied().min().unwrap_or(0),
            max: values.iter().copied().max().unwrap_or(0),
            mean,
            hours_at_or_above: values.iter().filter(|aqhi| **aqhi >= threshold).count(),
            first_seen: crate::iso8601_from_unix(first),
            last_seen: crate::iso8601_from_unix(last),
        });
    }
    stations.sort_by(|left, right| {
        right.max.cmp(&left.max).then_with(|| {
            right
                .mean
                .partial_cmp(&left.mean)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    Ok(StatsReport {
        days,
        threshold,
        observations: stations.iter().map(|station| station.samples).sum(),
        coverage_from: earliest.map(crate::iso8601_from_unix),
        stations,
        source: "local log + EPD 24-hour AQHI".to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Compare current AQHI across stations, separated by category.
///
/// # Errors
///
/// Returns the usual HTTP/parse errors from [`crate::aqhi::fetch`].
pub fn compare() -> Result<CompareReport> {
    let report = aqhi::fetch()?;
    let rows: Vec<&AqhiReading> = report
        .stations
        .iter()
        .filter(|reading| reading.aqhi.is_some())
        .collect();
    let general_mean = category_mean(&rows, "general");
    let roadside_mean = category_mean(&rows, "roadside");
    let network_mean = mean(rows.iter().filter_map(|reading| reading.aqhi));
    let mut stations: Vec<CompareRow> = rows
        .iter()
        .map(|reading| {
            let category = station_category(&reading.station);
            let reference = if category == "roadside" {
                roadside_mean
            } else {
                general_mean
            };
            CompareRow {
                station: reading.station.clone(),
                category: category.to_string(),
                aqhi: reading.aqhi,
                risk: reading.health_risk.clone(),
                vs_category_mean: match (reading.aqhi, reference) {
                    (Some(value), Some(mean)) => {
                        #[allow(clippy::cast_precision_loss)]
                        let value = value as f64;
                        Some(((value - mean) * 10.0).round() / 10.0)
                    }
                    _ => None,
                },
            }
        })
        .collect();
    stations.sort_by(|left, right| {
        right
            .aqhi
            .cmp(&left.aqhi)
            .then_with(|| left.station.cmp(&right.station))
    });
    Ok(CompareReport {
        general_mean,
        roadside_mean,
        network_mean,
        stations,
        source: "EPD".to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

fn category_mean(rows: &[&AqhiReading], category: &str) -> Option<f64> {
    mean(
        rows.iter()
            .filter(|reading| station_category(&reading.station) == category)
            .filter_map(|reading| reading.aqhi),
    )
}

fn mean(values: impl Iterator<Item = i64>) -> Option<f64> {
    let collected: Vec<i64> = values.collect();
    if collected.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let count = collected.len() as f64;
    #[allow(clippy::cast_precision_loss)]
    let sum = collected.iter().sum::<i64>() as f64;
    Some((sum / count * 10.0).round() / 10.0)
}

/// Unix hour for an ISO-8601 instant with an offset.
fn hour_from_iso(iso: &str) -> Option<i64> {
    let (date, rest) = iso.split_once('T')?;
    let day = crate::days_from_iso(date)?;
    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    Some(day * 86_400 + (hour - 8) * 3_600)
}
