//! `hkpollution doctor`: one live probe of every upstream plus a shape check.
//!
//! The probes are plain data ([`Probe`]) so the whole report can be built
//! against a local stub in tests. Nothing here reads or writes the cache: the
//! point is to see what the sources are serving right now.

use std::time::Instant;

use serde::Serialize;
use serde_json::Value;

use crate::error::Error;
use crate::{
    VERSION, aqhi, aqhi24, beach, cache, epd_app, http, now_hkt_iso8601, parse, pollutants, praise,
    radiation, redtide, uv, water, weather,
};

/// Representative coordinate used by the PRAISE-HK probes (Tsim Sha Tsui).
const PROBE_LAT: f64 = 22.3024;
/// Representative longitude used by the PRAISE-HK probes.
const PROBE_LNG: f64 = 114.1747;

/// What a healthy payload must contain.
#[derive(Debug, Clone, Copy)]
pub enum Schema {
    /// A top-level JSON array whose first object carries this key.
    FirstItem(&'static str),
    /// A JSON object carrying every one of these top-level keys.
    Keys(&'static [&'static str]),
    /// A text or XML payload containing every one of these element names.
    Elements(&'static [&'static str]),
    /// A text payload containing this literal marker.
    Contains(&'static str),
}

/// One source to probe.
#[derive(Debug, Clone)]
pub struct Probe {
    /// Label shown in the `SOURCE` column and the JSON `source` field.
    pub source: &'static str,
    /// URL fetched once, with retries.
    pub url: String,
    /// Minimum shape the body must have.
    pub schema: Schema,
    /// How old the newest timestamp inside the body may be before the source
    /// is reported as stale. `None` skips the check.
    pub max_age_secs: Option<u64>,
}

/// One row of the report.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Source label.
    pub source: String,
    /// `ok`, `fail:<reason>` or `schema:<missing key>`.
    pub status: String,
    /// Round-trip time in milliseconds.
    pub ms: u64,
    /// Short human-readable note.
    pub detail: String,
    /// Age of the newest timestamp in the payload, in hours.
    pub age_hours: Option<f64>,
}

/// Counts over [`Check`]s.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Summary {
    /// Checks with status `ok`.
    pub ok: usize,
    /// Checks that answered with a payload older than allowed.
    pub stale: usize,
    /// Checks that failed or mismatched.
    pub failed: usize,
}

/// Cache location and size, as reported by `doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct CacheInfo {
    /// Cache directory, `null` when the platform gives us none.
    pub dir: Option<String>,
    /// Number of cached bodies.
    pub entries: usize,
    /// Total size of the cache files.
    pub bytes: u64,
}

/// The `doctor --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Every probe, in the order it was made.
    pub checks: Vec<Check>,
    /// Pass/fail counts.
    pub summary: Summary,
    /// Crate version.
    pub version: String,
    /// Cache location and size.
    pub cache: CacheInfo,
    /// When the probes ran, in Hong Kong time.
    pub fetched_at: String,
}

impl Report {
    /// Whether every probe passed and served fresh data.
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.summary.failed == 0 && self.summary.stale == 0
    }
}

/// Every source the doctor knows how to check.
#[must_use]
pub fn default_probes() -> Vec<Probe> {
    vec![
        Probe {
            source: "data.gov.hk AQHI",
            url: aqhi::AQHI_JSON_URL.to_string(),
            schema: Schema::FirstItem("station"),
            max_age_secs: Some(2 * 3_600),
        },
        Probe {
            source: "aqhi.gov.hk RSS",
            url: aqhi::AQHI_RSS_URL.to_string(),
            schema: Schema::Elements(&["item"]),
            max_age_secs: Some(2 * 3_600),
        },
        Probe {
            source: "aqhi.gov.hk 24pc",
            url: pollutants::POLLUTANTS_URL.to_string(),
            schema: Schema::Elements(&["PollutantConcentration"]),
            max_age_secs: Some(2 * 3_600),
        },
        Probe {
            source: "HKO rhrread",
            url: weather::WEATHER_URL.to_string(),
            schema: Schema::Keys(&["temperature", "humidity"]),
            max_age_secs: Some(2 * 3_600),
        },
        Probe {
            source: "HKO fnd",
            url: weather::FORECAST_URL.to_string(),
            schema: Schema::Keys(&["weatherForecast"]),
            max_age_secs: Some(21_600),
        },
        Probe {
            source: "EPD app forecast",
            url: epd_app::FORECAST_URL.to_string(),
            schema: Schema::Keys(&["general_stations"]),
            max_age_secs: Some(86_400),
        },
        Probe {
            source: "EPD app monitors",
            url: epd_app::MONITORS_URL.to_string(),
            schema: Schema::FirstItem("id"),
            max_age_secs: Some(3_600),
        },
        Probe {
            source: "PRAISE weather",
            url: praise::weather_url(PROBE_LAT, PROBE_LNG),
            schema: Schema::Keys(&["temperature", "humidity"]),
            max_age_secs: Some(3_600),
        },
        Probe {
            source: "PRAISE pollutant_index",
            url: praise::pollutant_index_url(PROBE_LAT, PROBE_LNG),
            schema: Schema::Keys(&["AQHIBN"]),
            max_age_secs: Some(3_600),
        },
        Probe {
            source: "EPD beach grading",
            url: beach::BEACH_URL.to_string(),
            schema: Schema::Keys(&["features"]),
            max_age_secs: Some(7 * 86_400),
        },
        Probe {
            source: "EPD beach E. coli",
            url: beach::ecoli_url(beach::latest_published_year()),
            schema: Schema::Contains("E. coli"),
            max_age_secs: Some(400 * 86_400),
        },
        Probe {
            source: "EPD river water quality",
            url: water::RIVER_URL.to_string(),
            schema: Schema::Contains("Dissolved Oxygen"),
            max_age_secs: Some(60 * 86_400),
        },
        Probe {
            source: "EPD marine water quality",
            url: water::MARINE_URL.to_string(),
            schema: Schema::Contains("Dissolved Oxygen"),
            max_age_secs: Some(60 * 86_400),
        },
        Probe {
            source: "EPD 24-hour AQHI",
            url: aqhi24::AQHI_24H_URL.to_string(),
            schema: Schema::Elements(&["StationName"]),
            max_age_secs: Some(2 * 3_600),
        },
        Probe {
            source: "HKO UV index",
            url: uv::UV_URL.to_string(),
            schema: Schema::Contains("UV Index"),
            // UV is not published overnight, so the newest reading is up to a
            // day old before the first daylight update.
            max_age_secs: Some(24 * 3_600),
        },
        Probe {
            source: "HKO radiation",
            url: radiation::RADIATION_URL.to_string(),
            schema: Schema::Contains("gamma"),
            max_age_secs: Some(3 * 3_600),
        },
        Probe {
            source: "AFCD red tide",
            url: redtide::RED_TIDE_URL.to_string(),
            schema: Schema::Contains("Red Tide Sighting No."),
            max_age_secs: None,
        },
    ]
}

