//! HKO hourly ambient gamma radiation dose rate.
//!
//! The Observatory publishes one hourly mean per monitoring station, in
//! microsievert per hour. Values are compared with the network median, not
//! against a health threshold: this reports what the stations measure, and a
//! ratio far above the median is the observable signal worth a look.

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::now_hkt_iso8601;

/// Hourly ambient gamma radiation, all stations.
pub const RADIATION_URL: &str =
    "https://data.weather.gov.hk/weatherAPI/hko_data/regional-weather/latest_hourly_rmn.csv";

/// Updated hourly; ten minutes of cache is enough.
const CACHE_TTL_SECS: u64 = 600;

/// One station's latest hourly mean.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RadiationReading {
    /// Station name, for example `Ping Chau`.
    pub station: String,
    /// Hourly mean ambient gamma dose rate, µSv/h.
    pub microsievert_per_hour: f64,
    /// Observation time, ISO-8601 with `+08:00`.
    pub time: String,
    /// Value relative to the network median.
    pub relative_to_median: Option<f64>,
    /// `typical` or `elevated` (more than twice the network median).
    pub level: String,
}

/// The `radiation` JSON payload.
#[derive(Debug, Clone, Serialize)]
pub struct RadiationReport {
    /// Newest observation time, ISO-8601 with `+08:00`.
    pub observed_at: String,
    /// Number of stations.
    pub count: usize,
    /// Median across stations, µSv/h.
    pub median_microsievert_per_hour: f64,
    /// Highest reading.
    pub max: Option<RadiationReading>,
    /// Readings, sorted by station name.
    pub stations: Vec<RadiationReading>,
    /// Source URL.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Fetch the latest hourly ambient gamma dose rate per station.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when the
/// payload carries no parsable reading.
pub fn fetch() -> Result<RadiationReport> {
    let body = http::get_cached(RADIATION_URL, CACHE_TTL_SECS)?;
    let mut report = parse(&body)?;
    report.fetched_at = now_hkt_iso8601();
    Ok(report)
}

fn parse(body: &str) -> Result<RadiationReport> {
    let (header, rows) = csv::rows(body);
    let time = csv::column(&header, "date time")
        .ok_or_else(|| Error::parse("radiation CSV has no date time column"))?;
    let station = csv::column(&header, "station")
        .ok_or_else(|| Error::parse("radiation CSV has no station column"))?;
    let value = csv::column(&header, "gamma")
        .ok_or_else(|| Error::parse("radiation CSV has no dose rate column"))?;

    let mut readings: Vec<(String, f64, String)> = Vec::new();
    for fields in rows {
        let (Some(name), Some(raw_time), Some(raw_value)) =
            (fields.get(station), fields.get(time), fields.get(value))
        else {
            continue;
        };
        let Ok(dose) = raw_value.parse::<f64>() else {
            continue;
        };
        let Some(observed) = compact_time(raw_time) else {
            continue;
        };
        if !dose.is_finite() || dose < 0.0 {
            continue;
        }
        readings.push((name.clone(), dose, observed));
    }
    if readings.is_empty() {
        return Err(Error::parse("radiation CSV has no parsable readings"));
    }
    let observed_at = readings
        .iter()
        .map(|(_, _, time)| time.clone())
        .max()
        .unwrap_or_default();
    let mut values: Vec<f64> = readings.iter().map(|(_, dose, _)| *dose).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = median(&values);
    let mut stations: Vec<RadiationReading> = readings
        .into_iter()
        .map(|(station, dose, time)| {
            let ratio = (median > 0.0).then(|| (dose / median * 100.0).round() / 100.0);
            RadiationReading {
                station,
                microsievert_per_hour: dose,
                time,
                relative_to_median: ratio,
                level: if ratio.is_some_and(|ratio| ratio > 2.0) {
                    "elevated".to_string()
                } else {
                    "typical".to_string()
                },
            }
        })
        .collect();
    stations.sort_by(|left, right| left.station.cmp(&right.station));
    let max = stations
        .iter()
        .max_by(|left, right| {
            left.microsievert_per_hour
                .partial_cmp(&right.microsievert_per_hour)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();
    Ok(RadiationReport {
        observed_at,
        count: stations.len(),
        median_microsievert_per_hour: median,
        max,
        stations,
        source: RADIATION_URL.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Keep stations whose name contains `query`, case-insensitively.
pub fn retain_query(report: &mut RadiationReport, query: &str) {
    let needle = query.trim().to_lowercase();
    report
        .stations
        .retain(|reading| reading.station.to_lowercase().contains(&needle));
    report.count = report.stations.len();
    report.max = report
        .stations
        .iter()
        .max_by(|left, right| {
            left.microsievert_per_hour
                .partial_cmp(&right.microsievert_per_hour)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();
}

fn median(sorted: &[f64]) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        ((sorted[mid - 1] + sorted[mid]) / 2.0 * 1000.0).round() / 1000.0
    } else {
        sorted[mid]
    }
}

/// `2026112201` to `2026-11-22T01:00:00+08:00`.
fn compact_time(raw: &str) -> Option<String> {
    let digits = raw.trim();
    if digits.len() != 10 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year = &digits[0..4];
    let month = &digits[4..6];
    let day = &digits[6..8];
    let hour = &digits[8..10];
    let month_number: u32 = month.parse().ok()?;
    let day_number: u32 = day.parse().ok()?;
    let hour_number: u32 = hour.parse().ok()?;
    if !(1..=12).contains(&month_number) || !(1..=31).contains(&day_number) || hour_number > 23 {
        return None;
    }
    Some(format!("{year}-{month}-{day}T{hour}:00:00+08:00"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str =
        "Date time,Station,Hourly mean ambient gamma radiation dose rate (microsievert per hour)\r
2026112201,Ping Chau,0.09\r
2026112201,Tap Mun,0.09\r
2026112201,Kat O,0.10\r
2026112201,Kwun Tong,0.30\r
";

    #[test]
    fn reads_median_and_flags_outliers() {
        let report = parse(SAMPLE).unwrap();
        assert_eq!(report.count, 4);
        assert_eq!(report.observed_at, "2026-11-22T01:00:00+08:00");
        assert_eq!(report.median_microsievert_per_hour, 0.095);
        assert_eq!(report.max.as_ref().unwrap().station, "Kwun Tong");
        let kwun = report
            .stations
            .iter()
            .find(|reading| reading.station == "Kwun Tong")
            .unwrap();
        assert_eq!(kwun.level, "elevated");
    }

    #[test]
    fn rejects_a_non_csv_body() {
        assert!(parse("Not found").is_err());
    }

    #[test]
    fn parses_compact_times() {
        assert_eq!(
            compact_time("2026112201").as_deref(),
            Some("2026-11-22T01:00:00+08:00")
        );
        assert_eq!(compact_time("2026-11-22"), None);
    }
}
