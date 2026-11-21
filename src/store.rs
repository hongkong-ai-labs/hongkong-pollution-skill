//! Local AQHI observation log.
//!
//! `stats` needs history longer than any upstream keeps, so every successful
//! current-reading fetch appends one compact line per station-hour to
//! `history.jsonl` in the cache directory. It is best-effort: a read-only or
//! missing directory never fails a command, it just means `stats` reports what
//! the upstream feeds still hold.

use std::collections::HashSet;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::model::AqhiReading;

/// File name inside the cache directory.
const FILE: &str = "history.jsonl";

/// Rows older than this are pruned on write.
const KEEP_DAYS: i64 = 90;

/// One observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// Unix hour (seconds floored to the hour) of the reading.
    pub hour: i64,
    /// Station name.
    pub station: String,
    /// AQHI value.
    pub aqhi: i64,
}

impl Observation {
    /// Unix hour for a timestamp, floored.
    #[must_use]
    pub fn hour_of(unix_seconds: i64) -> i64 {
        unix_seconds - unix_seconds.rem_euclid(3_600)
    }
}

fn path() -> Option<std::path::PathBuf> {
    crate::cache::dir().map(|dir| dir.join(FILE))
}

/// Every stored observation, oldest first. Unreadable lines are skipped.
#[must_use]
pub fn read_all() -> Vec<Observation> {
    let Some(path) = path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut rows: Vec<Observation> = raw
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    rows.sort_by_key(|row| row.hour);
    rows
}

/// Observations from the last `days` days, oldest first.
#[must_use]
pub fn read_days(days: u32) -> Vec<Observation> {
    let cutoff = Observation::hour_of(crate::now_unix_seconds()) - i64::from(days) * 24 * 3_600;
    read_all()
        .into_iter()
        .filter(|row| row.hour >= cutoff)
        .collect()
}

/// Append observations for the current hour, skipping any already stored.
///
/// The whole file is rewritten so pruning stays simple; the volume is one line
/// per station per hour, far below anything that would matter.
pub fn record(readings: &[AqhiReading], unix_seconds: i64) {
    let hour = Observation::hour_of(unix_seconds);
    let rows: Vec<Observation> = readings
        .iter()
        .filter_map(|reading| {
            let aqhi = reading.aqhi.filter(|value| (1..=11).contains(value))?;
            Some(Observation {
                hour,
                station: reading.station.clone(),
                aqhi,
            })
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let existing = read_all();
    // The hour is already captured; a second fetch would only duplicate it.
    if rows.iter().all(|new| {
        existing
            .iter()
            .any(|row| row.hour == hour && row.station == new.station)
    }) {
        return;
    }
    let cutoff = hour - KEEP_DAYS * 24 * 3_600;
    let mut seen: HashSet<(i64, String)> = HashSet::new();
    let mut merged: Vec<Observation> = Vec::with_capacity(existing.len() + rows.len());
    for row in existing.into_iter().filter(|row| row.hour >= cutoff) {
        if seen.insert((row.hour, row.station.clone())) {
            merged.push(row);
        }
    }
    for row in rows {
        if seen.insert((row.hour, row.station.clone())) {
            merged.push(row);
        }
    }
    merged.sort_by_key(|row| row.hour);
    let Some(path) = path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(mut file) = std::fs::File::create(&path) else {
        return;
    };
    for row in &merged {
        if serde_json::to_writer(&mut file, row).is_err() {
            return;
        }
        if file.write_all(b"\n").is_err() {
            return;
        }
    }
}

/// Number of stored observations and the oldest one's hour.
#[must_use]
pub fn summary() -> (usize, Option<i64>) {
    let rows = read_all();
    (rows.len(), rows.first().map(|row| row.hour))
}
