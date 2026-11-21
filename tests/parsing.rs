//! Fixture tests for every parser. No network access.

use hongkong_pollution_skill::model::{ForecastEntry, ForecastRange};
use hongkong_pollution_skill::parse::{
    blocks, inner_text, join_forecast, normalize_datetime, parse_aqhi_json, parse_aqhi_rss,
    parse_forecast_entries, parse_forecast_json, parse_measurement, parse_monitor_detail_json,
    parse_monitors_json, parse_pollutants_xml, parse_praise_json, parse_weather_json,
    rfc2822_to_iso, strip_bom,
};
use hongkong_pollution_skill::praise::{build, validate_location};

const AQHI_JSON: &str = include_str!("fixtures/aqhi.json");
const AQHI_RSS: &str = include_str!("fixtures/aqhi_rss.xml");
const POLLUTANTS_XML: &str = include_str!("fixtures/24pc.xml");
const HKO_JSON: &str = include_str!("fixtures/hko.json");
const EPD_FORECAST: &str = include_str!("fixtures/epd_forecast.json");
const EPD_MONITORS: &str = include_str!("fixtures/epd_monitors.json");
const EPD_MONITOR_66: &str = include_str!("fixtures/epd_monitor_66.json");
const PRAISE: &str = include_str!("fixtures/praise_pollutant_index.json");

#[test]
fn aqhi_json_yields_every_station() {
    let readings = parse_aqhi_json(AQHI_JSON).unwrap();
    assert_eq!(readings.len(), 6);
    let central = readings
        .iter()
        .find(|reading| reading.station == "Central/Western")
        .unwrap();
    assert_eq!(central.aqhi, Some(4));
    assert_eq!(central.health_risk.as_deref(), Some("Moderate"));
    assert_eq!(central.publish_date.as_deref(), Some("2026-11-21T17:30:00"));
}

#[test]
fn aqhi_json_rejects_non_array_payload() {
    assert!(parse_aqhi_json("{\"error\":\"boom\"}").is_err());
    assert!(parse_aqhi_json("[]").is_err());
    assert!(parse_aqhi_json("not json").is_err());
}

#[test]
fn aqhi_rss_fallback_scans_station_and_value() {
    let readings = parse_aqhi_rss(AQHI_RSS).unwrap();
    assert_eq!(readings.len(), 4);

    let central = &readings[0];
    assert_eq!(central.station, "Central/Western");
    assert_eq!(central.aqhi, Some(4));
    assert_eq!(central.health_risk.as_deref(), Some("Moderate"));
    assert_eq!(
        central.publish_date.as_deref(),
        Some("2026-11-21T17:30:00+08:00")
    );

    let mong_kok = &readings[1];
    assert_eq!(mong_kok.station, "Mong Kok");
    assert_eq!(mong_kok.aqhi, Some(8));
    assert_eq!(mong_kok.health_risk.as_deref(), Some("High"));

    // No <pubDate>, and the "17:30" clock digits must not be mistaken for an AQHI.
    let tap_mun = &readings[2];
    assert_eq!(tap_mun.station, "Tap Mun");
    assert_eq!(tap_mun.aqhi, Some(2));
    assert_eq!(tap_mun.health_risk.as_deref(), Some("Low"));
    assert_eq!(tap_mun.publish_date, None);

    // Two-digit AQHI values stay readable.
    assert_eq!(readings[3].aqhi, Some(11));
    assert_eq!(readings[3].health_risk.as_deref(), Some("Serious"));
}

#[test]
fn aqhi_rss_rejects_document_without_items() {
    assert!(parse_aqhi_rss("<rss><channel></channel></rss>").is_err());
}

