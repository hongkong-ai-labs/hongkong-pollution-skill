//! Station matching tests: substring, case-insensitive, ambiguous, no match.

use hongkong_pollution_skill::model::{
    AQHI_STATIONS, EPD_APP_STATIONS, STATION_COORDS, app_category_label, app_station_id,
    app_station_name, haversine_km, nearest_station, resolve_station,
};

#[test]
fn station_list_has_eighteen_entries() {
    assert_eq!(AQHI_STATIONS.len(), 18);
    assert!(AQHI_STATIONS.contains(&"Mong Kok"));
    assert!(AQHI_STATIONS.contains(&"Central/Western"));
    assert!(AQHI_STATIONS.contains(&"Tap Mun"));
}

#[test]
fn substring_match_is_case_insensitive() {
    assert_eq!(resolve_station("mong kok").unwrap(), vec!["Mong Kok"]);
    assert_eq!(resolve_station("MONG").unwrap(), vec!["Mong Kok"]);
    assert_eq!(resolve_station("tseung").unwrap(), vec!["Tseung Kwan O"]);
    assert_eq!(resolve_station("  sha tin  ").unwrap(), vec!["Sha Tin"]);
}

#[test]
fn ambiguous_match_returns_all_matches() {
    let matches = resolve_station("Central").unwrap();
    assert_eq!(matches, vec!["Central/Western", "Central"]);

    let roadside = resolve_station("o").unwrap();
    assert!(roadside.len() > 2, "a loose substring returns every match");
}

#[test]
fn exact_match_still_works_for_slashed_names() {
    assert_eq!(
        resolve_station("central/western").unwrap(),
        vec!["Central/Western"]
    );
}

#[test]
fn no_match_lists_the_valid_stations() {
    let error = resolve_station("kownloon").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("no station matches"));
    assert!(message.contains("Central/Western"));
    assert!(message.contains("Mong Kok"));
    assert!(
        !error.is_usage(),
        "zero matches is a data error, exit code 1"
    );
}

#[test]
fn empty_query_is_a_usage_error() {
    let error = resolve_station("   ").unwrap_err();
    assert!(error.is_usage());
}

#[test]
fn station_coordinates_cover_every_canonical_station() {
    assert_eq!(STATION_COORDS.len(), AQHI_STATIONS.len());
    for station in STATION_COORDS {
        assert!(
            AQHI_STATIONS.contains(&station.station),
            "{} is not a canonical station name",
            station.station
        );
        assert!((22.1..=22.6).contains(&station.lat), "{}", station.station);
        assert!(
            (113.8..=114.5).contains(&station.lng),
            "{}",
            station.station
        );
    }
}

#[test]
fn nearest_station_resolves_a_known_point() {
    let (name, distance) = nearest_station(22.3301, 114.1589);
    assert_eq!(name, "Sham Shui Po");
    assert!(distance < 0.1, "station point distance: {distance}");

    let (name, distance) = nearest_station(22.3350, 114.1620);
    assert_eq!(name, "Sham Shui Po");
    assert!(
        (0.4..0.9).contains(&distance),
        "a northbound point should be under a kilometre away, got {distance}"
    );

    let (name, _) = nearest_station(22.2801, 114.1851);
    assert_eq!(name, "Causeway Bay");
}

#[test]
fn haversine_matches_a_short_known_distance() {
    let distance = haversine_km(22.3301, 114.1589, 22.3226, 114.1683);
    assert!(
        (1.1..1.5).contains(&distance),
        "Sham Shui Po to Mong Kok should be about 1.3 km, got {distance}"
    );
    assert_eq!(haversine_km(22.3301, 114.1589, 22.3301, 114.1589), 0.0);
}

#[test]
fn app_station_table_maps_every_id_to_a_canonical_name() {
    assert_eq!(EPD_APP_STATIONS.len(), 18);
    for station in EPD_APP_STATIONS {
        assert!(
            AQHI_STATIONS.contains(&station.name_en),
            "{} is not a canonical station name",
            station.name_en
        );
        assert_eq!(app_station_name(station.id), Some(station.name_en));
        assert_eq!(app_station_id(station.name_en), Some(station.id));
        assert!(matches!(station.category, 0 | 1));
    }

    let mut ids: Vec<i64> = EPD_APP_STATIONS.iter().map(|station| station.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 18, "monitor ids are unique");

    let mut names: Vec<&str> = EPD_APP_STATIONS
        .iter()
        .map(|station| station.name_en)
        .collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 18, "canonical names are unique");

    assert_eq!(app_station_id("sham shui po"), Some(66));
    assert_eq!(app_station_name(66), Some("Sham Shui Po"));
    assert_eq!(app_station_name(999), None);
    assert_eq!(app_category_label(0), "general");
    assert_eq!(app_category_label(1), "roadside");
    assert_eq!(
        EPD_APP_STATIONS
            .iter()
            .filter(|station| station.category == 1)
            .count(),
        3,
        "Causeway Bay, Central and Mong Kok are the roadside monitors"
    );
}
