//! Data model for every payload the crate exposes, plus station lookup and the
//! pure AQHI rollup used by `summary`.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::{Error, Result};

/// The 18 AQHI monitoring stations published by the EPD.
///
/// The last three (`Causeway Bay`, `Central`, `Mong Kok`) are roadside stations.
pub const AQHI_STATIONS: [&str; 18] = [
    "Central/Western",
    "Southern",
    "Eastern",
    "Kwun Tong",
    "Sham Shui Po",
    "Kwai Chung",
    "Tsuen Wan",
    "Tseung Kwan O",
    "Yuen Long",
    "Tuen Mun",
    "Tung Chung",
    "Tai Po",
    "Sha Tin",
    "North",
    "Tap Mun",
    "Causeway Bay",
    "Central",
    "Mong Kok",
];

/// One monitor in the undocumented EPD AQHI app API station table.
///
/// `/monitors` names stations in Traditional Chinese only, so app API payloads
/// are mapped back to [`AQHI_STATIONS`] through this table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppStation {
    /// Numeric monitor id used by `app.epd-asmg.gov.hk`.
    pub id: i64,
    /// Traditional Chinese name as published.
    pub name_tc: &'static str,
    /// Canonical English name, matching [`AQHI_STATIONS`].
    pub name_en: &'static str,
    /// `0` for general stations, `1` for roadside stations.
    pub category: u8,
}

/// The app API station table, verified against the live `/monitors` list on
/// 2026-11-21.
pub const EPD_APP_STATIONS: [AppStation; 18] = [
    AppStation {
        id: 66,
        name_tc: "深水埗",
        name_en: "Sham Shui Po",
        category: 0,
    },
    AppStation {
        id: 69,
        name_tc: "大埔",
        name_en: "Tai Po",
        category: 0,
    },
    AppStation {
        id: 70,
        name_tc: "元朗",
        name_en: "Yuen Long",
        category: 0,
    },
    AppStation {
        id: 71,
        name_tc: "銅鑼灣",
        name_en: "Causeway Bay",
        category: 1,
    },
    AppStation {
        id: 72,
        name_tc: "葵涌",
        name_en: "Kwai Chung",
        category: 0,
    },
    AppStation {
        id: 73,
        name_tc: "東區",
        name_en: "Eastern",
        category: 0,
    },
    AppStation {
        id: 74,
        name_tc: "觀塘",
        name_en: "Kwun Tong",
        category: 0,
    },
    AppStation {
        id: 75,
        name_tc: "沙田",
        name_en: "Sha Tin",
        category: 0,
    },
    AppStation {
        id: 76,
        name_tc: "塔門",
        name_en: "Tap Mun",
        category: 0,
    },
    AppStation {
        id: 77,
        name_tc: "荃灣",
        name_en: "Tsuen Wan",
        category: 0,
    },
    AppStation {
        id: 78,
        name_tc: "東涌",
        name_en: "Tung Chung",
        category: 0,
    },
    AppStation {
        id: 79,
        name_tc: "中環",
        name_en: "Central",
        category: 1,
    },
    AppStation {
        id: 80,
        name_tc: "中西區",
        name_en: "Central/Western",
        category: 0,
    },
    AppStation {
        id: 81,
        name_tc: "旺角",
        name_en: "Mong Kok",
        category: 1,
    },
    AppStation {
        id: 82,
        name_tc: "屯門",
        name_en: "Tuen Mun",
        category: 0,
    },
    AppStation {
        id: 83,
        name_tc: "將軍澳",
        name_en: "Tseung Kwan O",
        category: 0,
    },
    AppStation {
        id: 84,
        name_tc: "南區",
        name_en: "Southern",
        category: 0,
    },
    AppStation {
        id: 85,
        name_tc: "北區",
        name_en: "North",
        category: 0,
    },
];

/// Canonical English name for an app API monitor id.
#[must_use]
pub fn app_station_name(id: i64) -> Option<&'static str> {
    EPD_APP_STATIONS
        .iter()
        .find(|station| station.id == id)
        .map(|station| station.name_en)
}

/// App API monitor id for a canonical station name.
#[must_use]
pub fn app_station_id(name: &str) -> Option<i64> {
    EPD_APP_STATIONS
        .iter()
        .find(|station| station.name_en.eq_ignore_ascii_case(name.trim()))
        .map(|station| station.id)
}