#[test]
fn pollutants_xml_handles_bom_dash_and_dot_eight() {
    let parsed = parse_pollutants_xml(POLLUTANTS_XML).unwrap();
    assert_eq!(
        parsed.last_build_date.as_deref(),
        Some("Sat, 21 Nov 2026 17:30:00 +0800")
    );
    assert_eq!(parsed.readings.len(), 6);

    let central: Vec<_> = parsed
        .readings
        .iter()
        .filter(|record| record.station == "Central/Western")
        .collect();
    assert_eq!(central.len(), 3);

    let first = &central[0].reading;
    assert_eq!(first.time, "2026-11-20T18:00:00+08:00");
    assert_eq!(first.no2, Some(47.8));
    assert_eq!(first.pm25, Some(10.8));
    assert_eq!(first.co, None, "`-` means not measured at that station");

    let second = &central[1].reading;
    assert_eq!(second.o3, Some(0.8), "`.8` parses as 0.8");
    assert_eq!(second.so2, Some(1.2));

    let third = &central[2].reading;
    assert_eq!(third.so2, None);
}

#[test]
fn strip_bom_removes_byte_order_mark() {
    assert_eq!(
        strip_bom("\u{feff}<?xml version=\"1.0\"?>"),
        "<?xml version=\"1.0\"?>"
    );
    assert!(
        POLLUTANTS_XML.starts_with('\u{feff}'),
        "fixture must carry a BOM"
    );
    assert!(strip_bom(POLLUTANTS_XML).starts_with("<?xml"));
}

#[test]
fn weather_json_reads_temperature_humidity_and_warnings() {
    let report = parse_weather_json(HKO_JSON).unwrap();
    assert_eq!(
        report.update_time.as_deref(),
        Some("2026-11-21T18:02:00+08:00")
    );
    assert_eq!(report.source, "HKO");
    assert_eq!(report.temperature.len(), 3);
    assert_eq!(report.temperature[0].place, "Hong Kong Observatory");
    assert_eq!(report.temperature[0].value, Some(24.6));
    assert_eq!(report.temperature[0].unit.as_deref(), Some("C"));

    assert_eq!(report.humidity.len(), 2);
    assert_eq!(
        report.humidity[1].value,
        Some(71.0),
        "string humidity values are accepted"
    );

    assert_eq!(report.warnings.len(), 2);
    assert_eq!(
        report.warnings[0], "Very Hot Weather Warning is in force.",
        "markup is stripped from warnings"
    );
}

#[test]
fn weather_json_tolerates_missing_warnings() {
    let report = parse_weather_json(
        "{\"updateTime\":\"2026-11-21T18:02:00+08:00\",\"temperature\":{\"data\":[]},\"humidity\":{\"data\":[]}}",
    )
    .unwrap();
    assert!(report.warnings.is_empty());
    assert!(report.temperature.is_empty());
    assert!(parse_weather_json("[]").is_err());
    assert!(parse_weather_json("{}").is_err());
}

#[test]
fn measurement_parsing_is_tolerant() {
    assert_eq!(parse_measurement("47.8"), Some(47.8));
    assert_eq!(parse_measurement(" 47.8 "), Some(47.8));
    assert_eq!(parse_measurement(".8"), Some(0.8));
    assert_eq!(parse_measurement("12.5 ug/m3"), Some(12.5));
    assert_eq!(parse_measurement("-"), None);
    assert_eq!(parse_measurement("NA"), None);
    assert_eq!(parse_measurement(""), None);
}

#[test]
fn timestamps_normalise_to_iso_8601() {
    assert_eq!(
        rfc2822_to_iso("Sat, 21 Nov 2026 17:30:00 +0800").as_deref(),
        Some("2026-11-21T17:30:00+08:00")
    );
    assert_eq!(
        rfc2822_to_iso("Fri, 20 Nov 2026 18:00:00 GMT").as_deref(),
        Some("2026-11-20T18:00:00+00:00")
    );
    assert_eq!(
        normalize_datetime("2026-11-21T17:30:00"),
        "2026-11-21T17:30:00"
    );
    assert_eq!(normalize_datetime("soon"), "soon");
}

#[test]
fn tag_scanner_handles_attributes_and_cdata() {
    let xml = "<root><a unit=\"ug\">1.5</a><b><![CDATA[x < y]]></b></root>";
    assert_eq!(inner_text(xml, "a"), Some("1.5"));
    assert_eq!(inner_text(xml, "b"), Some("x < y"));
    assert_eq!(inner_text(xml, "missing"), None);

    let items = blocks("<r><item>one</item><item>two</item></r>", "item");
    assert_eq!(items, vec!["one", "two"]);
    assert!(blocks("<r></r>", "item").is_empty());
}

