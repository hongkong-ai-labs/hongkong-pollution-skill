//! HKO daily mean sea temperature at Waglan Island.
//!
//! The Observatory publishes one CSV per year with a bilingual title header, so
//! the parser skips until the `Year/Month/Day/Value` row. `***` marks a missing
//! day and is skipped rather than treated as zero.

use serde::Serialize;

use crate::csv;
use crate::error::{Error, Result};
use crate::http;
use crate::now_hkt_iso8601;

/// Yearly daily-mean sea temperature file; the year is inserted.
#[must_use]
pub fn url(year: u32) -> String {
    format!(
        "https://data.weather.gov.hk/weatherAPI/cis/csvfile/WGL/{year}/daily_WGL_SST_{year}.csv"
    )
}

/// Sea temperature moves slowly; six hours of cache is plenty.
const CACHE_TTL_SECS: u64 = 21_600;

/// Default averaging window in days.
pub const DEFAULT_DAYS: u32 = 14;

/// Largest accepted window.
pub const MAX_DAYS: u32 = 365;

/// One day's reading.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeaSample {
    /// Date, `YYYY-MM-DD`.
    pub date: String,
    /// Daily mean sea temperature in degrees Celsius.
    pub celsius: f64,
}

/// The `sea-temp` payload.
#[derive(Debug, Clone, Serialize)]
pub struct SeaTempReport {
    /// Station name.
    pub station: String,
    /// Newest reading.
    pub latest: SeaSample,
    /// Window used for the statistics, in days.
    pub days: u32,
    /// Readings in the window.
    pub samples: usize,
    /// Mean over the window, one decimal.
    pub mean_celsius: Option<f64>,
    /// Lowest reading in the window.
    pub min_celsius: Option<f64>,
    /// Highest reading in the window.
    pub max_celsius: Option<f64>,
    /// Readings in the window, newest first.
    pub series: Vec<SeaSample>,
    /// Source URL.
    pub source: String,
    /// When this process fetched the data, in Hong Kong time.
    pub fetched_at: String,
}

/// Fetch the latest daily mean sea temperature and its recent window.
///
/// The current year is tried first; when it has no rows yet (early January),
/// the previous year is used.
///
/// # Errors
///
/// Returns [`Error::Usage`] for a window outside 1..=[`MAX_DAYS`], the usual
/// HTTP/parse errors, and [`Error::Parse`] when no year yields a reading.
pub fn fetch(days: u32) -> Result<SeaTempReport> {
    if days == 0 || days > MAX_DAYS {
        return Err(Error::usage(format!(
            "--days must be between 1 and {MAX_DAYS}"
        )));
    }
    let year = crate::now_hkt_iso8601()
        .get(0..4)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(2_026);
    let mut last_error = None;
    for candidate in [year, year.saturating_sub(1)] {
        let source = url(candidate);
        match http::get_cached(&source, CACHE_TTL_SECS).and_then(|body| parse(&body)) {
            Ok(samples) if !samples.is_empty() => {
                return Ok(report(samples, days, source));
            }
            Ok(_) => last_error = Some(format!("{source} has no rows")),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(Error::parse(last_error.unwrap_or_else(|| {
        "no sea temperature file available".to_string()
    })))
}

fn report(mut samples: Vec<SeaSample>, days: u32, source: String) -> SeaTempReport {
    samples.sort_by(|left, right| left.date.cmp(&right.date));
    let latest = samples.last().cloned().unwrap_or(SeaSample {
        date: String::new(),
        celsius: 0.0,
    });
    let cutoff = crate::days_from_iso(&latest.date)
        .map(|day| day - i64::from(days) + 1)
        .unwrap_or(i64::MIN);
    let mut window: Vec<SeaSample> = samples
        .into_iter()
        .filter(|sample| crate::days_from_iso(&sample.date).is_some_and(|day| day >= cutoff))
        .collect();
    window.sort_by(|left, right| right.date.cmp(&left.date));
    let values: Vec<f64> = window.iter().map(|sample| sample.celsius).collect();
    let mean = if values.is_empty() {
        None
    } else {
        #[allow(clippy::cast_precision_loss)]
        let count = values.len() as f64;
        Some((values.iter().sum::<f64>() / count * 10.0).round() / 10.0)
    };
    let min = values
        .iter()
        .copied()
        .reduce(f64::min)
        .map(|value| (value * 10.0).round() / 10.0);
    let max = values
        .iter()
        .copied()
        .reduce(f64::max)
        .map(|value| (value * 10.0).round() / 10.0);
    SeaTempReport {
        station: "Waglan Island".to_string(),
        latest,
        days,
        samples: window.len(),
        mean_celsius: mean,
        min_celsius: min,
        max_celsius: max,
        series: window,
        source,
        fetched_at: now_hkt_iso8601(),
    }
}

fn parse(body: &str) -> Result<Vec<SeaSample>> {
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .find(|line| {
            let lowered = line.to_lowercase();
            lowered.contains("year") && lowered.contains("value")
        })
        .ok_or_else(|| Error::parse("sea temperature CSV has no header row"))?;
    let header = csv::split_line(header);
    let year = csv::column(&header, "year")
        .ok_or_else(|| Error::parse("sea temperature CSV has no year column"))?;
    let month = csv::column(&header, "month")
        .ok_or_else(|| Error::parse("sea temperature CSV has no month column"))?;
    let day = csv::column(&header, "day")
        .ok_or_else(|| Error::parse("sea temperature CSV has no day column"))?;
    let value = csv::column(&header, "value")
        .ok_or_else(|| Error::parse("sea temperature CSV has no value column"))?;

    let mut samples = Vec::new();
    for line in lines {
        let fields = csv::split_line(line);
        let (Some(year), Some(month), Some(day), Some(value)) = (
            fields.get(year),
            fields.get(month),
            fields.get(day),
            fields.get(value),
        ) else {
            continue;
        };
        let (Ok(year), Ok(month), Ok(day)) = (
            year.trim().parse::<i64>(),
            month.trim().parse::<u32>(),
            day.trim().parse::<u32>(),
        ) else {
            continue;
        };
        let Ok(celsius) = value.trim().parse::<f64>() else {
            continue;
        };
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            continue;
        }
        samples.push(SeaSample {
            date: format!("{year:04}-{month:02}-{day:02}"),
            celsius,
        });
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "日平均海面溫度(攝氏度) - 橫瀾島\r
Daily Mean Sea Temperature (°C) at Waglan Island\r
年/Year,月/Month,日/Day,數值/Value,數據完整性/data Completeness\r
2026,1,1,***,\r
2026,1,2,19.2,\r
2026,1,3,19.4,\r
2026,1,4,19.1,\r
";

    #[test]
    fn parses_rows_and_skips_missing_days() {
        let samples = parse(SAMPLE).unwrap();
        assert_eq!(samples.len(), 3, "*** is not a value");
        assert_eq!(samples[0].date, "2026-01-02");
        assert_eq!(samples[0].celsius, 19.2);
    }

    #[test]
    fn computes_window_statistics() {
        let samples = parse(SAMPLE).unwrap();
        let report = report(samples, 2, "test".to_string());
        assert_eq!(report.latest.date, "2026-01-04");
        assert_eq!(report.samples, 2);
        assert_eq!(report.mean_celsius, Some(19.3));
        assert_eq!(report.min_celsius, Some(19.1));
        assert_eq!(report.max_celsius, Some(19.4));
    }

    #[test]
    fn rejects_a_body_without_a_header() {
        assert!(parse("Not found").is_err());
    }
}