/// `general` for app API category 0, `roadside` for category 1.
#[must_use]
pub fn app_category_label(category: u8) -> &'static str {
    if category == 1 { "roadside" } else { "general" }
}

/// One station's current AQHI reading.
#[derive(Debug, Clone, Serialize)]
pub struct AqhiReading {
    /// Station name as published by the EPD.
    pub station: String,
    /// AQHI value (1-10, or 11 for the "serious" band), if reported.
    pub aqhi: Option<i64>,
    /// Health risk band, for example `Moderate`.
    pub health_risk: Option<String>,
    /// Publish timestamp, ISO-8601 when it could be normalised.
    pub publish_date: Option<String>,
}

/// The `aqhi --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct AqhiReport {
    /// Per-station readings.
    pub stations: Vec<AqhiReading>,
    /// Data provider, always `EPD`.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Pollutant concentrations for one station at one hour, in µg/m³.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PollutantReading {
    /// Reading time, ISO-8601 with `+08:00`.
    pub time: String,
    /// Nitrogen dioxide.
    pub no2: Option<f64>,
    /// Ozone.
    pub o3: Option<f64>,
    /// Sulphur dioxide.
    pub so2: Option<f64>,
    /// Carbon monoxide.
    pub co: Option<f64>,
    /// Respirable suspended particulates (PM10).
    pub pm10: Option<f64>,
    /// Fine suspended particulates (PM2.5).
    pub pm25: Option<f64>,
}

/// Window averages for a station; keys with no measured value are omitted.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Averages {
    /// Mean NO2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no2: Option<f64>,
    /// Mean O3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub o3: Option<f64>,
    /// Mean SO2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub so2: Option<f64>,
    /// Mean CO.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co: Option<f64>,
    /// Mean PM10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pm10: Option<f64>,
    /// Mean PM2.5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pm25: Option<f64>,
}

impl Averages {
    /// Average `series` per pollutant, skipping nulls and rounding to one
    /// decimal place.
    #[must_use]
    pub fn from_series(series: &[PollutantReading]) -> Self {
        fn mean_of(
            series: &[PollutantReading],
            pick: fn(&PollutantReading) -> Option<f64>,
        ) -> Option<f64> {
            let values: Vec<f64> = series.iter().filter_map(pick).collect();
            mean(&values)
        }

        Self {
            no2: mean_of(series, |reading| reading.no2),
            o3: mean_of(series, |reading| reading.o3),
            so2: mean_of(series, |reading| reading.so2),
            co: mean_of(series, |reading| reading.co),
            pm10: mean_of(series, |reading| reading.pm10),
            pm25: mean_of(series, |reading| reading.pm25),
        }
    }

    /// Average an app API history series per pollutant, skipping nulls and
    /// rounding to one decimal place.
    #[must_use]
    pub fn from_history(series: &[HistoryReading]) -> Self {
        fn mean_of(
            series: &[HistoryReading],
            pick: fn(&HistoryReading) -> Option<f64>,
        ) -> Option<f64> {
            let values: Vec<f64> = series.iter().filter_map(pick).collect();
            mean(&values)
        }

        Self {
            no2: mean_of(series, |reading| reading.no2),
            o3: mean_of(series, |reading| reading.o3),
            so2: mean_of(series, |reading| reading.so2),
            co: mean_of(series, |reading| reading.co),
            pm10: mean_of(series, |reading| reading.pm10),
            pm25: mean_of(series, |reading| reading.pm25),
        }
    }
}

/// AQHI range forecast for one period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ForecastRange {
    /// Lowest forecast AQHI in the period.
    pub min: i64,
    /// Highest forecast AQHI in the period.
    pub max: i64,
}

/// One entry of one forecast list, before the two lists are joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForecastEntry {
    /// English period label, for example `Tomorrow A.M.`.
    pub period: String,
    /// Lowest forecast AQHI.
    pub min: i64,
    /// Highest forecast AQHI.
    pub max: i64,
}

impl ForecastEntry {
    /// The range as it appears in the report.
    #[must_use]
    pub fn range(&self) -> ForecastRange {
        ForecastRange {
            min: self.min,
            max: self.max,
        }
    }
}

/// Raw output of the forecast JSON parser: the two lists, unjoined.
#[derive(Debug, Clone, Default)]
pub struct ParsedForecast {
    /// General-station forecasts, in payload order.
    pub general: Vec<ForecastEntry>,
    /// Roadside-station forecasts, in payload order.
    pub roadside: Vec<ForecastEntry>,
}

