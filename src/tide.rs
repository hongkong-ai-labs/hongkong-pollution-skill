//! HKO tide information: latest observed heights and astronomical high/low
//! predictions.
//!
//! Two documented feeds are joined by station name: the observed heights file
//! (every station, updated continuously) and the high/low prediction API, which
//! is keyed by HKO station code and takes a date.

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::now_hkt_iso8601;

/// Latest observed tide heights for every station.
pub const OBSERVED_URL: &str = "https://data.weather.gov.hk/weatherAPI/hko_data/tide/ALL_en.csv";

/// High/low prediction endpoint; station, year, month and day are appended.
pub const PREDICTION_URL: &str = "https://data.weather.gov.hk/weatherAPI/opendata/opendata.php?dataType=HLT&lang=en&rformat=json";

/// Observed heights are near real time; five minutes of cache is enough.
const OBSERVED_TTL_SECS: u64 = 300;

/// Predictions are fixed for the day.
const PREDICTION_TTL_SECS: u64 = 86_400;

/// Tide stations and their HKO codes.
/// Closed stations (Chi Ma Wan since 1997, Lok On Pai since 1999) are omitted:
/// HKO still publishes their predictions but warns they are unreliable.
pub const STATIONS: [(&str, &str); 13] = [
    ("Cheung Chau", "CCH"),
    ("Chek Lap Kok", "CLK"),
    ("Ko Lau Wan", "KLW"),
    ("Kwai Chung", "KCT"),
    ("Ma Wan", "MWC"),
    ("Po Toi", "PT1"),
    ("Quarry Bay", "QUB"),
    ("Shek Pik", "SPW"),
    ("Tai Miu Wan", "TMW"),
    ("Tai O", "TAO"),
    ("Tai Po Kau", "TPK"),
    ("Tsim Bei Tsui", "TBT"),
    ("Waglan Island", "WAG"),
];

/// Look up the HKO code for a station name, already matched case-insensitively.
#[must_use]
pub fn code_for(name: &str) -> Option<&'static str> {
    STATIONS
        .iter()
        .find(|(station, _)| station.eq_ignore_ascii_case(name.trim()))
        .map(|(_, code)| *code)
}

