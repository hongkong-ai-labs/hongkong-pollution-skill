//! EPD beach water-quality grading and E. coli monitoring.
//!
//! Two EPD feeds are joined by beach name:
//!
//! - the grading GeoJSON ([`BEACH_URL`]) carries the latest grade, status,
//!   district and a usable WGS84 point for every gazetted beach;
//! - the yearly monitoring CSV ([`ecoli_url`]) carries the E. coli count per
//!   100 mL for each sampling date.
//!
//! The CSV lags the grading feed: EPD publishes a year's file only after the
//! year ends, so a beach graded today is joined to the most recent published
//! sampling history. The report names the file it used in `ecoli_source` and
//! never fails just because the CSV is missing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http;
use crate::model::haversine_km;

/// Official beach grading feed, with WGS84 geometry.
pub const BEACH_URL: &str = "https://cd.epic.epd.gov.hk/EPICDI/json/beach/beachgrading?lang=en";

/// Grading changes at most once per sampling round; an hour of cache is safe.
const CACHE_TTL_SECS: u64 = 3_600;

/// The yearly E. coli file changes at most once per year.
const ECOLI_CACHE_TTL_SECS: u64 = 21_600;

/// Sampling dates kept per beach, newest first.
///
/// EPD grades a beach on the geometric mean of the five most recent sampling
/// occasions, so five is also the window used for
/// [`EcoliSummary::geometric_mean_per_100ml`].
pub const RECENT_SAMPLES: usize = 5;

/// Hong Kong latitude bounds, matching the `street` and `--near` checks.
const HK_LAT: (f64, f64) = (22.1, 22.6);
/// Hong Kong longitude bounds.
const HK_LNG: (f64, f64) = (113.8, 114.5);

/// Yearly E. coli monitoring file for `year`.
#[must_use]
pub fn ecoli_url(year: u16) -> String {
    format!("https://cd.epic.epd.gov.hk/beachpsi/en/beach-historical-data-{year}-en.csv")
}

#[derive(Debug, Deserialize)]
struct FeatureCollection {
    #[serde(default)]
    features: Vec<Feature>,
}

#[derive(Debug, Default, Deserialize)]
struct Feature {
    #[serde(default)]
    geometry: Geometry,
    #[serde(default)]
    properties: Properties,
}

#[derive(Debug, Default, Deserialize)]
struct Geometry {
    #[serde(default)]
    coordinates: Vec<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct Properties {
    #[serde(default)]
    district: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    grade: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    desc: String,
    #[serde(default, rename = "beachCode")]
    beach_code: String,
}

/// One E. coli measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EcoliSample {
    /// Sampling date, `YYYY-MM-DD`.
    pub sampled_on: String,
    /// E. coli count per 100 mL.
    pub per_100ml: u32,
}

/// E. coli history for one beach, from the most recent published year.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EcoliSummary {
    /// Date of the newest sample, `YYYY-MM-DD`.
    pub latest_sampled_on: String,
    /// Newest E. coli count per 100 mL.
    pub latest_per_100ml: u32,
    /// Geometric mean over the [`RECENT_SAMPLES`] most recent samples, the same
    /// window EPD grades on (Good ≤ 24, Fair 25-180, Poor 181-610,
    /// Very Poor > 610).
    pub geometric_mean_per_100ml: Option<f64>,
    /// Samples in the yearly file for this beach.
    pub samples: usize,
    /// Up to [`RECENT_SAMPLES`] newest samples, newest first.
    pub recent: Vec<EcoliSample>,
}

