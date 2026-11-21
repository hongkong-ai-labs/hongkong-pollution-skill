//! `doctor` probes against a local stub server. No external network access.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use hongkong_pollution_skill::doctor::{Probe, Schema, run};

/// Serve a fixed body per path on a loopback port and return the base URL.
fn stub(routes: &'static [(&'static str, &'static str)]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("stub address");
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = [0_u8; 2048];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("/");
            let body = routes
                .iter()
                .find(|(route, _)| *route == path)
                .map_or("{}", |(_, body)| *body);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{address}")
}

#[test]
fn doctor_json_has_the_documented_shape() {
    // One retry keeps the connection-refused probe quick.
    // SAFETY: this test binary only touches the variable in this one test.
    unsafe { std::env::set_var("HKPOLLUTION_RETRIES", "1") };

    let base = stub(&[
        ("/aqhi", "[{\"station\":\"Central\",\"aqhi\":4}]"),
        ("/rss", "<rss><channel><item>x</item></channel></rss>"),
    ]);
    let probes = vec![
        Probe {
            source: "data.gov.hk AQHI",
            url: format!("{base}/aqhi"),
            schema: Schema::FirstItem("station"),
            max_age_secs: None,
        },
        Probe {
            source: "aqhi.gov.hk RSS",
            url: format!("{base}/rss"),
            schema: Schema::Elements(&["item"]),
            max_age_secs: None,
        },
        Probe {
            source: "EPD app forecast",
            url: format!("{base}/forecast"),
            schema: Schema::Keys(&["general_stations"]),
            max_age_secs: None,
        },
        Probe {
            source: "unreachable",
            url: "http://127.0.0.1:1/nothing".to_string(),
            schema: Schema::Keys(&["AQHIBN"]),
            max_age_secs: None,
        },
    ];
    let report = run(&probes);
    assert!(!report.healthy());

    let value = serde_json::to_value(&report).expect("report serialises");
    let checks = value["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), 4);
    for check in checks {
        assert!(check["source"].is_string());
        assert!(check["status"].is_string());
        assert!(check["ms"].is_u64());
        assert!(check["detail"].is_string());
    }

    assert_eq!(checks[0]["status"], "ok");
    assert_eq!(checks[1]["status"], "ok");
    assert_eq!(
        checks[2]["status"], "schema:general_stations",
        "a shape mismatch names the missing key"
    );
    let failed = checks[3]["status"].as_str().expect("failed status");
    assert!(failed.starts_with("fail:"), "status: {failed}");

    assert_eq!(value["summary"]["ok"], 2);
    assert_eq!(value["summary"]["failed"], 2);
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert!(value["cache"].get("dir").is_some());
    assert!(value["cache"]["entries"].is_u64());
    assert!(value["cache"]["bytes"].is_u64());
    assert!(value["fetched_at"].is_string());
}
