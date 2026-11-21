//! Spawns the real binary in MCP mode and checks JSON-RPC framing.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_hkpollution"))
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn hkpollution mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            child,
            stdin: Some(stdin),
            stdout,
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        writeln!(stdin, "{message}").expect("write request");
        stdin.flush().expect("flush request");
    }

    fn receive(&mut self) -> Value {
        let mut line = String::new();
        let read = self.stdout.read_line(&mut line).expect("read response");
        assert!(read > 0, "server closed stdout before answering");
        serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("non-JSON line {line:?}: {error}"))
    }

    fn close_stdin(&mut self) {
        drop(self.stdin.take());
    }
}

#[test]
fn mcp_answers_initialize_tools_list_ping_and_unknown_methods() {
    let mut server = Server::start();

    server.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "hkpollution-tests", "version": "0.0.0" }
        }
    }));
    // A notification must not produce a reply, so the next line read is for id 2.
    server.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    server.send(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    server.send(&json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }));
    server.send(&json!({ "jsonrpc": "2.0", "id": 4, "method": "no/such/method" }));
    server.send(&json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "nope" }
    }));
    server.send(&json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "station_history", "arguments": { "hours": 24 } }
    }));

    let initialize = server.receive();
    assert_eq!(initialize["jsonrpc"], "2.0");
    assert_eq!(initialize["id"], 1);
    assert_eq!(initialize["result"]["protocolVersion"], "2024-11-05");
    assert!(initialize["result"]["capabilities"]["tools"].is_object());
    assert_eq!(
        initialize["result"]["serverInfo"]["name"],
        "hongkong-pollution-skill"
    );
    assert_eq!(
        initialize["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );

    let tools_list = server.receive();
    assert_eq!(tools_list["id"], 2);
    let tools = tools_list["result"]["tools"]
        .as_array()
        .expect("tools array");
    let mut names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "air_quality_aqhi",
            "aqhi_forecast",
            "hong_kong_weather",
            "pollutant_concentrations",
            "pollution_summary",
            "station_history"
        ]
    );
    for tool in tools {
        assert!(tool["description"].is_string(), "every tool is described");
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    let forecast = tools
        .iter()
        .find(|tool| tool["name"] == "aqhi_forecast")
        .unwrap();
    assert!(
        forecast["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .is_empty(),
        "aqhi_forecast takes no arguments"
    );
    let history = tools
        .iter()
        .find(|tool| tool["name"] == "station_history")
        .unwrap();
    assert_eq!(history["inputSchema"]["properties"]["hours"]["maximum"], 23);
    assert_eq!(
        history["inputSchema"]["properties"]["station"]["type"],
        "string"
    );

    let ping = server.receive();
    assert_eq!(ping["id"], 3);
    assert!(ping["result"].is_object());
    assert!(ping.get("error").is_none());

    let unknown = server.receive();
    assert_eq!(unknown["id"], 4);
    assert_eq!(unknown["error"]["code"], -32601);
    assert!(unknown["error"]["message"].is_string());

    let bad_tool = server.receive();
    assert_eq!(bad_tool["id"], 5);
    assert_eq!(bad_tool["error"]["code"], -32602);

    // Out-of-range hours is rejected before any network call, so this stays offline.
    let bad_hours = server.receive();
    assert_eq!(bad_hours["id"], 6);
    assert_eq!(bad_hours["result"]["isError"], true);
    let message = bad_hours["result"]["content"][0]["text"]
        .as_str()
        .expect("error text");
    assert!(message.contains("23"), "message: {message}");

    server.close_stdin();
    let status = server.child.wait().expect("server exits on EOF");
    assert!(status.success(), "server exit status: {status}");
}

#[test]
fn mcp_reports_parse_errors_and_keeps_serving() {
    let mut server = Server::start();
    writeln!(server.stdin.as_mut().expect("stdin"), "this is not json").expect("write bad line");
    server
        .stdin
        .as_mut()
        .expect("stdin")
        .flush()
        .expect("flush bad line");

    let parse_error = server.receive();
    assert_eq!(parse_error["jsonrpc"], "2.0");
    assert!(parse_error["id"].is_null());
    assert_eq!(parse_error["error"]["code"], -32700);

    server.send(&json!({ "jsonrpc": "2.0", "id": "after", "method": "ping" }));
    let ping = server.receive();
    assert_eq!(ping["id"], "after");
    assert!(ping["result"].is_object());

    server.close_stdin();
    assert!(server.child.wait().expect("server exits").success());
}