/// One beach and its latest published grading.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Beach {
    /// Beach name as published by EPD.
    pub name: String,
    /// EPD district, for example `Islands District`.
    pub district: String,
    /// `1`-`4` when graded, otherwise `None`.
    pub grade: Option<u8>,
    /// Human-readable grade, for example `Good` or `Not graded in winter months`.
    pub grade_name: String,
    /// Raw status: `open`, `notopen`, `notgraded`, `closed`, `planned`.
    pub status: String,
    /// Short published description, for example `Grade 1 - Good`.
    pub description: String,
    /// EPD beach code.
    pub code: String,
    /// WGS84 latitude, when the feed publishes a usable point.
    pub latitude: Option<f64>,
    /// WGS84 longitude, when the feed publishes a usable point.
    pub longitude: Option<f64>,
    /// Distance from the `--near` coordinate, in kilometres.
    pub distance_km: Option<f64>,
    /// E. coli history, when the yearly monitoring file covers this beach.
    pub ecoli: Option<EcoliSummary>,
}

impl Beach {
    /// Whether the beach is currently graded.
    #[must_use]
    pub fn is_graded(&self) -> bool {
        self.grade.is_some()
    }

    /// Whether swimming is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.status == "open"
    }
}

/// Latest grading for every beach EPD publishes.
#[derive(Debug, Clone, Serialize)]
pub struct BeachReport {
    /// Grading feed URL.
    pub source: String,
    /// Number of beaches returned after filtering.
    pub count: usize,
    /// Beaches carrying a grade of 1-4.
    pub graded: usize,
    /// Beaches open for swimming.
    pub open: usize,
    /// Yearly E. coli file used, or `null` when none could be read.
    pub ecoli_source: Option<String>,
    /// Why the E. coli file could not be read, when it could not.
    pub ecoli_error: Option<String>,
    /// Beaches, sorted by district then name (by distance with `--near`).
    pub beaches: Vec<Beach>,
}

fn grade_name(grade: &str, status: &str, desc: &str) -> String {
    if !desc.is_empty() {
        return desc.to_string();
    }
    match grade {
        "1" => "Good".to_string(),
        "2" => "Fair".to_string(),
        "3" => "Poor".to_string(),
        "4" => "Very Poor".to_string(),
        _ => match status {
            "notgraded" => "Not graded in winter months".to_string(),
            "notopen" => "Beach not open for swimming".to_string(),
            "closed" => "Closed beach".to_string(),
            "planned" => "A planned public beach".to_string(),
            _ => "Not graded".to_string(),
        },
    }
}

/// Fetch the grading feed and join it to the yearly E. coli file.
///
/// A missing or unreadable E. coli file is reported in `ecoli_error`; only a
/// failure of the grading feed itself is an error.
///
/// # Errors
///
/// Returns [`crate::Error::Http`] for transport failures and
/// [`crate::Error::Parse`] when the payload is not a FeatureCollection.
pub fn fetch() -> Result<BeachReport> {
    let body = http::get_cached(BEACH_URL, CACHE_TTL_SECS)?;
    let mut report = parse_grading(&body)?;
    match fetch_ecoli() {
        Ok((url, history)) => {
            for beach in &mut report.beaches {
                beach.ecoli = history
                    .get(&normalise_name(&beach.name))
                    .and_then(|samples| summarise(samples));
            }
            report.ecoli_source = Some(url);
        }
        Err(error) => report.ecoli_error = Some(error.to_string()),
    }
    report.graded = report
        .beaches
        .iter()
        .filter(|beach| beach.is_graded())
        .count();
    report.open = report
        .beaches
        .iter()
        .filter(|beach| beach.is_open())
        .count();
    Ok(report)
}

