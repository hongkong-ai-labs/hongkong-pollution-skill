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
fn forecast_rejects_station_and_hours() {
    let station = run(&["forecast", "--station", "Mong Kok"]);
    assert_eq!(station.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&station.stderr).contains("--station is only valid"));

    let hours = run(&["forecast", "--hours", "2"]);
    assert_eq!(hours.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&hours.stderr).contains("--hours is only valid"));
}
