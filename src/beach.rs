//! EPD beach water-quality grading.
//!
//! The Environmental Protection Department publishes the latest grading for
//! every gazetted beach as a GeoJSON FeatureCollection. Each feature's
//! `properties` carries the beach name, district, grade (1-4) and status.
//!
//! The geometry in that feed is not usable WGS84 Hong Kong coordinates (the
//! published points are far outside the territory), so it is deliberately not
//! exposed. Only the grading attributes are used.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::http;

/// Official beach grading feed.
pub const BEACH_URL: &str = "https://cd.epic.epd.gov.hk/EPICDI/json/beach/beachgrading_GeoXY?lang=en";

/// Grading changes at most once per sampling round; an hour of cache is safe.
const CACHE_TTL_SECS: u64 = 3_600;

#[derive(Debug, Deserialize)]
struct FeatureCollection {
    #[serde(default)]
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
struct Feature {
    #[serde(default)]
    properties: Properties,
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

/// One beach and its latest published grading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// Source URL.
    pub source: String,
    /// Number of beaches returned after filtering.
    pub count: usize,
    /// Beaches, sorted by district then name.
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

/// Fetch and parse the official grading feed.
///
/// # Errors
///
/// Returns [`crate::Error::Http`] for transport failures and
/// [`crate::Error::Parse`] when the payload is not a FeatureCollection.
pub fn fetch() -> Result<BeachReport> {
    let body = http::get_cached(BEACH_URL, CACHE_TTL_SECS)?;
    let collection: FeatureCollection =
        serde_json::from_str(&body).map_err(|error| crate::Error::parse(error.to_string()))?;
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
            Beach {
                name: properties.name.trim().to_string(),
                district: properties.district.trim().to_string(),
                grade,
                grade_name,
                status: properties.status.trim().to_string(),
                description: properties.desc.trim().to_string(),
                code: properties.beach_code.trim().to_string(),
            }
        })
        .collect();
    beaches.sort_by(|left, right| {
        left.district
            .cmp(&right.district)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(BeachReport {
        source: BEACH_URL.to_string(),
        count: beaches.len(),
        beaches,
    })
}

/// Keep beaches whose name or district contains `query`, case-insensitively.
pub fn retain_query(report: &mut BeachReport, query: &str) {
    let needle = query.trim().to_lowercase();
    report.beaches.retain(|beach| {
        beach.name.to_lowercase().contains(&needle)
            || beach.district.to_lowercase().contains(&needle)
    });
    report.count = report.beaches.len();
}

/// Keep beaches with a specific grade band: `good`, `fair`, `poor`,
/// `very-poor`, `graded`, `ungraded`, or `open`.
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
        return Err(crate::Error::usage(format!(
            "unknown beach grade '{band}'; use good, fair, poor, very-poor, graded, ungraded or open"
        )));
    }
    report.count = report.beaches.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},
         "properties":{"district":"Islands District","name":"Silver Mine Bay Beach",
         "grade":"1","status":"open","desc":"Grade 1 - Good","beachCode":"smb"}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},
         "properties":{"district":"Southern District","name":"Big Wave Bay Beach",
         "grade":"","status":"notgraded","desc":"","beachCode":"bwb"}}
    ]}"#;

    #[test]
    fn parses_grades_and_status() {
        let collection: FeatureCollection = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(collection.features.len(), 2);
        let graded = &collection.features[0].properties;
        assert_eq!(graded.grade, "1");
        assert_eq!(graded.beach_code, "smb");
        assert_eq!(grade_name(&graded.grade, &graded.status, &graded.desc), "Grade 1 - Good");
    }

    #[test]
    fn filters_grades() {
        let mut report = BeachReport {
            source: BEACH_URL.to_string(),
            count: 2,
            beaches: vec![
                Beach {
                    name: "Silver Mine Bay Beach".into(),
                    district: "Islands District".into(),
                    grade: Some(1),
                    grade_name: "Good".into(),
                    status: "open".into(),
                    description: "Grade 1 - Good".into(),
                    code: "smb".into(),
                },
                Beach {
                    name: "Big Wave Bay Beach".into(),
                    district: "Southern District".into(),
                    grade: None,
                    grade_name: "Not graded in winter months".into(),
                    status: "notgraded".into(),
                    description: String::new(),
                    code: "bwb".into(),
                },
            ],
        };
        retain_grade(&mut report, "good").unwrap();
        assert_eq!(report.count, 1);
        assert_eq!(report.beaches[0].name, "Silver Mine Bay Beach");
        assert!(retain_grade(&mut report, "bogus").is_err());
    }
}