/// One forecast period joined across general and roadside stations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForecastPeriod {
    /// English period label.
    pub period: String,
    /// General-station range, `null` when the period only exists for roadside.
    pub general: Option<ForecastRange>,
    /// Roadside range, `null` when the period only exists for general stations.
    pub roadside: Option<ForecastRange>,
}

/// The `forecast --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct ForecastReport {
    /// Forecast periods, in payload order.
    pub periods: Vec<ForecastPeriod>,
    /// Data provider, always `EPD app API`.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// One hourly AQHI and pollutant reading from the app API.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryReading {
    /// Reading time, ISO-8601 with `+08:00`.
    pub time: String,
    /// Air Quality Health Index.
    pub aqhi: Option<i64>,
    /// Nitrogen dioxide in µg/m³.
    pub no2: Option<f64>,
    /// Ozone in µg/m³.
    pub o3: Option<f64>,
    /// Sulphur dioxide in µg/m³.
    pub so2: Option<f64>,
    /// Carbon monoxide in µg/m³.
    pub co: Option<f64>,
    /// Respirable suspended particulates (PM10) in µg/m³.
    pub pm10: Option<f64>,
    /// Fine suspended particulates (PM2.5) in µg/m³.
    pub pm25: Option<f64>,
}

/// One monitor from `/monitors`, before any history is requested.
#[derive(Debug, Clone)]
pub struct MonitorStation {
    /// App API monitor id.
    pub id: i64,
    /// Canonical English station name.
    pub station: String,
    /// `0` for general, `1` for roadside.
    pub category: u8,
    /// Station latitude in degrees.
    pub latitude: Option<f64>,
    /// Station longitude in degrees.
    pub longitude: Option<f64>,
    /// Newest reading, if the monitor published one.
    pub latest: Option<HistoryReading>,
}

/// One monitor from `/monitors/{id}`, including its hourly histories.
#[derive(Debug, Clone)]
pub struct MonitorDetail {
    /// App API monitor id.
    pub id: i64,
    /// Newest reading from the monitor's top-level fields.
    pub latest: Option<HistoryReading>,
    /// Full hourly series, newest first.
    pub series: Vec<HistoryReading>,
}

/// One station in the `history --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct StationHistory {
    /// Canonical English station name.
    pub station: String,
    /// App API monitor id.
    pub station_id: i64,
    /// `general` or `roadside`.
    pub category: String,
    /// Station latitude in degrees.
    pub latitude: Option<f64>,
    /// Station longitude in degrees.
    pub longitude: Option<f64>,
    /// Newest reading in the series.
    pub latest: Option<HistoryReading>,
    /// Full hourly series, newest first.
    pub series: Vec<HistoryReading>,
}

/// The `history --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryReport {
    /// Requested table window in hours (1-23).
    pub hours: u32,
    /// Selected stations, in monitor-id order.
    pub stations: Vec<StationHistory>,
    /// Data provider, always `EPD app API`.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// One station in the `pollutants --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct StationPollutants {
    /// Station name.
    pub station: String,
    /// Newest reading in the requested window.
    pub latest: Option<PollutantReading>,
    /// Requested window, oldest first, at most `hours` entries.
    pub series: Vec<PollutantReading>,
    /// Averages over the window.
    pub averages: Averages,
}

/// The `pollutants --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct PollutantsReport {
    /// `<lastBuildDate>` exactly as published, when present.
    pub last_build_date: Option<String>,
    /// Requested window size in hours (1-24).
    pub hours: u32,
    /// Per-station series and averages.
    pub stations: Vec<StationPollutants>,
    /// Data provider, always `EPD`.
    pub source: String,
}

/// A parsed `<PollutantConcentration>` block before windowing.
#[derive(Debug, Clone)]
pub struct PollutantRecord {
    /// Station name.
    pub station: String,
    /// The reading, including its normalised time.
    pub reading: PollutantReading,
}

/// Raw output of the 24-hour pollutant XML parser.
#[derive(Debug, Clone)]
pub struct ParsedPollutants {
    /// `<lastBuildDate>` as published.
    pub last_build_date: Option<String>,
    /// Every block found in the document.
    pub readings: Vec<PollutantRecord>,
}