fn entry(period: &str, min: i64, max: i64) -> ForecastEntry {
    ForecastEntry {
        period: period.to_string(),
        min,
        max,
    }
}

#[test]
fn forecast_json_joins_general_and_roadside_by_position() {
    let parsed = parse_forecast_entries(EPD_FORECAST).unwrap();
    assert_eq!(parsed.general.len(), 2);
    assert_eq!(parsed.roadside.len(), 2);
    assert_eq!(parsed.general[0].period, "Tomorrow A.M.");

    let periods = parse_forecast_json(EPD_FORECAST).unwrap();
    assert_eq!(periods.len(), 2);
    assert_eq!(periods[0].period, "Tomorrow A.M.");
    assert_eq!(periods[0].general, Some(ForecastRange { min: 0, max: 1 }));
    assert_eq!(periods[0].roadside, Some(ForecastRange { min: 0, max: 1 }));
    assert_eq!(periods[1].period, "Tomorrow P.M.");
    assert_eq!(periods[1].general, Some(ForecastRange { min: 0, max: 1 }));
}

#[test]
fn forecast_join_falls_back_to_name_and_nulls_missing_sides() {
    let general = vec![entry("Tomorrow A.M.", 0, 1), entry("Tomorrow P.M.", 1, 2)];
    let roadside = vec![entry("Tomorrow P.M.", 2, 3), entry("Tomorrow A.M.", 4, 5)];
    let joined = join_forecast(&general, &roadside);
    assert_eq!(joined.len(), 2);
    assert_eq!(joined[0].period, "Tomorrow A.M.");
    assert_eq!(joined[0].roadside, Some(ForecastRange { min: 4, max: 5 }));
    assert_eq!(joined[1].period, "Tomorrow P.M.");
    assert_eq!(joined[1].roadside, Some(ForecastRange { min: 2, max: 3 }));

    let unequal = join_forecast(&general[..1], &[entry("Tomorrow P.M.", 2, 3)]);
    assert_eq!(unequal.len(), 2);
    assert_eq!(
        unequal[0].roadside, None,
        "general-only period keeps a null side"
    );
    assert_eq!(unequal[1].period, "Tomorrow P.M.");
    assert_eq!(
        unequal[1].general, None,
        "roadside-only period keeps a null side"
    );
    assert_eq!(unequal[1].roadside, Some(ForecastRange { min: 2, max: 3 }));

    assert!(parse_forecast_json("{}").is_err());
    assert!(parse_forecast_json("[]").is_err());
}

#[test]
fn monitors_json_maps_ids_to_canonical_names() {
    let monitors = parse_monitors_json(EPD_MONITORS).unwrap();
    assert_eq!(monitors.len(), 18);

    let sham_shui_po = &monitors[0];
    assert_eq!(sham_shui_po.id, 66);
    assert_eq!(sham_shui_po.station, "Sham Shui Po");
    assert_eq!(sham_shui_po.category, 0);
    assert_eq!(sham_shui_po.latitude, Some(22.330099));
    assert_eq!(sham_shui_po.longitude, Some(114.158867));
    let latest = sham_shui_po.latest.as_ref().unwrap();
    assert_eq!(latest.time, "2026-11-21T18:00:00+08:00");
    assert_eq!(latest.aqhi, Some(4));
    assert_eq!(latest.no2, Some(56.4));
    assert_eq!(latest.o3, Some(58.6));
    assert_eq!(latest.co, None);

    assert!(
        monitors
            .iter()
            .all(|monitor| !monitor.station.contains('深')),
        "every published Chinese name is replaced by its canonical English name"
    );
    assert!(parse_monitors_json("{}").is_err());
    assert!(parse_monitors_json("[]").is_err());
}

