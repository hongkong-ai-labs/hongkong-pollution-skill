//! Station matching tests: substring, case-insensitive, ambiguous, no match.

use hongkong_pollution_skill::model::{AQHI_STATIONS, resolve_station};

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
