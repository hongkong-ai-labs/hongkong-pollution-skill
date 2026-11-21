//! Past-24-hour pollutant concentrations from the EPD XML feed.

use crate::error::{Error, Result};
use crate::http;
use crate::model::{
    Averages, ParsedPollutants, PollutantExtreme, PollutantReading, PollutantsReport,
    StationPollutants,
};
use crate::parse;

/// The 24-hour pollutant concentration XML feed.
pub const POLLUTANTS_URL: &str = "https://www.aqhi.gov.hk/epd/ddata/html/out/24pc_Eng.xml";

/// Largest accepted `--hours` window.
pub const MAX_HOURS: u32 = 24;

/// Freshness of a cached pollutant feed, in seconds.
const CACHE_TTL_SECS: u64 = 300;

/// Fetch the 24-hour pollutant feed and window it to `hours` readings per
/// station.
///
/// # Errors
///
/// Returns [`Error::Http`] for transport failures, [`Error::Parse`] for an
/// unreadable document and [`Error::Usage`] when `hours` is out of range.
pub fn fetch(hours: u32) -> Result<PollutantsReport> {
    let body = http::get_cached(POLLUTANTS_URL, CACHE_TTL_SECS)?;
    let parsed = parse::parse_pollutants_xml(&body)?;
    build(parsed, hours)
}

/// Turn parsed records into the public report shape.
///
/// Each station keeps its newest `hours` readings, oldest first; `latest` is
/// the newest of those and `averages` skips nulls.
///
/// # Errors
///
/// Returns [`Error::Usage`] when `hours` is `0` or greater than [`MAX_HOURS`].
pub fn build(parsed: ParsedPollutants, hours: u32) -> Result<PollutantsReport> {
    if hours == 0 || hours > MAX_HOURS {
        return Err(Error::usage(format!(
            "--hours must be between 1 and {MAX_HOURS}, got {hours}"
        )));
    }
    let mut grouped: Vec<(String, Vec<PollutantReading>)> = Vec::new();
    for record in parsed.readings {
        match grouped
            .iter_mut()
            .find(|(station, _)| *station == record.station)
        {
            Some((_, series)) => series.push(record.reading),
            None => grouped.push((record.station, vec![record.reading])),
        }
    }
    let limit = hours as usize;
    let stations = grouped
        .into_iter()
        .map(|(station, mut series)| {
            series.sort_by(|left, right| left.time.cmp(&right.time));
            if series.len() > limit {
                let excess = series.len() - limit;
                series.drain(..excess);
            }
            let latest = series.last().cloned();
            let averages = Averages::from_series(&series);
            StationPollutants {
                station,
                latest,
                series,
                averages,
            }
        })
        .collect();
    Ok(PollutantsReport {
        last_build_date: parsed.last_build_date,
        hours,
        stations,
        source: "EPD".to_string(),
    })
}

/// Station with the highest latest value of one pollutant.
///
/// `pick` selects the field, for example `|reading| reading.pm25`.
#[must_use]
pub fn worst_pollutant(
    stations: &[StationPollutants],
    pick: fn(&PollutantReading) -> Option<f64>,
) -> Option<PollutantExtreme> {
    stations
        .iter()
        .filter_map(|station| {
            let reading = station.latest.as_ref()?;
            Some(PollutantExtreme {
                station: station.station.clone(),
                value: pick(reading)?,
                time: reading.time.clone(),
            })
        })
        .max_by(|left, right| left.value.total_cmp(&right.value))
}

/// Retain only stations whose name matches one of `names` (case-insensitive).
pub fn retain_stations(report: &mut PollutantsReport, names: &[String]) {
    report.stations.retain(|station| {
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&station.station))
    });
}
