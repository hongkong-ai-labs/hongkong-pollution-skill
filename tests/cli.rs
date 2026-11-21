//! CLI argument handling for the app API commands. No network access: every
//! case here is rejected before a request is made.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hkpollution"))
        .args(args)
        .output()
        .expect("run hkpollution")
}

#[test]
fn help_lists_the_app_api_commands() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hkpollution forecast"));
    assert!(stdout.contains("hkpollution history"));
    assert!(stdout.contains("hkpollution street"));
}

#[test]
fn history_hours_outside_the_window_is_a_usage_error() {
    for hours in ["0", "24", "abc"] {
        let output = run(&["history", "--hours", hours]);
        assert_eq!(output.status.code(), Some(2), "--hours {hours}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("1 and 23"),
            "--hours {hours} stderr: {stderr}"
        );
    }
}

#[test]
fn street_requires_valid_coordinates() {
    let missing = run(&["street"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--lat and --lng"));

    let only_lat = run(&["street", "--lat", "22.3"]);
    assert_eq!(only_lat.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&only_lat.stderr).contains("--lat and --lng"));

    for (lat, lng) in [("22.0", "114.2"), ("22.3", "114.9")] {
        let output = run(&["street", "--lat", lat, "--lng", lng]);
        assert_eq!(output.status.code(), Some(2), "{lat},{lng}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("Hong Kong bounds"));
    }

    let non_numeric = run(&["street", "--lat", "north", "--lng", "114.2"]);
    assert_eq!(non_numeric.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&non_numeric.stderr).contains("--lat must be a number"));

    let hours = run(&["street", "--lat", "22.3", "--lng", "114.2", "--hours", "49"]);
    assert_eq!(hours.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&hours.stderr).contains("1 and 48"));

    let station = run(&[
        "street",
        "--lat",
        "22.3",
        "--lng",
        "114.2",
        "--station",
        "Central",
    ]);
    assert_eq!(station.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&station.stderr).contains("--station is only valid"));
}

#[test]
fn forecast_rejects_station_and_hours() {
    let station = run(&["forecast", "--station", "Mong Kok"]);
    assert_eq!(station.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&station.stderr).contains("--station is only valid"));

    let hours = run(&["forecast", "--hours", "2"]);
    assert_eq!(hours.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&hours.stderr).contains("--hours is only valid"));
}
