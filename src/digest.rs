//! `hkpollution digest`: one line per source, for a cron job or a morning check.
//!
//! Every section is independent, so a single upstream outage produces an
//! `unavailable` line rather than an empty report. The command always succeeds.

use serde::Serialize;

use crate::model::rollup;
use crate::now_hkt_iso8601;
use crate::{aqhi, beach, radiation, redtide, uv, water, weather};

/// One line of the digest.
#[derive(Debug, Clone, Serialize)]
pub struct DigestLine {
    /// Source label, for example `air` or `red tide`.
    pub source: String,
    /// `ok` or `unavailable`.
    pub status: String,
    /// One-line summary, empty when unavailable.
    pub summary: String,
    /// Why the source is unavailable.
    pub error: Option<String>,
}

/// The `digest` payload.
#[derive(Debug, Clone, Serialize)]
pub struct DigestReport {
    /// One line per source, in a fixed order.
    pub lines: Vec<DigestLine>,
    /// How many lines are `ok`.
    pub ok: usize,
    /// When this process assembled the digest, in Hong Kong time.
    pub fetched_at: String,
}

/// Build the digest, degrading each source independently.
#[must_use]
pub fn build() -> DigestReport {
    let lines = vec![
        air(),
        beach(),
        red_tide(),
        radiation_level(),
        river(),
        ultraviolet(),
        warnings(),
    ];
    let ok = lines.iter().filter(|line| line.status == "ok").count();
    DigestReport {
        lines,
        ok,
        fetched_at: now_hkt_iso8601(),
    }
}

fn line(source: &str, summary: String) -> DigestLine {
    DigestLine {
        source: source.to_string(),
        status: "ok".to_string(),
        summary,
        error: None,
    }
}

fn unavailable(source: &str, error: impl ToString) -> DigestLine {
    DigestLine {
        source: source.to_string(),
        status: "unavailable".to_string(),
        summary: String::new(),
        error: Some(error.to_string()),
    }
}

fn air() -> DigestLine {
    match aqhi::fetch() {
        Ok(report) => {
            let rolled = rollup(&report.stations);
            match rolled.worst {
                Some(worst) => line(
                    "air",
                    format!(
                        "worst AQHI {} at {} ({}), network mean {:.1}",
                        worst.aqhi.unwrap_or(0),
                        worst.station,
                        worst.health_risk.as_deref().unwrap_or("unknown"),
                        rolled.mean_aqhi.unwrap_or(0.0)
                    ),
                ),
                None => unavailable("air", "no station carried an AQHI"),
            }
        }
        Err(error) => unavailable("air", error),
    }
}

fn beach() -> DigestLine {
    match beach::fetch() {
        Ok(report) => {
            let ungraded = report.count.saturating_sub(report.graded);
            line(
                "beach",
                format!(
                    "{} graded, {} open, {} not graded (EPD {})",
                    report.graded,
                    report.open,
                    ungraded,
                    report
                        .ecoli_source
                        .as_deref()
                        .and_then(|url| url.rsplit('/').next())
                        .unwrap_or("E. coli unavailable")
                ),
            )
        }
        Err(error) => unavailable("beach", error),
    }
}

fn red_tide() -> DigestLine {
    match redtide::fetch(redtide::DEFAULT_DAYS) {
        Ok(report) if report.active > 0 => line(
            "red tide",
            format!(
                "{} active sighting(s); latest {} at {}",
                report.active,
                report
                    .sightings
                    .first()
                    .map(|sighting| sighting.reported_on.as_str())
                    .unwrap_or("-"),
                report
                    .sightings
                    .first()
                    .map(|sighting| sighting.location.as_str())
                    .unwrap_or("-")
            ),
        ),
        Ok(report) => line(
            "red tide",
            format!(
                "no active sightings; {} reported in the last {} days",
                report.count, report.days
            ),
        ),
        Err(error) => unavailable("red tide", error),
    }
}

fn radiation_level() -> DigestLine {
    match radiation::fetch() {
        Ok(report) => {
            let max = report
                .max
                .as_ref()
                .map(|reading| {
                    format!(
                        "{:.2} µSv/h at {}",
                        reading.microsievert_per_hour, reading.station
                    )
                })
                .unwrap_or_else(|| "no reading".to_string());
            line(
                "radiation",
                format!(
                    "{} stations, median {:.2} µSv/h, max {max}",
                    report.count, report.median_microsievert_per_hour
                ),
            )
        }
        Err(error) => unavailable("radiation", error),
    }
}

fn river() -> DigestLine {
    match water::fetch(water::Kind::River, 1) {
        Ok(report) => line(
            "river",
            format!(
                "{} stations, newest sampling month {} (EPD monthly)",
                report.count,
                report.latest_month.as_deref().unwrap_or("unknown")
            ),
        ),
        Err(error) => unavailable("river", error),
    }
}

fn ultraviolet() -> DigestLine {
    match uv::fetch() {
        Ok(report) => line(
            "uv",
            format!(
                "index {:.1} ({}), observed {}",
                report.index, report.band, report.time
            ),
        ),
        Err(error) => unavailable("uv", error),
    }
}

fn warnings() -> DigestLine {
    match weather::fetch() {
        Ok(report) if report.warnings.is_empty() => {
            line("weather", "no active warnings".to_string())
        }
        Ok(report) => line("weather", report.warnings.join("; ")),
        Err(error) => unavailable("weather", error),
    }
}