/// Resolve a `--station` value: an exact code, an exact name, or a substring.
///
/// # Errors
///
/// Returns [`Error::NoMatch`] when nothing matches, and [`Error::Usage`] when
/// a substring is ambiguous.
pub fn resolve(value: &str) -> Result<(&'static str, &'static str)> {
    let needle = value.trim();
    if let Some(found) = STATIONS.iter().find(|(station, code)| {
        station.eq_ignore_ascii_case(needle) || code.eq_ignore_ascii_case(needle)
    }) {
        return Ok(*found);
    }
    let matches: Vec<(&'static str, &'static str)> = STATIONS
        .iter()
        .filter(|(station, _)| station.to_lowercase().contains(&needle.to_lowercase()))
        .copied()
        .collect();
    match matches.as_slice() {
        [one] => Ok(*one),
        [] => Err(Error::no_match(format!(
            "no tide station matches {value:?}"
        ))),
        many => Err(Error::usage(format!(
            "{value:?} matches several stations: {}",
            many.iter()
                .map(|(station, _)| *station)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// A predicted high or low tide.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TideEvent {
    /// `high` or `low`.
    pub kind: String,
    /// Local time, `HH:MM`.
    pub time: String,
    /// Height in metres above chart datum.
    pub height_m: f64,
}

/// One station's tide information.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TideStation {
    /// Station name.
    pub station: String,
    /// HKO station code.
    pub code: String,
    /// Latest observed height, when the station reports one.
    pub observed: Option<ObservedTide>,
    /// Predicted events for the requested date, in time order.
    pub events: Vec<TideEvent>,
}

/// The latest observed height.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObservedTide {
    /// Observation date, `YYYY-MM-DD`.
    pub date: String,
    /// Observation time, `HH:MM`.
    pub time: String,
    /// Height in metres.
    pub height_m: f64,
}

/// The `tide` payload.
#[derive(Debug, Clone, Serialize)]
pub struct TideReport {
    /// Prediction date, `YYYY-MM-DD`.
    pub date: String,
    /// Stations included.
    pub count: usize,
    /// Stations, in the order requested.
    pub stations: Vec<TideStation>,
    /// Feed URLs used.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Fetch observed heights and, for one station, that date's predictions.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures and [`Error::Parse`] when a
/// payload cannot be read. A prediction failure for one station is reported as
/// an empty `events` list rather than failing the whole report.
pub fn fetch(station: Option<&str>, date: Option<&str>) -> Result<TideReport> {
    let (year, month, day, iso) = match date {
        Some(raw) => parse_date(raw)?,
        None => today()?,
    };
    let body = http::get_cached(OBSERVED_URL, OBSERVED_TTL_SECS)?;
    let observed = parse_observed(&body)?;
    let selected: Vec<(&'static str, &'static str)> = match station {
        Some(value) => vec![resolve(value)?],
        None => STATIONS.to_vec(),
    };
    let mut stations = Vec::with_capacity(selected.len());
    for (name, code) in selected {
        let latest = observed
            .iter()
            .find(|(station, _)| station.eq_ignore_ascii_case(name))
            .map(|(_, tide)| tide.clone());
        // Predictions are one request per station, so they are only fetched
        // when the caller named a station.
        let events = if station.is_some() {
            fetch_events(code, year, month, day).unwrap_or_default()
        } else {
            Vec::new()
        };
        stations.push(TideStation {
            station: name.to_string(),
            code: code.to_string(),
            observed: latest,
            events,
        });
    }
    if station.is_none() {
        // An unnamed query is "what is the tide now", so stations that are not
        // reporting would only be noise.
        stations.retain(|entry| entry.observed.is_some());
    }
    Ok(TideReport {
        date: iso,
        count: stations.len(),
        stations,
        source: format!("{OBSERVED_URL} + {PREDICTION_URL}"),
        fetched_at: now_hkt_iso8601(),
    })
}

fn fetch_events(code: &str, year: i64, month: u32, day: u32) -> Result<Vec<TideEvent>> {
    let url = format!("{PREDICTION_URL}&station={code}&year={year}&month={month}&day={day}");
    let body = http::get_cached(&url, PREDICTION_TTL_SECS)?;
    parse_events(&body)
}

/// Parse the latest observed heights, one row per station.
fn parse_observed(body: &str) -> Result<Vec<(String, ObservedTide)>> {
    let (header, rows) = csv::rows(body);
    let station = csv::column(&header, "tide station")
        .ok_or_else(|| Error::parse("tide CSV has no station column"))?;
    let date =
        csv::column(&header, "date").ok_or_else(|| Error::parse("tide CSV has no date column"))?;
    let time =
        csv::column(&header, "time").ok_or_else(|| Error::parse("tide CSV has no time column"))?;
    let height = csv::column(&header, "height")
        .ok_or_else(|| Error::parse("tide CSV has no height column"))?;
    let mut observed = Vec::new();
    for fields in rows {
        let (Some(name), Some(date), Some(time), Some(height)) = (
            fields.get(station),
            fields.get(date),
            fields.get(time),
            fields.get(height),
        ) else {
            continue;
        };
        let Ok(height) = height.parse::<f64>() else {
            continue;
        };
        observed.push((
            name.clone(),
            ObservedTide {
                date: date.clone(),
                time: time.clone(),
                height_m: height,
            },
        ));
    }
    if observed.is_empty() {
        return Err(Error::parse("tide CSV has no parsable readings"));
    }
    Ok(observed)
}

/// Parse the HLT payload: `[[MM, DD, time, height, time, height, ...]]`.
fn parse_events(body: &str) -> Result<Vec<TideEvent>> {
    let root: serde_json::Value = serde_json::from_str(body)?;
    let rows = root
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::parse("prediction payload has no data array"))?;
    let row = rows
        .first()
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::parse("prediction payload has no row"))?;
    let mut heights: Vec<(String, f64)> = Vec::new();
    let mut index = 2;
    while index + 1 < row.len() {
        let (Some(time), Some(height)) = (
            row[index].as_str(),
            row[index + 1]
                .as_str()
                .and_then(|raw| raw.parse::<f64>().ok()),
        ) else {
            index += 2;
            continue;
        };
        heights.push((time.to_string(), height));
        index += 2;
    }
    // HKO lists alternating extremes; the first is high when it exceeds the
    // second.
    let first_is_high = match (heights.first(), heights.get(1)) {
        (Some((_, a)), Some((_, b))) => *a >= *b,
        _ => true,
    };
    Ok(heights
        .into_iter()
        .enumerate()
        .map(|(position, (time, height_m))| TideEvent {
            kind: if (position % 2 == 0) == first_is_high {
                "high".to_string()
            } else {
                "low".to_string()
            },
            time: format!(
                "{}:{}",
                &time[0..2.min(time.len())],
                &time[2.min(time.len())..]
            ),
            height_m,
        })
        .collect())
}

fn parse_date(raw: &str) -> Result<(i64, u32, u32, String)> {
    let mut parts = raw.trim().split('-');
    let year: i64 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| Error::usage(format!("--date must be YYYY-MM-DD, got {raw:?}")))?;
    let month: u32 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| Error::usage(format!("--date must be YYYY-MM-DD, got {raw:?}")))?;
    let day: u32 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| Error::usage(format!("--date must be YYYY-MM-DD, got {raw:?}")))?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(Error::usage(format!(
            "--date must be YYYY-MM-DD, got {raw:?}"
        )));
    }
    Ok((year, month, day, format!("{year:04}-{month:02}-{day:02}")))
}

fn today() -> Result<(i64, u32, u32, String)> {
    let now = now_hkt_iso8601();
    parse_date(now.get(0..10).unwrap_or_default())
}

/// Keep only stations whose name contains `query`.
pub fn retain_query(report: &mut TideReport, query: &str) {
    let needle = query.trim().to_lowercase();
    report
        .stations
        .retain(|entry| entry.station.to_lowercase().contains(&needle));
    report.count = report.stations.len();
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVED: &str = "\u{feff}Tide Station,Date,Time,Height(m)\r
Quarry Bay,2026-11-22,02:05,1.19\r
Shek Pik,2026-11-22,02:05,1.04\r
";

    #[test]
    fn parses_observed_heights() {
        let observed = parse_observed(OBSERVED).unwrap();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].0, "Quarry Bay");
        assert_eq!(observed[0].1.height_m, 1.19);
    }

    #[test]
    fn parses_high_low_events() {
        let body = r#"{"fields":["Month","Date","Time","Height(m)","Time","Height(m)","Time","Height(m)","Time","Height(m)"],
            "data":[["11","21","0032","1.31","0618","1.83","1216","1.13","1907","2.15"]]}"#;
        let events = parse_events(body).unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].time, "00:32");
        // 1.31 > 1.83 is false, so the first event is a low.
        assert_eq!(events[0].kind, "low");
        assert_eq!(events[1].kind, "high");
    }

    #[test]
    fn resolves_names_and_codes() {
        assert_eq!(resolve("QUB").unwrap().0, "Quarry Bay");
        assert_eq!(resolve("quarry").unwrap().1, "QUB");
        assert!(resolve("nothing").is_err());
    }
}
