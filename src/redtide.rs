//! AFCD red tide sightings.
//!
//! The Agriculture, Fisheries and Conservation Department publishes every red
//! tide report since 1975 as a CSV. A sighting with no dissipation date is
//! still active, which is the signal a beach or marine command cannot derive
//! from water quality alone.

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::model::haversine_km;
use crate::now_hkt_iso8601;

/// Every red tide sighting, one row each.
pub const RED_TIDE_URL: &str = "https://redtide.afcd.gov.hk/data/RTMS_ob_RTLE.csv";

/// Sightings change at most a few times a week.
const CACHE_TTL_SECS: u64 = 3_600;

/// Default recent window in days.
pub const DEFAULT_DAYS: u32 = 90;

/// Largest accepted window, in days.
pub const MAX_DAYS: u32 = 3_650;

/// One red tide sighting.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sighting {
    /// AFCD sighting number, for example `1975S0001`.
    pub id: String,
    /// Date the sighting was reported, `YYYY-MM-DD`.
    pub reported_on: String,
    /// Date the red tide dissipated, `null` while still active.
    pub dissipated_on: Option<String>,
    /// Algal group, for example `Dinoflagellates`.
    pub group: String,
    /// Species, for example `Noctiluca scintillans`.
    pub species: String,
    /// Free-text location as published.
    pub location: String,
    /// WGS84 latitude, when published and inside Hong Kong.
    pub latitude: Option<f64>,
    /// WGS84 longitude, when published and inside Hong Kong.
    pub longitude: Option<f64>,
    /// Distance from `--near`, in kilometres.
    pub distance_km: Option<f64>,
    /// Whether no dissipation date has been published.
    pub active: bool,
}