/// Parse the grading GeoJSON without any E. coli join.
fn parse_grading(body: &str) -> Result<BeachReport> {
    let collection: FeatureCollection =
        serde_json::from_str(body).map_err(|error| Error::parse(error.to_string()))?;
    let mut beaches: Vec<Beach> = collection
        .features
        .into_iter()
        .filter(|feature| !feature.properties.name.trim().is_empty())
        .map(|feature| {
            let properties = feature.properties;
            let grade = match properties.grade.trim() {
                "1" => Some(1),
                "2" => Some(2),
                "3" => Some(3),
                "4" => Some(4),
                _ => None,
            };
            let grade_name = grade_name(&properties.grade, &properties.status, &properties.desc);
            let (latitude, longitude) = coordinates(&feature.geometry.coordinates);
            Beach {
                name: properties.name.trim().to_string(),
                district: properties.district.trim().to_string(),
                grade,
                grade_name,
                status: properties.status.trim().to_string(),
                description: properties.desc.trim().to_string(),
                code: properties.beach_code.trim().to_string(),
                latitude,
                longitude,
                distance_km: None,
                ecoli: None,
            }
        })
        .collect();
    beaches.sort_by(|left, right| {
        left.district
            .cmp(&right.district)
            .then_with(|| left.name.cmp(&right.name))
    });
    let graded = beaches.iter().filter(|beach| beach.is_graded()).count();
    let open = beaches.iter().filter(|beach| beach.is_open()).count();
    Ok(BeachReport {
        source: BEACH_URL.to_string(),
        count: beaches.len(),
        graded,
        open,
        ecoli_source: None,
        ecoli_error: None,
        beaches,
    })
}

/// Keep the point only when it is a plausible Hong Kong WGS84 coordinate.
fn coordinates(raw: &[f64]) -> (Option<f64>, Option<f64>) {
    let (Some(&lng), Some(&lat)) = (raw.first(), raw.get(1)) else {
        return (None, None);
    };
    if !(HK_LAT.0..=HK_LAT.1).contains(&lat) || !(HK_LNG.0..=HK_LNG.1).contains(&lng) {
        return (None, None);
    }
    (
        Some((lat * 1e6).round() / 1e6),
        Some((lng * 1e6).round() / 1e6),
    )
}

/// The newest yearly E. coli file that should be complete and published.
#[must_use]
pub fn latest_published_year() -> u16 {
    current_hkt_year().saturating_sub(1)
}

fn current_hkt_year() -> u16 {
    crate::now_hkt_iso8601()
        .get(0..4)
        .and_then(|year| year.parse().ok())
        .unwrap_or(2_026)
}

/// Read the newest usable yearly E. coli file, trying the current year first.
fn fetch_ecoli() -> Result<(String, BTreeMap<String, Vec<EcoliSample>>)> {
    let year = current_hkt_year();
    let mut last_error = None;
    for candidate in [year, year.saturating_sub(1)] {
        let url = ecoli_url(candidate);
        match http::get_cached(&url, ECOLI_CACHE_TTL_SECS).and_then(|body| parse_ecoli_csv(&body)) {
            Ok(history) if !history.is_empty() => return Ok((url, history)),
            Ok(_) => last_error = Some(format!("{url} has no sampling rows")),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(Error::parse(last_error.unwrap_or_else(|| {
        "no beach E. coli file available".to_string()
    })))
}

/// Parse `Beach Name, Sampling Date (DD/MM/YYYY), E. coli count per 100mL`.
///
/// The feed answers unpublished years with an HTTP 200 "Page Not Found" body,
/// so the header is checked before any row is trusted.
fn parse_ecoli_csv(body: &str) -> Result<BTreeMap<String, Vec<EcoliSample>>> {
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().unwrap_or_default();
    if !header.contains("Beach Name") || !header.contains("E. coli") {
        return Err(Error::parse(format!(
            "not an EPD beach E. coli CSV: {:?}",
            header.chars().take(60).collect::<String>()
        )));
    }
    let mut history: BTreeMap<String, Vec<EcoliSample>> = BTreeMap::new();
    for line in lines {
        let fields = split_csv(line);
        let (Some(name), Some(date), Some(count)) = (fields.first(), fields.get(1), fields.get(2))
        else {
            continue;
        };
        let Some(sampled_on) = iso_date(date) else {
            continue;
        };
        let Ok(per_100ml) = count.trim().parse::<u32>() else {
            continue;
        };
        history
            .entry(normalise_name(name))
            .or_default()
            .push(EcoliSample {
                sampled_on,
                per_100ml,
            });
    }
    for samples in history.values_mut() {
        samples.sort_by(|left, right| left.sampled_on.cmp(&right.sampled_on));
    }
    Ok(history)
}

/// Split one CSV line, honouring double quotes and doubled quote escapes.
fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            other => field.push(other),
        }
    }
    fields.push(field);
    fields
}

