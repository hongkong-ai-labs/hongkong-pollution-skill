//! `hkpollution brief`: one call that answers "what is the air like where I am".
//!
//! Five parts are assembled independently — place name, street-level reading,
//! nearest EPD station, HKO weather and the advice for the worse of the two
//! AQHI values. A part that fails is recorded as `<part>_error` instead of
//! aborting the whole report, so one flaky upstream cannot hide the rest.

use crate::advice::{self, AdviceRow};
use crate::aqhi;
use crate::error::{Error, Result};
use crate::model::nearest_station;
use crate::now_hkt_iso8601;
use crate::praise;
use crate::weather;
use serde::Serialize;

/// Source label for a complete report.
pub const SOURCE: &str = "EPD / PRAISE-HK / HKO";

/// Queried location.
#[derive(Debug, Clone, Serialize)]
pub struct BriefLocation {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lng: f64,
    /// Reverse-geocoded place name, when PRAISE-HK answered.
    pub place: Option<String>,
}

/// Street-level part of the report.
#[derive(Debug, Clone, Serialize)]
pub struct BriefStreet {
    /// Reading time, ISO-8601 with `+08:00`.
    pub time: String,
    /// Street-level AQHI.
    pub aqhi: Option<i64>,
    /// Added short-term health risk in percent.
    pub added_health_risk: Option<f64>,
}

/// Nearest EPD station part of the report.
#[derive(Debug, Clone, Serialize)]
pub struct BriefStation {
    /// Canonical station name.
    pub name: String,
    /// Official AQHI for that station.
    pub aqhi: Option<i64>,
    /// Health risk band.
    pub risk: Option<String>,
    /// Publish time.
    pub time: Option<String>,
    /// Distance from the queried point in kilometres, one decimal.
    pub distance_km: f64,
}

/// Weather part of the report.
#[derive(Debug, Clone, Serialize)]
pub struct BriefWeather {
    /// Temperature in degrees Celsius at the Observatory, when reported.
    pub temperature: Option<f64>,
    /// Relative humidity in percent at the Observatory, when reported.
    pub humidity: Option<i64>,
    /// Active warning messages.
    pub warnings: Vec<String>,
}

/// The advice that applies to the worse of the two AQHI values.
#[derive(Debug, Clone, Serialize)]
pub struct BriefAdvice {
    /// Risk label of the band used.
    pub risk: &'static str,
    /// AQHI range of the band used.
    pub aqhi_range: &'static str,
    /// Advice for the general public.
    #[serde(flatten)]
    pub row: AdviceRow,
}

/// The `brief --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct BriefReport {
    /// Queried location.
    pub location: BriefLocation,
    /// Street-level reading, absent when PRAISE-HK failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<BriefStreet>,
    /// Why the street part is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street_error: Option<String>,
    /// Nearest station reading, absent when EPD failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub station: Option<BriefStation>,
    /// Why the station part is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub station_error: Option<String>,
    /// Weather, absent when HKO failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<BriefWeather>,
    /// Why the weather part is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather_error: Option<String>,
    /// Advice for the worse of the two AQHI values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice: Option<BriefAdvice>,
    /// True when at least one part is missing.
    pub partial: bool,
    /// Data providers used.
    pub source: String,
    /// When this process assembled the report, in Hong Kong time.
    pub fetched_at: String,
}

