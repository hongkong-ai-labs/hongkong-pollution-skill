//! Rollup tests: windowing, null skipping, worst/mean selection.

use hongkong_pollution_skill::epd_app::build_history;
use hongkong_pollution_skill::model::{mean, rollup};
use hongkong_pollution_skill::parse::{
    parse_aqhi_json, parse_monitor_detail_json, parse_monitors_json, parse_pollutants_xml,
};
use hongkong_pollution_skill::pollutants::{build, worst_pollutant};

const AQHI_JSON: &str = include_str!("fixtures/aqhi.json");
const POLLUTANTS_XML: &str = include_str!("fixtures/24pc.xml");
const EPD_MONITORS: &str = include_str!("fixtures/epd_monitors.json");
const EPD_MONITOR_66: &str = include_str!("fixtures/epd_monitor_66.json");

fn central_report(hours: u32) -> hongkong_pollution_skill::model::PollutantsReport {
    let parsed = parse_pollutants_xml(POLLUTANTS_XML).unwrap();
    build(parsed, hours).unwrap()
}

#[test]
fn hours_window_keeps_newest_readings_oldest_first() {
    let report = central_report(2);
    assert_eq!(report.hours, 2);
    assert_eq!(report.stations.len(), 2);

    let central = &report.stations[0];
    assert_eq!(central.station, "Central/Western");
    assert_eq!(central.series.len(), 2);
    assert_eq!(central.series[0].time, "2026-11-20T19:00:00+08:00");
    assert_eq!(
        central.latest.as_ref().map(|reading| reading.time.as_str()),
        Some("2026-11-20T20:00:00+08:00")
    );
}

#[test]
fn default_window_is_latest_only() {
    let report = central_report(1);
    for station in &report.stations {
        assert_eq!(station.series.len(), 1);
        assert_eq!(
            station.latest.as_ref().unwrap().time,
            station.series[0].time
        );
    }
}

#[test]
fn full_window_keeps_every_reading() {
    let report = central_report(24);
    for station in &report.stations {
        assert_eq!(station.series.len(), 3);
    }
}

#[test]
fn averages_skip_nulls_and_omit_all_null_keys() {
    let report = central_report(2);
    let central = &report.stations[0];
    let averages = &central.averages;
    assert_eq!(averages.no2, Some(37.9));
    assert_eq!(averages.o3, Some(0.6));
    assert_eq!(
        averages.so2,
        Some(1.2),
        "one measured value averages to itself"
    );
    assert_eq!(
        averages.co, None,
        "CO is null in every reading of the window"
    );
    assert_eq!(averages.pm10, Some(15.6));
    assert_eq!(averages.pm25, Some(8.7));

    let json = serde_json::to_value(averages).unwrap();
    assert!(json.get("co").is_none(), "all-null keys are omitted");
    assert_eq!(json.get("no2").and_then(|value| value.as_f64()), Some(37.9));
}

#[test]
fn invalid_windows_are_rejected() {
    let parsed = parse_pollutants_xml(POLLUTANTS_XML).unwrap();
    assert!(build(parsed.clone(), 0).is_err());
    assert!(build(parsed, 25).is_err());
}

#[test]
fn worst_pollutant_picks_the_highest_latest_value() {
    let report = central_report(1);
    let pm25 = worst_pollutant(&report.stations, |reading| reading.pm25).unwrap();
    assert_eq!(pm25.station, "Mong Kok");
    assert_eq!(pm25.value, 28.1);
    assert_eq!(pm25.time, "2026-11-20T20:00:00+08:00");

    let no2 = worst_pollutant(&report.stations, |reading| reading.no2).unwrap();
    assert_eq!(no2.station, "Mong Kok");
    assert_eq!(no2.value, 84.4);

    let all_null = worst_pollutant(&report.stations, |_| None);
    assert!(all_null.is_none());
}

#[test]
fn aqhi_rollup_reports_worst_mean_and_risk_counts() {
    let readings = parse_aqhi_json(AQHI_JSON).unwrap();
    let rolled = rollup(&readings);

    let worst = rolled.worst.unwrap();
    assert_eq!(worst.station, "Mong Kok");
    assert_eq!(worst.aqhi, Some(8));
    assert_eq!(rolled.mean_aqhi, Some(4.8));
    assert_eq!(rolled.risk_counts.get("Moderate"), Some(&2));
    assert_eq!(rolled.risk_counts.get("High"), Some(&2));
    assert_eq!(rolled.risk_counts.get("Low"), Some(&2));
    assert_eq!(rolled.risk_counts.get("Serious"), None);
}

#[test]
fn rollup_handles_empty_and_missing_values() {
    let rolled = rollup(&[]);
    assert!(rolled.worst.is_none());
    assert_eq!(rolled.mean_aqhi, None);
    assert!(rolled.risk_counts.is_empty());

    assert_eq!(mean(&[]), None);
    assert_eq!(mean(&[1.0, 2.0]), Some(1.5));
    assert_eq!(mean(&[1.0, 2.0, 2.0]), Some(1.7));
}

#[test]
fn history_report_keeps_the_full_series_and_validates_hours() {
    let monitors = parse_monitors_json(EPD_MONITORS).unwrap();
    let detail = parse_monitor_detail_json(EPD_MONITOR_66).unwrap();
    let report = build_history(monitors.clone(), vec![detail], 3).unwrap();

    assert_eq!(report.hours, 3);
    assert_eq!(report.stations.len(), 18);
    assert_eq!(report.source, "EPD app API");

    let sham_shui_po = report
        .stations
        .iter()
        .find(|station| station.station_id == 66)
        .unwrap();
    assert_eq!(sham_shui_po.station, "Sham Shui Po");
    assert_eq!(sham_shui_po.category, "general");
    assert_eq!(sham_shui_po.latitude, Some(22.330099));
    assert_eq!(
        sham_shui_po.series.len(),
        23,
        "JSON always carries the full series, not just the table window"
    );
    assert_eq!(
        sham_shui_po.latest.as_ref().unwrap().time,
        sham_shui_po.series[0].time
    );

    let causeway_bay = report
        .stations
        .iter()
        .find(|station| station.station_id == 71)
        .unwrap();
    assert_eq!(causeway_bay.category, "roadside");

    let tai_po = report
        .stations
        .iter()
        .find(|station| station.station_id == 69)
        .unwrap();
    assert_eq!(
        tai_po.series.len(),
        1,
        "a station without a fetched detail keeps its newest reading"
    );
    assert_eq!(tai_po.series[0], *tai_po.latest.as_ref().unwrap());

    assert!(build_history(monitors.clone(), Vec::new(), 0).is_err());
    assert!(build_history(monitors, Vec::new(), 24).is_err());
}