/// `29/12/2025` to `2025-12-29`.
fn iso_date(raw: &str) -> Option<String> {
    let mut parts = raw.trim().split('/');
    let day: u32 = parts.next()?.trim().parse().ok()?;
    let month: u32 = parts.next()?.trim().parse().ok()?;
    let year: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !(1..=31).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Lower-case, trim and collapse whitespace so both feeds match on a name.
fn normalise_name(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn summarise(samples: &[EcoliSample]) -> Option<EcoliSummary> {
    let latest = samples.last()?;
    let window = &samples[samples.len().saturating_sub(RECENT_SAMPLES)..];
    let values: Vec<f64> = window
        .iter()
        .filter(|sample| sample.per_100ml > 0)
        .map(|sample| f64::from(sample.per_100ml))
        .collect();
    let geometric_mean = if values.is_empty() {
        None
    } else {
        #[allow(clippy::cast_precision_loss)]
        let count = values.len() as f64;
        let mean_ln = values.iter().map(|value| value.ln()).sum::<f64>() / count;
        Some((mean_ln.exp() * 10.0).round() / 10.0)
    };
    Some(EcoliSummary {
        latest_sampled_on: latest.sampled_on.clone(),
        latest_per_100ml: latest.per_100ml,
        geometric_mean_per_100ml: geometric_mean,
        samples: samples.len(),
        recent: window.iter().rev().cloned().collect(),
    })
}

/// Keep beaches whose name or district contains `query`, case-insensitively.
pub fn retain_query(report: &mut BeachReport, query: &str) {
    let needle = query.trim().to_lowercase();
    report.beaches.retain(|beach| {
        beach.name.to_lowercase().contains(&needle)
            || beach.district.to_lowercase().contains(&needle)
    });
    refresh_counts(report);
}

/// Sort beaches by distance from `(lat, lng)`, nearest first.
///
/// Beaches without a published point sort last.
pub fn sort_by_distance(report: &mut BeachReport, lat: f64, lng: f64) {
    for beach in &mut report.beaches {
        beach.distance_km = match (beach.latitude, beach.longitude) {
            (Some(beach_lat), Some(beach_lng)) => {
                Some((haversine_km(lat, lng, beach_lat, beach_lng) * 10.0).round() / 10.0)
            }
            _ => None,
        };
    }
    report
        .beaches
        .sort_by(|left, right| match (left.distance_km, right.distance_km) {
            (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.name.cmp(&right.name),
        });
}

/// Keep beaches with a specific grade band: `good`, `fair`, `poor`,
/// `very-poor`, `graded`, `ungraded`, or `open`.
///
/// # Errors
///
/// Returns [`crate::Error::Usage`] for an unknown band.
pub fn retain_grade(report: &mut BeachReport, band: &str) -> Result<()> {
    let band = band.trim().to_lowercase().replace('_', "-");
    report.beaches.retain(|beach| match band.as_str() {
        "good" | "1" => beach.grade == Some(1),
        "fair" | "2" => beach.grade == Some(2),
        "poor" | "3" => beach.grade == Some(3),
        "very-poor" | "4" => beach.grade == Some(4),
        "graded" => beach.is_graded(),
        "ungraded" | "not-graded" => !beach.is_graded(),
        "open" => beach.is_open(),
        _ => false,
    });
    if report.beaches.is_empty()
        && !matches!(
            band.as_str(),
            "good"
                | "1"
                | "fair"
                | "2"
                | "poor"
                | "3"
                | "very-poor"
                | "4"
                | "graded"
                | "ungraded"
                | "not-graded"
                | "open"
        )
    {
        return Err(Error::usage(format!(
            "unknown beach grade '{band}'; use good, fair, poor, very-poor, graded, ungraded or open"
        )));
    }
    refresh_counts(report);
    Ok(())
}

fn refresh_counts(report: &mut BeachReport) {
    report.count = report.beaches.len();
    report.graded = report
        .beaches
        .iter()
        .filter(|beach| beach.is_graded())
        .count();
    report.open = report
        .beaches
        .iter()
        .filter(|beach| beach.is_open())
        .count();
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"Point","coordinates":[114.3722,22.2298]},
         "properties":{"district":"Islands District","name":"Silver Mine Bay Beach",
         "grade":"1","status":"open","desc":"Grade 1 - Good","beachCode":"smb"}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[105.31,-156.267]},
         "properties":{"district":"Southern District","name":"Big Wave Bay Beach",
         "grade":"","status":"notgraded","desc":"","beachCode":"bwb"}}
    ]}"#;

    const ECOLI: &str = "\u{feff}\"Beach Name\",\"Sampling Date (DD/MM/YYYY)\",\"E. coli count per 100mL on the sampling date\"\r