/// Probe every source once, without the cache.
#[must_use]
pub fn run(probes: &[Probe]) -> Report {
    let checks: Vec<Check> = probes.iter().map(probe).collect();
    let stale = checks
        .iter()
        .filter(|check| check.status == "stale")
        .count();
    let failed = checks
        .iter()
        .filter(|check| check.status != "ok" && check.status != "stale")
        .count();
    let (entries, bytes) = cache::stats();
    Report {
        summary: Summary {
            ok: checks.len() - stale - failed,
            stale,
            failed,
        },
        checks,
        version: VERSION.to_string(),
        cache: CacheInfo {
            dir: cache::dir().map(|dir| dir.display().to_string()),
            entries,
            bytes,
        },
        fetched_at: now_hkt_iso8601(),
    }
}

fn probe(probe: &Probe) -> Check {
    let started = Instant::now();
    let outcome = http::get_text(&probe.url);
    let ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(body) => {
            let age = parse::latest_timestamp(&body)
                .map(|stamp| (crate::now_unix_seconds() - stamp).max(0) as f64 / 3_600.0);
            match schema_problem(&body, probe.schema) {
                None => {
                    let stale = match (age, probe.max_age_secs) {
                        (Some(age), Some(max)) => age * 3_600.0 > max as f64,
                        _ => false,
                    };
                    Check {
                        source: probe.source.to_string(),
                        status: if stale {
                            "stale".to_string()
                        } else {
                            "ok".to_string()
                        },
                        ms,
                        detail: match age {
                            Some(age) if stale => {
                                format!("{} bytes, newest data {age:.1} h old", body.len())
                            }
                            Some(age) => format!("{} bytes, data {age:.1} h old", body.len()),
                            None => format!("{} bytes", body.len()),
                        },
                        age_hours: age,
                    }
                }
                Some((key, detail)) => Check {
                    source: probe.source.to_string(),
                    status: format!("schema:{key}"),
                    ms,
                    detail,
                    age_hours: age,
                },
            }
        }
        Err(error) => Check {
            source: probe.source.to_string(),
            status: format!("fail:{}", reason(&error)),
            ms,
            detail: error.to_string(),
            age_hours: None,
        },
    }
}

/// The short part of an error, without the repeated URL.
fn reason(error: &Error) -> String {
    match error {
        Error::Http { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

/// The missing key (or `json`/`array`/`items`) and a readable note.
fn schema_problem(body: &str, schema: Schema) -> Option<(String, String)> {
    match schema {
        Schema::Keys(keys) => {
            let root: Value = match serde_json::from_str(body) {
                Ok(root) => root,
                Err(error) => return Some(("json".to_string(), error.to_string())),
            };
            if !root.is_object() {
                return Some((
                    "object".to_string(),
                    "expected a top-level JSON object".to_string(),
                ));
            }
            keys.iter()
                .find(|key| root.get(**key).is_none())
                .map(|key| (key.to_string(), format!("missing key {key:?}")))
        }
        Schema::FirstItem(key) => {
            let root: Value = match serde_json::from_str(body) {
                Ok(root) => root,
                Err(error) => return Some(("json".to_string(), error.to_string())),
            };
            let Some(array) = root.as_array() else {
                return Some((
                    "array".to_string(),
                    "expected a top-level JSON array".to_string(),
                ));
            };
            let Some(first) = array.first() else {
                return Some(("items".to_string(), "the JSON array is empty".to_string()));
            };
            first.get(key).is_none().then(|| {
                (
                    key.to_string(),
                    format!("missing key {key:?} in the first item"),
                )
            })
        }
        Schema::Elements(elements) => elements
            .iter()
            .find(|element| !body.contains(&format!("<{element}")))
            .map(|element| (element.to_string(), format!("missing element <{element}>"))),
        Schema::Contains(marker) => (!body.contains(marker))
            .then(|| (marker.to_string(), format!("missing marker {marker:?}"))),
    }
}