/// The `red-tide` JSON payload.
#[derive(Debug, Clone, Serialize)]
pub struct RedTideReport {
    /// Recent window in days that produced `sightings`.
    pub days: u32,
    /// Sightings in the report.
    pub count: usize,
    /// How many of them are still active.
    pub active: usize,
    /// Sorted sightings: active first, then most recently reported.
    pub sightings: Vec<Sighting>,
    /// Source URL.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Fetch active and recent red tide sightings.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures, [`Error::Parse`] when the
/// payload is not the expected CSV, and [`Error::Usage`] when `days` is 0 or
/// greater than [`MAX_DAYS`].
pub fn fetch(days: u32) -> Result<RedTideReport> {
    if days == 0 || days > MAX_DAYS {
        return Err(Error::usage(format!(
            "--days must be between 1 and {MAX_DAYS}"
        )));
    }
    let body = http::get_cached(RED_TIDE_URL, CACHE_TTL_SECS)?;
    let mut report = parse(&body, days)?;
    report.fetched_at = now_hkt_iso8601();
    Ok(report)
}

fn parse(body: &str, days: u32) -> Result<RedTideReport> {
    let (header, rows) = csv::rows(body);
    let id = csv::column(&header, "sighting no");
    let reported = csv::column(&header, "date of report")
        .ok_or_else(|| Error::parse("red tide CSV has no report date column"))?;
    let dissipated = csv::column(&header, "date of dissipation");
    let group = csv::column(&header, "group");
    let species = csv::column(&header, "species");
    let location = csv::column(&header, "location");
    let latitude = csv::column(&header, "latitude");
    let longitude = csv::column(&header, "longitude");

    let cutoff = crate::hkt_day_number() - i64::from(days);
    let mut sightings = Vec::new();
    for fields in rows {
        let Some(reported_on) = fields.get(reported).and_then(|raw| csv::iso_date(raw)) else {
            continue;
        };
        let dissipated_on = dissipated
            .and_then(|index| fields.get(index))
            .and_then(|raw| csv::iso_date(raw));
        let active = dissipated_on.is_none();
        if !active {
            let Some(day) = crate::days_from_iso(&reported_on) else {
                continue;
            };
            if day < cutoff {
                continue;
            }
        }
        let (lat, lng) = match (
            latitude
                .and_then(|index| fields.get(index))
                .and_then(|raw| raw.parse().ok()),
            longitude
                .and_then(|index| fields.get(index))
                .and_then(|raw| raw.parse().ok()),
        ) {
            (Some(lat), Some(lng)) if plausible(lat, lng) => (Some(lat), Some(lng)),
            _ => (None, None),
        };
        sightings.push(Sighting {
            id: id
                .and_then(|index| fields.get(index))
                .cloned()
                .unwrap_or_default(),
            reported_on,
            dissipated_on,
            group: group
                .and_then(|index| fields.get(index))
                .cloned()
                .unwrap_or_default(),
            species: species
                .and_then(|index| fields.get(index))
                .cloned()
                .unwrap_or_default(),
            location: location
                .and_then(|index| fields.get(index))
                .cloned()
                .unwrap_or_default(),
            latitude: lat,
            longitude: lng,
            distance_km: None,
            active,
        });
    }
    sightings.sort_by(|left, right| {
        right
            .active
            .cmp(&left.active)
            .then_with(|| right.reported_on.cmp(&left.reported_on))
    });
    let active = sightings.iter().filter(|sighting| sighting.active).count();
    Ok(RedTideReport {
        days,
        count: sightings.len(),
        active,
        sightings,
        source: RED_TIDE_URL.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

fn plausible(lat: f64, lng: f64) -> bool {
    (22.1..=22.6).contains(&lat) && (113.8..=114.5).contains(&lng)
}

/// Sort sightings by distance from `(lat, lng)`, nearest first.
///
/// Sightings without a published point sort last, active ones first among
/// equals.
pub fn sort_by_distance(report: &mut RedTideReport, lat: f64, lng: f64) {
    for sighting in &mut report.sightings {
        sighting.distance_km = match (sighting.latitude, sighting.longitude) {
            (Some(s_lat), Some(s_lng)) => {
                Some((haversine_km(lat, lng, s_lat, s_lng) * 10.0).round() / 10.0)
            }
            _ => None,
        };
    }
    report
        .sightings
        .sort_by(|left, right| match (left.distance_km, right.distance_km) {
            (Some(a), Some(b)) => a
                .partial_cmp(&b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.active.cmp(&left.active)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => right
                .active
                .cmp(&left.active)
                .then_with(|| right.reported_on.cmp(&left.reported_on)),
        });
}

/// Keep only sightings still active.
pub fn retain_active(report: &mut RedTideReport) {
    report.sightings.retain(|sighting| sighting.active);
    report.count = report.sightings.len();
    report.active = report.count;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}Red Tide Sighting No.,Date of Report,Date of Dissipation,Group,Species,Location,Latitude,Longitude,Coordinate System\r
1975S0001,1975-08-06,1975-08-11,Dinoflagellates,Noctiluca scintillans,\"Hap Mun Bay, Clear Water Bay\",22.2837,114.2953,WGS84\r
2026S0001,2026-11-01,,Diatoms,Thalassiosira sp.,Tolo Harbour,22.4300,114.1900,WGS84\r
2026S0002,2026-10-01,2026-10-03,Dinoflagellates,Karenia papilionacea,Silver Mine Bay,,,WGS84\r
";

    #[test]
    fn keeps_active_and_recent_only() {
        let report = parse(SAMPLE, 90).unwrap();
        assert_eq!(report.active, 1);
        assert_eq!(
            report.sightings[0].location, "Tolo Harbour",
            "active sightings sort first"
        );
        assert_eq!(report.sightings[0].latitude, Some(22.43));
    }

    #[test]
    fn sorts_by_distance() {
        let mut report = parse(SAMPLE, 90).unwrap();
        sort_by_distance(&mut report, 22.29, 114.30);
        assert_eq!(report.sightings[0].location, "Tolo Harbour");
        assert!(report.sightings[0].distance_km.unwrap() > 0.0);
    }

    #[test]
    fn rejects_a_non_csv_body() {
        assert!(parse("Invalid Access", 30).is_err());
    }
}