/// Worst AQHI named in the report, used by `--fail-above`.
#[must_use]
pub fn worst_aqhi(report: &BriefReport) -> Option<i64> {
    let street = report.street.as_ref().and_then(|part| part.aqhi);
    let station = report.station.as_ref().and_then(|part| part.aqhi);
    match (street, station) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

/// Assemble a brief for one Hong Kong coordinate.
///
/// # Errors
///
/// Returns [`Error::Usage`] when the coordinate is outside Hong Kong, and a
/// plain error only when both the street and the station parts failed — every
/// other partial failure is reported inside the payload.
pub fn build(lat: f64, lng: f64) -> Result<BriefReport> {
    praise::validate_location(lat, lng)?;

    let mut partial = false;

    let place = match praise::reverse_geocode(lat, lng) {
        Ok(name) => Some(name),
        Err(_) => {
            partial = true;
            None
        }
    };

    let street = match praise::fetch(lat, lng, 1) {
        Ok(report) => report.current.map(|reading| BriefStreet {
            time: reading.time,
            aqhi: reading.aqhi,
            added_health_risk: reading.added_health_risk,
        }),
        Err(_) => {
            partial = true;
            None
        }
    };
    let street_error = if street.is_none() {
        Some("PRAISE-HK street-level reading unavailable".to_string())
    } else {
        None
    };

    let (station_name, distance_km) = nearest_station(lat, lng);
    let station = match aqhi::fetch() {
        Ok(report) => report
            .stations
            .iter()
            .find(|reading| reading.station.eq_ignore_ascii_case(station_name))
            .map(|reading| BriefStation {
                name: reading.station.clone(),
                aqhi: reading.aqhi,
                risk: reading.health_risk.clone(),
                time: reading.publish_date.clone(),
                distance_km,
            }),
        Err(_) => None,
    };
    let station_error = if station.is_none() {
        Some(format!(
            "EPD reading for the nearest station ({station_name}) unavailable"
        ))
    } else {
        None
    };
    if station.is_none() {
        partial = true;
    }

    let weather = match weather::fetch() {
        Ok(report) => Some(BriefWeather {
            temperature: observatory_value(&report.temperature),
            humidity: observatory_value(&report.humidity).map(|value| value.round() as i64),
            warnings: report.warnings,
        }),
        Err(_) => {
            partial = true;
            None
        }
    };
    let weather_error = if weather.is_none() {
        Some("HKO weather unavailable".to_string())
    } else {
        None
    };

    if street.is_none() && station.is_none() {
        return Err(Error::parse(
            "could not read either the street-level model or the nearest EPD station; try `hkpollution doctor`",
        ));
    }

    let worst = [
        street.as_ref().and_then(|part| part.aqhi),
        station.as_ref().and_then(|part| part.aqhi),
    ]
    .into_iter()
    .flatten()
    .max();

    let advice = worst.map(|aqhi| {
        let index = advice::band_index_for_aqhi(aqhi);
        let (risk, aqhi_range) = advice::band_label(index);
        BriefAdvice {
            risk,
            aqhi_range,
            row: advice::english_row(index),
        }
    });

    Ok(BriefReport {
        location: BriefLocation { lat, lng, place },
        street,
        street_error,
        station,
        station_error,
        weather,
        weather_error,
        advice,
        partial,
        source: SOURCE.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

/// Value at the Hong Kong Observatory, falling back to the first observation.
fn observatory_value(readings: &[crate::model::HkoReading]) -> Option<f64> {
    readings
        .iter()
        .find(|reading| reading.place.eq_ignore_ascii_case("Hong Kong Observatory"))
        .or_else(|| readings.first())
        .and_then(|reading| reading.value)
}

#[cfg(test)]
mod tests {
    use super::{BriefLocation, BriefReport, BriefStation, BriefStreet, worst_aqhi};

    fn report(street: Option<i64>, station: Option<i64>) -> BriefReport {
        BriefReport {
            location: BriefLocation {
                lat: 22.3,
                lng: 114.2,
                place: None,
            },
            street: street.map(|aqhi| BriefStreet {
                time: "2026-11-21T19:00+08:00".to_string(),
                aqhi: Some(aqhi),
                added_health_risk: None,
            }),
            street_error: None,
            station: station.map(|aqhi| BriefStation {
                name: "Sham Shui Po".to_string(),
                aqhi: Some(aqhi),
                risk: None,
                time: None,
                distance_km: 1.2,
            }),
            station_error: None,
            weather: None,
            weather_error: None,
            advice: None,
            partial: false,
            source: super::SOURCE.to_string(),
            fetched_at: "2026-11-21T19:00:00+08:00".to_string(),
        }
    }

    #[test]
    fn worst_aqhi_takes_the_higher_part() {
        assert_eq!(worst_aqhi(&report(Some(4), Some(7))), Some(7));
        assert_eq!(worst_aqhi(&report(Some(9), Some(2))), Some(9));
        assert_eq!(worst_aqhi(&report(Some(9), None)), Some(9));
        assert_eq!(worst_aqhi(&report(None, Some(3))), Some(3));
        assert_eq!(worst_aqhi(&report(None, None)), None);
    }
}
