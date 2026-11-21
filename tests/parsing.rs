//! Fixture tests for every parser. No network access.

use hongkong_pollution_skill::parse::{
    blocks, inner_text, normalize_datetime, parse_aqhi_json, parse_aqhi_rss, parse_measurement,
    parse_pollutants_xml, parse_weather_json, rfc2822_to_iso, strip_bom,
};

const AQHI_JSON: &str = include_str!("fixtures/aqhi.json");
const AQHI_RSS: &str = include_str!("fixtures/aqhi_rss.xml");
const POLLUTANTS_XML: &str = include_str!("fixtures/24pc.xml");
const HKO_JSON: &str = include_str!("fixtures/hko.json");

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
