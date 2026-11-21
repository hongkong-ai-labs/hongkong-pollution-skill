//! JSON-RPC 2.0 over stdio: the `hkpollution mcp` server.
//!
//! One JSON object per line in, one JSON object per line out. Unknown methods
//! answer `-32601`, unparsable lines answer `-32700`, and the loop never exits
//! on a bad request — it keeps serving until stdin closes.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::model::resolve_station;
use crate::{VERSION, aqhi, epd_app, pollutants, weather};

/// MCP protocol revision this server implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Serve JSON-RPC on stdin/stdout until stdin reaches EOF.
///
/// # Errors
///
/// Returns [`Error::Io`] when stdin or stdout fails.
pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(&line) {
            let encoded = serde_json::to_string(&response)?;
            writeln!(stdout, "{encoded}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Turn one input line into an optional response.
fn handle_line(line: &str) -> Option<Value> {
    match serde_json::from_str::<Value>(line) {
        Ok(request) => dispatch(&request),
        Err(error) => Some(error_response(
            None,
            PARSE_ERROR,
            &format!("parse error: {error}"),
        )),
    }
}

fn dispatch(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    if !request.is_object() {
        return Some(error_response(
            id,
            INVALID_REQUEST,
            "invalid request: expected a JSON object",
        ));
    }
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(error_response(
            id,
            INVALID_REQUEST,
            "invalid request: missing method",
        ));
    };
    if method.starts_with("notifications/") {
        return None;
    }
    match method {
        "initialize" => Some(success(id, initialize_result())),
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, tools_list())),
        "tools/call" => Some(call_tool(id, request.get("params"))),
        other => Some(error_response(
            id,
            METHOD_NOT_FOUND,
            &format!("method not found: {other}"),
        )),
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "hongkong-pollution-skill", "version": VERSION }
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "air_quality_aqhi",
                "description": "Current Air Quality Health Index (AQHI) for every Hong Kong EPD \
                                monitoring station, optionally filtered by station name.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "station": {
                            "type": "string",
                            "description": "Case-insensitive station name substring, e.g. \"Mong Kok\"."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "pollutant_concentrations",
                "description": "Past 24-hour NO2, O3, SO2, CO, PM10 and PM2.5 concentrations \
                                (µg/m³) per station, with the requested hourly window and averages.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "station": {
                            "type": "string",
                            "description": "Case-insensitive station name substring."
                        },
                        "hours": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 24,
                            "description": "Newest hourly readings to keep per station (default 1)."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "pollution_summary",
                "description": "Network summary: worst and mean AQHI, risk-level counts, worst \
                                PM2.5 and NO2 stations, optionally with HKO weather.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "include_weather": {
                            "type": "boolean",
                            "description": "Also include HKO temperature, humidity and warnings."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "hong_kong_weather",
                "description": "Current HKO temperature and humidity observations plus any \
                                active weather warnings.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "aqhi_forecast",
                "description": "EPD app API forecast of the AQHI range for the next general and \
                                roadside periods (undocumented endpoint).",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "station_history",
                "description": "EPD app API hourly AQHI and pollutant history per station, \
                                newest first; defaults to every station's latest reading.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "station": {
                            "type": "string",
                            "description": "Case-insensitive station name substring."
                        },
                        "hours": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 23,
                            "description": "Newest hourly table window (default 1)."
                        }
                    },
                    "additionalProperties": false
                }
            }
        ]
    })
}

fn call_tool(id: Option<Value>, params: Option<&Value>) -> Value {
    let Some(params) = params.filter(|value| value.is_object()) else {
        return error_response(id, INVALID_PARAMS, "invalid params: expected an object");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(id, INVALID_PARAMS, "invalid params: missing tool name");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let outcome = match name {
        "air_quality_aqhi" => tool_aqhi(&arguments),
        "pollutant_concentrations" => tool_pollutants(&arguments),
        "pollution_summary" => tool_summary(&arguments),
        "hong_kong_weather" => tool_weather(),
        "aqhi_forecast" => tool_forecast(),
        "station_history" => tool_history(&arguments),
        other => {
            return error_response(id, INVALID_PARAMS, &format!("unknown tool: {other}"));
        }
    };
    success(id, tool_payload(outcome))
}

fn tool_payload(outcome: Result<Value>) -> Value {
    match outcome {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value)
                .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"));
            json!({
                "content": [{ "type": "text", "text": text }],
                "isError": false
            })
        }
        Err(error) => json!({
            "content": [{ "type": "text", "text": error.to_string() }],
            "isError": true
        }),
    }
}

fn tool_aqhi(arguments: &Value) -> Result<Value> {
    let mut report = aqhi::fetch()?;
    if let Some(query) = arguments.get("station").and_then(Value::as_str) {
        let names = resolve_station(query)?;
        aqhi::retain_stations(&mut report.stations, &names);
    }
    Ok(serde_json::to_value(&report)?)
}

fn tool_pollutants(arguments: &Value) -> Result<Value> {
    let hours = match arguments.get("hours") {
        None | Some(Value::Null) => 1,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("hours must be an integer between 1 and 24"))?,
    };
    if hours == 0 || hours > pollutants::MAX_HOURS {
        return Err(Error::usage("hours must be between 1 and 24"));
    }
    let mut report = pollutants::fetch(hours)?;
    if let Some(query) = arguments.get("station").and_then(Value::as_str) {
        let names = resolve_station(query)?;
        pollutants::retain_stations(&mut report, &names);
    }
    Ok(serde_json::to_value(&report)?)
}

fn tool_summary(arguments: &Value) -> Result<Value> {
    let include_weather = arguments
        .get("include_weather")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let report = aqhi::summary(include_weather)?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_weather() -> Result<Value> {
    let report = weather::fetch()?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_forecast() -> Result<Value> {
    let report = epd_app::fetch_forecast()?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_history(arguments: &Value) -> Result<Value> {
    let hours = match arguments.get("hours") {
        None | Some(Value::Null) => 1,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("hours must be an integer between 1 and 23"))?,
    };
    if hours == 0 || hours > epd_app::MAX_HOURS {
        return Err(Error::usage("hours must be between 1 and 23"));
    }
    let station = arguments.get("station").and_then(Value::as_str);
    let report = epd_app::fetch_history(station, hours)?;
    Ok(serde_json::to_value(&report)?)
}

fn success(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result
    })
}

fn error_response(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}
