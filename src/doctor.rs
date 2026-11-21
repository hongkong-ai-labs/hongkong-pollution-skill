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
    VERSION, aqhi, beach, cache, epd_app, http, now_hkt_iso8601, pollutants, praise, weather,
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
}

/// Counts over [`Check`]s.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Summary {
    /// Checks with status `ok`.
    pub ok: usize,
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
    /// Whether every probe passed.
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.summary.failed == 0
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
        },
        Probe {
            source: "aqhi.gov.hk RSS",
            url: aqhi::AQHI_RSS_URL.to_string(),
            schema: Schema::Elements(&["item"]),
        },
        Probe {
            source: "aqhi.gov.hk 24pc",
            url: pollutants::POLLUTANTS_URL.to_string(),
            schema: Schema::Elements(&["PollutantConcentration"]),
        },
        Probe {
            source: "HKO rhrread",
            url: weather::WEATHER_URL.to_string(),
            schema: Schema::Keys(&["temperature", "humidity"]),
        },
        Probe {
            source: "HKO fnd",
            url: weather::FORECAST_URL.to_string(),
            schema: Schema::Keys(&["weatherForecast"]),
        },
        Probe {
            source: "EPD app forecast",
            url: epd_app::FORECAST_URL.to_string(),
            schema: Schema::Keys(&["general_stations"]),
        },
        Probe {
            source: "EPD app monitors",
            url: epd_app::MONITORS_URL.to_string(),
            schema: Schema::FirstItem("id"),
        },
        Probe {
            source: "PRAISE weather",
            url: praise::weather_url(PROBE_LAT, PROBE_LNG),
            schema: Schema::Keys(&["temperature", "humidity"]),
        },
        Probe {
            source: "PRAISE pollutant_index",
            url: praise::pollutant_index_url(PROBE_LAT, PROBE_LNG),
            schema: Schema::Keys(&["AQHIBN"]),
        },
        Probe {
            source: "EPD beach grading",
            url: beach::BEACH_URL.to_string(),
            schema: Schema::Keys(&["features"]),
        },
        Probe {
            source: "EPD beach E. coli",
            url: beach::ecoli_url(beach::latest_published_year()),
            schema: Schema::Contains("E. coli"),
        },
    ]
}

/// Probe every source once, without the cache.
#[must_use]
pub fn run(probes: &[Probe]) -> Report {
    let checks: Vec<Check> = probes.iter().map(probe).collect();
    let failed = checks.iter().filter(|check| check.status != "ok").count();
    let (entries, bytes) = cache::stats();
    Report {
        summary: Summary {
            ok: checks.len() - failed,
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
        Ok(body) => match schema_problem(&body, probe.schema) {
            None => Check {
                source: probe.source.to_string(),
                status: "ok".to_string(),
                ms,
                detail: format!("{} bytes", body.len()),
            },
            Some((key, detail)) => Check {
                source: probe.source.to_string(),
                status: format!("schema:{key}"),
                ms,
                detail,
            },
        },
        Err(error) => Check {
            source: probe.source.to_string(),
            status: format!("fail:{}", reason(&error)),
            ms,
            detail: error.to_string(),
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
