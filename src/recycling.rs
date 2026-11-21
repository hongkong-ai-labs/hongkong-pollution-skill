//! Waste Reduction and Recycling: recyclable collection points.
//!
//! The Environmental Protection Department publishes every collection point
//! with coordinates, accepted waste types, opening hours and notes as one CSV.
//! `recycling` answers the only question that matters at street level: what is
//! near me, and what does it take.

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::model::haversine_km;
use crate::now_hkt_iso8601;

/// Stable download path for the collection-point CSV.
pub const RECYCLING_URL: &str =
    "https://www.wastereduction.gov.hk/sites/default/files/wasteless.csv";

/// The list changes slowly; a day of cache is plenty.
const CACHE_TTL_SECS: u64 = 86_400;

/// Default number of points returned.
pub const DEFAULT_LIMIT: usize = 5;

/// Largest accepted `--limit`.
pub const MAX_LIMIT: usize = 50;

/// One collection point.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Point {
    /// Identifier in the published file.
    pub id: String,
    /// English address or location name.
    pub name: String,
    /// District, with underscores replaced by spaces.
    pub district: String,
    /// Accepted waste types, as published.
    pub waste_types: Vec<String>,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// Distance from the queried coordinate, in kilometres.
    pub distance_km: f64,
    /// Opening hours, when published.
    pub opening_hours: Option<String>,
    /// Access note, when published.
    pub note: Option<String>,
}

/// The `recycling` payload.
#[derive(Debug, Clone, Serialize)]
pub struct RecyclingReport {
    /// Queried latitude.
    pub lat: f64,
    /// Queried longitude.
    pub lng: f64,
    /// Waste type filter, when one was applied.
    pub waste_type: Option<String>,
    /// Points returned after filtering, nearest first.
    pub count: usize,
    /// Nearest points.
    pub points: Vec<Point>,
    /// Source URL.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Find collection points near `(lat, lng)`, nearest first.
///
/// `waste_type` matches case-insensitively against the published accepted
/// types, so `plastic` finds `Plastics` and `Plastic Bottle`.
///
/// # Errors
///
/// Returns [`Error::Usage`] for a limit outside 1..=[`MAX_LIMIT`], the usual
/// HTTP/parse errors, and [`Error::NoMatch`] when nothing is in range.
pub fn near(lat: f64, lng: f64, limit: usize, waste_type: Option<&str>) -> Result<RecyclingReport> {
    if limit == 0 || limit > MAX_LIMIT {
        return Err(Error::usage(format!(
            "--limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    if !(22.1..=22.6).contains(&lat) || !(113.8..=114.5).contains(&lng) {
        return Err(Error::usage(format!(
            "coordinates must be inside Hong Kong, got {lat},{lng}"
        )));
    }
    let body = http::get_cached(RECYCLING_URL, CACHE_TTL_SECS)?;
    let mut points = parse(&body)?
        .into_iter()
        .filter(|point| {
            waste_type.is_none_or(|needle| {
                point
                    .waste_types
                    .iter()
                    .any(|kind| kind.to_lowercase().contains(&needle.trim().to_lowercase()))
            })
        })
        .map(|point| {
            let distance = haversine_km(lat, lng, point.latitude, point.longitude);
            Point {
                distance_km: (distance * 10.0).round() / 10.0,
                ..point
            }
        })
        .collect::<Vec<Point>>();
    if points.is_empty() {
        return Err(Error::no_match(match waste_type {
            Some(kind) => format!("no collection point near {lat},{lng} takes {kind:?}"),
            None => format!("no collection point near {lat},{lng}"),
        }));
    }
    points.sort_by(|left, right| {
        left.distance_km
            .partial_cmp(&right.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points.truncate(limit);
    Ok(RecyclingReport {
        lat,
        lng,
        waste_type: waste_type.map(str::to_string),
        count: points.len(),
        points,
        source: RECYCLING_URL.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

fn parse(body: &str) -> Result<Vec<Point>> {
    let (header, rows) = csv::rows(body);
    let lat = csv::column(&header, "lat").ok_or_else(|| Error::parse("no latitude column"))?;
    let lng = csv::column(&header, "lgt")
        .or_else(|| csv::column(&header, "lng"))
        .ok_or_else(|| Error::parse("no longitude column"))?;
    let id = csv::column(&header, "cp_id");
    let address = csv::column(&header, "address_en");
    let address2 = csv::column(&header, "address2_en");
    let district = csv::column(&header, "district_id");
    let waste =
        csv::column(&header, "waste_type").ok_or_else(|| Error::parse("no waste_type column"))?;
    let hours = csv::column(&header, "openhour_en");
    let note = csv::column(&header, "legend");
    let state = csv::column(&header, "cp_state");

    let mut points = Vec::new();
    for fields in rows {
        if state
            .and_then(|index| fields.get(index))
            .is_some_and(|value| !value.is_empty() && !value.eq_ignore_ascii_case("Accepted"))
        {
            continue;
        }
        let (Some(latitude), Some(longitude)) = (
            fields.get(lat).and_then(|raw| raw.parse::<f64>().ok()),
            fields.get(lng).and_then(|raw| raw.parse::<f64>().ok()),
        ) else {
            continue;
        };
        if !(22.1..=22.6).contains(&latitude) || !(113.8..=114.5).contains(&longitude) {
            continue;
        }
        let name = [address, address2]
            .into_iter()
            .flatten()
            .filter_map(|index| fields.get(index))
            .filter(|value| !value.is_empty())
            .cloned()
            .collect::<Vec<String>>()
            .join(", ");
        if name.is_empty() {
            continue;
        }
        points.push(Point {
            id: id
                .and_then(|index| fields.get(index))
                .cloned()
                .unwrap_or_default(),
            name,
            district: district
                .and_then(|index| fields.get(index))
                .map(|value| value.replace('_', " "))
                .unwrap_or_default(),
            waste_types: fields
                .get(waste)
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|kind| !kind.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            latitude,
            longitude,
            distance_km: 0.0,
            opening_hours: hours
                .and_then(|index| fields.get(index))
                .filter(|value| !value.is_empty())
                .cloned(),
            note: note
                .and_then(|index| fields.get(index))
                .filter(|value| !value.is_empty())
                .cloned(),
        });
    }
    if points.is_empty() {
        return Err(Error::parse(
            "recycling CSV has no usable collection points",
        ));
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}cp_id,cp_state,district_id,address_en,address2_en,lat,lgt,waste_type,legend,openhour_en\r
\"1\",\"Accepted\",\"Kwun_Tong\",\"Tai Wong Ha Resite Village RCP\",\"\",\"22.352730\",\"114.101100\",\"Metals,Paper,Plastics\",\"Recycling Bins at Public Place\",\"\"\r
\"2\",\"Removed\",\"Kwun_Tong\",\"Old Site\",\"\",\"22.350000\",\"114.100000\",\"Glass Bottles\",\"\",\"\"\r
";

    #[test]
    fn parses_points_and_skips_removed_sites() {
        let points = parse(SAMPLE).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].district, "Kwun Tong");
        assert_eq!(
            points[0].waste_types,
            vec!["Metals", "Paper", "Plastics"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_a_non_csv_body() {
        assert!(parse("Not found").is_err());
    }
}
