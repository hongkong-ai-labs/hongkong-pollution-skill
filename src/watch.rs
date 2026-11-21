//! `watch`: one alert decision across the live sources.
//!
//! The CLI loops over [`check`]; the MCP tool exposes a single evaluation. Each
//! source is optional and independent, so a failed upstream becomes an entry in
//! `errors` rather than a missing decision.

use serde::Serialize;

use crate::error::Result;
use crate::now_hkt_iso8601;
use crate::{aqhi, radiation, redtide};

/// What to alert on.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Alert when the worst AQHI reaches this value.
    pub aqhi: i64,
    /// Alert on any active red tide sighting.
    pub red_tide: bool,
    /// Alert when any radiation station is above twice the network median.
    pub radiation: bool,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            aqhi: 7,
            red_tide: false,
            radiation: false,
        }
    }
}

/// The AQHI reading that triggered, or would trigger, an alert.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorstAqhi {
    /// Station name.
    pub station: String,
    /// AQHI value.
    pub aqhi: i64,
    /// Health risk band.
    pub risk: Option<String>,
}

/// One alert.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    /// `air`, `red tide` or `radiation`.
    pub source: String,
    /// Human-readable message.
    pub message: String,
}

/// The result of one evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct WatchCheck {
    /// When the check ran, ISO-8601 in Hong Kong time.
    pub at: String,
    /// Worst AQHI seen, even below the threshold.
    pub worst_aqhi: Option<WorstAqhi>,
    /// Active red tide sightings, when checked.
    pub active_red_tides: Option<usize>,
    /// Radiation stations above twice the median, when checked.
    pub elevated_radiation: Option<usize>,
    /// Alerts, empty when everything is below threshold.
    pub alerts: Vec<Alert>,
    /// Sources that could not be read.
    pub errors: Vec<String>,
}

impl WatchCheck {
    /// Whether any alert fired.
    #[must_use]
    pub fn triggered(&self) -> bool {
        !self.alerts.is_empty()
    }
}

/// Evaluate every enabled threshold once.
///
/// # Errors
///
/// Returns [`crate::Error::Usage`] when the AQHI threshold is outside 1..=11.
/// Source failures are reported in [`WatchCheck::errors`], not returned.
pub fn check(thresholds: &Thresholds) -> Result<WatchCheck> {
    if !(1..=11).contains(&thresholds.aqhi) {
        return Err(crate::Error::usage("--aqhi must be between 1 and 11"));
    }
    let mut alerts = Vec::new();
    let mut errors = Vec::new();

    let worst_aqhi = match aqhi::fetch() {
        Ok(report) => {
            let worst = report
                .stations
                .iter()
                .filter_map(|reading| {
                    reading.aqhi.map(|value| WorstAqhi {
                        station: reading.station.clone(),
                        aqhi: value,
                        risk: reading.health_risk.clone(),
                    })
                })
                .max_by_key(|reading| reading.aqhi);
            if let Some(worst) = &worst
                && worst.aqhi >= thresholds.aqhi
            {
                alerts.push(Alert {
                    source: "air".to_string(),
                    message: format!(
                        "AQHI {} at {} ({}) reached the threshold of {}",
                        worst.aqhi,
                        worst.station,
                        worst.risk.as_deref().unwrap_or("unknown"),
                        thresholds.aqhi
                    ),
                });
            }
            worst
        }
        Err(error) => {
            errors.push(format!("air: {error}"));
            None
        }
    };

    let active_red_tides = if thresholds.red_tide {
        match redtide::fetch(30) {
            Ok(report) => {
                if report.active > 0 {
                    alerts.push(Alert {
                        source: "red tide".to_string(),
                        message: format!(
                            "{} active red tide sighting(s), latest at {}",
                            report.active,
                            report
                                .sightings
                                .first()
                                .map(|sighting| sighting.location.as_str())
                                .unwrap_or("-")
                        ),
                    });
                }
                Some(report.active)
            }
            Err(error) => {
                errors.push(format!("red tide: {error}"));
                None
            }
        }
    } else {
        None
    };

    let elevated_radiation = if thresholds.radiation {
        match radiation::fetch() {
            Ok(report) => {
                let elevated = report
                    .stations
                    .iter()
                    .filter(|reading| reading.level == "elevated")
                    .count();
                if elevated > 0 {
                    alerts.push(Alert {
                        source: "radiation".to_string(),
                        message: format!(
                            "{elevated} station(s) above twice the network median ({:.2} µSv/h)",
                            report.median_microsievert_per_hour
                        ),
                    });
                }
                Some(elevated)
            }
            Err(error) => {
                errors.push(format!("radiation: {error}"));
                None
            }
        }
    } else {
        None
    };

    Ok(WatchCheck {
        at: now_hkt_iso8601(),
        worst_aqhi,
        active_red_tides,
        elevated_radiation,
        alerts,
        errors,
    })
}