\"Silver Mine Bay Beach\",\"08/01/2025\",\"10\"\r
\"Silver Mine Bay Beach\",\"15/01/2025\",\"40\"\r
\"Silver Mine Bay Beach\",\"22/01/2025\",\"0\"\r
\"Big Wave Bay Beach\",\"08/01/2025\",\"160\"\r
";

    #[test]
    fn parses_grades_and_coordinates() {
        let report = parse_grading(SAMPLE).unwrap();
        assert_eq!(report.count, 2);
        assert_eq!(report.graded, 1);
        assert_eq!(report.open, 1);
        let graded = &report.beaches[0];
        assert_eq!(graded.name, "Silver Mine Bay Beach");
        assert_eq!(graded.grade, Some(1));
        assert_eq!(graded.latitude, Some(22.2298));
        assert_eq!(graded.longitude, Some(114.3722));
        // The published point for the second beach is outside Hong Kong.
        assert_eq!(report.beaches[1].latitude, None);
        assert_eq!(
            grade_name(&graded.description, &graded.status, &graded.description),
            "Grade 1 - Good"
        );
    }

    #[test]
    fn parses_ecoli_and_joins_by_name() {
        let history = parse_ecoli_csv(ECOLI).unwrap();
        let samples = history.get("silver mine bay beach").unwrap();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].sampled_on, "2025-01-08");
        let summary = summarise(samples).unwrap();
        assert_eq!(summary.latest_sampled_on, "2025-01-22");
        assert_eq!(summary.latest_per_100ml, 0);
        assert_eq!(summary.samples, 3);
        // (10 * 40)^(1/2) = 20, the zero sample is excluded.
        assert_eq!(summary.geometric_mean_per_100ml, Some(20.0));
        assert_eq!(summary.recent[0].sampled_on, "2025-01-22");
    }

    #[test]
    fn rejects_a_non_csv_body() {
        assert!(parse_ecoli_csv("Invalid Access / Page Not Found!").is_err());
    }

    #[test]
    fn sorts_by_distance() {
        let mut report = parse_grading(SAMPLE).unwrap();
        sort_by_distance(&mut report, 22.23, 114.37);
        assert_eq!(report.beaches[0].name, "Silver Mine Bay Beach");
        assert!(report.beaches[0].distance_km.unwrap() < 1.0);
        assert_eq!(report.beaches[1].distance_km, None);
    }

    #[test]
    fn filters_grades() {
        let mut report = parse_grading(SAMPLE).unwrap();
        retain_grade(&mut report, "good").unwrap();
        assert_eq!(report.count, 1);
        assert_eq!(report.beaches[0].name, "Silver Mine Bay Beach");
        assert!(retain_grade(&mut report, "bogus").is_err());
    }

    #[test]
    fn splits_quoted_csv_fields() {
        let fields = split_csv("\"Anglers' Beach\",\"08/01/2025\",\"7\"");
        assert_eq!(fields, vec!["Anglers' Beach", "08/01/2025", "7"]);
    }
}