#[test]
fn praise_json_maps_series_and_rounds_to_one_decimal() {
    let series = parse_praise_json(PRAISE).unwrap();
    assert_eq!(series.len(), 49);

    let first = &series[0];
    assert_eq!(first.time, "2026-11-21T19:00+08:00");
    assert_eq!(first.aqhi, Some(4));
    assert_eq!(first.added_health_risk, Some(6.3));
    assert_eq!(first.no2, Some(12.8));
    assert_eq!(first.o3, Some(103.9));
    assert_eq!(first.pm10, Some(16.1));
    assert_eq!(first.pm25, Some(10.1));
    assert_eq!(first.so2, Some(4.7));
    assert_eq!(series.last().unwrap().time, "2026-11-23T19:00+08:00");
}

#[test]
fn praise_json_keeps_missing_values_null() {
    let payload = "{\"AQHIBN\":[3,null],\"AQHIER\":[5.0,1.2345],\"NO2\":[null,2.0],\
                   \"O3\":[1.0],\"PM10\":[1.0,null],\"PM2.5\":[null,null],\"SO2\":[3.0,4.0],\
                   \"isots\":[\"2026-11-21T19:00+08:00\",\"2026-11-21T20:00+08:00\"],\"status\":0}";
    let series = parse_praise_json(payload).unwrap();
    assert_eq!(series.len(), 2);
    assert_eq!(series[0].aqhi, Some(3));
    assert_eq!(series[1].aqhi, None);
    assert_eq!(series[1].added_health_risk, Some(1.2));
    assert_eq!(series[1].no2, Some(2.0));
    assert_eq!(series[1].o3, None, "a short series is null past its end");
    assert_eq!(series[0].pm25, None);
    assert!(parse_praise_json("[]").is_err());
    assert!(parse_praise_json("{\"status\":1,\"isots\":[]}").is_err());
}

#[test]
fn praise_hours_window_trims_the_series() {
    let series = parse_praise_json(PRAISE).unwrap();
    let report = build(series.clone(), 22.33751, 114.263, 6).unwrap();
    assert_eq!(report.series.len(), 6);
    assert_eq!(report.current.as_ref(), report.series.first());
    assert_eq!(report.source, "PRAISE-HK");
    assert_eq!(report.location.lat, 22.33751);

    let full = build(series, 22.33751, 114.263, 48).unwrap();
    assert_eq!(
        full.series.len(),
        48,
        "49 upstream hours trim to the 48-hour maximum"
    );

    assert!(build(Vec::new(), 22.33751, 114.263, 0).is_err());
    assert!(build(Vec::new(), 22.33751, 114.263, 49).is_err());
}

#[test]
fn praise_coordinate_bounds_are_usage_errors() {
    assert!(validate_location(22.1, 113.8).is_ok());
    assert!(validate_location(22.6, 114.5).is_ok());
    for (lat, lng) in [(22.0, 114.2), (22.7, 114.2), (22.3, 113.7), (22.3, 114.6)] {
        let error = validate_location(lat, lng).unwrap_err();
        assert!(error.is_usage(), "{lat},{lng} must be a usage error");
    }
    assert!(validate_location(f64::NAN, 114.2).unwrap_err().is_usage());
}

#[test]
fn monitor_detail_merges_histories_newest_first() {
    let detail = parse_monitor_detail_json(EPD_MONITOR_66).unwrap();
    assert_eq!(detail.id, 66);
    assert_eq!(detail.series.len(), 23);

    let newest = &detail.series[0];
    assert_eq!(newest.time, "2026-11-21T18:00:00+08:00");
    assert_eq!(newest.aqhi, Some(4));
    assert_eq!(newest.no2, Some(56.4));
    assert_eq!(newest.o3, Some(58.6));
    assert_eq!(newest.so2, Some(5.7));
    assert_eq!(newest.co, None, "a null history value stays null");
    assert_eq!(newest.pm10, Some(15.1));
    assert_eq!(newest.pm25, Some(7.8));

    let oldest = detail.series.last().unwrap();
    assert_eq!(oldest.time, "2026-11-20T20:00:00+08:00");
    assert_eq!(oldest.aqhi, Some(3));
    assert_eq!(oldest.no2, Some(45.2));

    assert!(
        detail
            .series
            .windows(2)
            .all(|pair| pair[0].time > pair[1].time),
        "the merged series is strictly newest first"
    );
    assert!(parse_monitor_detail_json("[]").is_err());
    assert!(parse_monitor_detail_json("{\"id\":66}").is_err());
}