/// One HKO observation (temperature or humidity).
#[derive(Debug, Clone, Serialize)]
pub struct HkoReading {
    /// Place name, for example `Hong Kong Observatory`.
    pub place: String,
    /// Observation value.
    pub value: Option<f64>,
    /// Unit as published, for example `C` or `percent`.
    pub unit: Option<String>,
}

/// The `weather --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct WeatherReport {
    /// HKO `updateTime`, ISO-8601 with offset.
    pub update_time: Option<String>,
    /// Temperature observations.
    pub temperature: Vec<HkoReading>,
    /// Humidity observations.
    pub humidity: Vec<HkoReading>,
    /// Active warning messages, HTML stripped.
    pub warnings: Vec<String>,
    /// Data provider, always `HKO`.
    pub source: String,
}

/// The station with the highest value of a pollutant in the latest hour.
#[derive(Debug, Clone, Serialize)]
pub struct PollutantExtreme {
    /// Station name.
    pub station: String,
    /// The value in µg/m³.
    pub value: f64,
    /// Time of the reading.
    pub time: String,
}

/// Pure AQHI rollup over a set of station readings.
#[derive(Debug, Clone, Default)]
pub struct Rollup {
    /// Station with the highest AQHI.
    pub worst: Option<AqhiReading>,
    /// Mean AQHI across stations that reported one, one decimal place.
    pub mean_aqhi: Option<f64>,
    /// How many stations reported each health risk band.
    pub risk_counts: BTreeMap<String, usize>,
}

/// The `summary --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct SummaryReport {
    /// When the data was fetched, in Hong Kong time.
    pub fetched_at: String,
    /// Per-station AQHI readings.
    pub stations: Vec<AqhiReading>,
    /// Highest AQHI station.
    pub worst: Option<AqhiReading>,
    /// Mean AQHI.
    pub mean_aqhi: Option<f64>,
    /// Health risk band counts.
    pub risk_counts: BTreeMap<String, usize>,
    /// Station with the highest latest PM2.5.
    pub worst_pm25: Option<PollutantExtreme>,
    /// Station with the highest latest NO2.
    pub worst_no2: Option<PollutantExtreme>,
    /// HKO readings, only present when weather was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<WeatherReport>,
    /// Data providers used, for example `EPD` or `EPD / HKO`.
    pub source: String,
}

/// Resolve a `--station` query against [`AQHI_STATIONS`].
///
/// The match is a case-insensitive substring match, so `central` resolves to
/// both `Central/Western` and `Central`. Zero matches is [`Error::NoMatch`]
/// and the message lists every valid station.
pub fn resolve_station(query: &str) -> Result<Vec<String>> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Err(Error::usage("--station needs a non-empty station name"));
    }
    let matches: Vec<String> = AQHI_STATIONS
        .iter()
        .filter(|station| station.to_lowercase().contains(&needle))
        .map(|station| (*station).to_string())
        .collect();
    if matches.is_empty() {
        return Err(Error::no_match(format!(
            "no station matches {query:?}; valid stations: {}",
            AQHI_STATIONS.join(", ")
        )));
    }
    Ok(matches)
}

/// Compute the worst station, mean AQHI and risk-band counts.
#[must_use]
pub fn rollup(stations: &[AqhiReading]) -> Rollup {
    // First station wins ties, so the report is stable when the whole network
    // sits on the same band.
    let mut worst: Option<&AqhiReading> = None;
    for reading in stations {
        if reading.aqhi.is_some() && worst.is_none_or(|current| reading.aqhi > current.aqhi) {
            worst = Some(reading);
        }
    }
    let worst = worst.cloned();
    let values: Vec<f64> = stations
        .iter()
        .filter_map(|reading| reading.aqhi)
        .map(|aqhi| aqhi as f64)
        .collect();
    let mut risk_counts: BTreeMap<String, usize> = BTreeMap::new();
    for reading in stations {
        if let Some(risk) = &reading.health_risk {
            let key = risk.trim();
            if !key.is_empty() {
                *risk_counts.entry(key.to_string()).or_insert(0) += 1;
            }
        }
    }
    Rollup {
        worst,
        mean_aqhi: mean(&values),
        risk_counts,
    }
}

/// Mean of `values` rounded to one decimal place, or `None` when empty.
#[must_use]
pub fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    Some(((sum / values.len() as f64) * 10.0).round() / 10.0)
}
