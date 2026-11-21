//! CLI argument handling for the app API commands. No network access: every
//! case here is rejected before a request is made.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use hongkong_pollution_skill::{aqhi, cache};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hkpollution"))
        .args(args)
        .output()
        .expect("run hkpollution")
}

fn run_with_cache(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hkpollution"))
        .args(args)
        .env("HKPOLLUTION_CACHE_DIR", dir)
        .env("HKPOLLUTION_RETRIES", "1")
        .output()
        .expect("run hkpollution")
}

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hkpollution-cli-test-{}-{tag}", std::process::id()))
}

/// Seed a fresh cache entry so a command can run without touching the network.
fn seed_cache(dir: &Path, url: &str, body: &str) {
    std::fs::create_dir_all(dir).expect("create cache dir");
    let path = dir.join(format!("{:016x}.json", cache::hash(url)));
    let stored_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after the epoch")
        .as_secs();
    let envelope = serde_json::json!({
        "url": url,
        "stored_at": stored_at,
        "body": body,
    });
    std::fs::write(path, envelope.to_string()).expect("write cache envelope");
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
fn fail_above_rejects_values_outside_one_to_eleven() {
    for value in ["0", "12", "-1", "abc"] {
        let output = run(&["aqhi", "--fail-above", value]);
        assert_eq!(output.status.code(), Some(2), "--fail-above {value}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("--fail-above"));
    }

    for command in ["weather", "forecast", "pollutants"] {
        let output = run(&[command, "--fail-above", "5"]);
        assert_eq!(output.status.code(), Some(2), "{command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("--fail-above is only valid"));
    }
}

#[test]
fn near_rejects_malformed_out_of_bounds_and_combined_flags() {
    for value in [
        "22.33",
        "22.0,114.16",
        "22.33,114.9",
        "north,114.16",
        "22.33,west",
    ] {
        let output = run(&["aqhi", "--near", value]);
        assert_eq!(output.status.code(), Some(2), "--near {value}");
    }

    let combined = run(&["aqhi", "--near", "22.33,114.16", "--station", "Central"]);
    assert_eq!(combined.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&combined.stderr).contains("cannot be combined"));

    for args in [
        vec!["pollutants", "--near", "22.33,114.16"],
        vec!["street", "--near", "22.33,114.16"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("--near is only valid"));
    }
}

#[test]
fn fail_above_exits_three_on_a_cached_payload() {
    let dir = temp_dir("fail-above");
    let _ = std::fs::remove_dir_all(&dir);
    seed_cache(
        &dir,
        aqhi::AQHI_JSON_URL,
        include_str!("fixtures/aqhi.json"),
    );

    // The fixture's worst station is Mong Kok at AQHI 8.
    let reached = run_with_cache(&["aqhi", "--fail-above", "8", "--json"], &dir);
    assert_eq!(reached.status.code(), Some(3));
    let payload: serde_json::Value =
        serde_json::from_slice(&reached.stdout).expect("JSON is printed before exiting 3");
    assert_eq!(payload["source"], "EPD");

    let below = run_with_cache(&["aqhi", "--fail-above", "9", "--json"], &dir);
    assert_eq!(below.status.code(), Some(0));

    let plain = run_with_cache(&["aqhi", "--json"], &dir);
    assert_eq!(plain.status.code(), Some(0), "the flag is opt-in");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn near_resolves_the_nearest_station_in_the_footer() {
    let dir = temp_dir("near");
    let _ = std::fs::remove_dir_all(&dir);
    seed_cache(
        &dir,
        aqhi::AQHI_JSON_URL,
        include_str!("fixtures/aqhi.json"),
    );

    let output = run_with_cache(&["aqhi", "--near", "22.3226,114.1683"], &dir);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("station: Mong Kok (0.0 km away)"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("Causeway Bay"),
        "only the nearest station is listed: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_flags_are_rejected_where_they_do_not_apply() {
    let doctor = run(&["doctor", "--no-cache"]);
    assert_eq!(doctor.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&doctor.stderr).contains("--no-cache is only valid"));

    let bad_age = run(&["aqhi", "--max-age", "soon"]);
    assert_eq!(bad_age.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad_age.stderr).contains("--max-age"));
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
