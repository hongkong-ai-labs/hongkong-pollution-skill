//! JSON-RPC 2.0 over stdio: the `hkpollution mcp` server.
//!
//! One JSON object per line in, one JSON object per line out. Unknown methods
//! answer `-32601`, unparsable lines answer `-32700`, and the loop never exits
//! on a bad request — it keeps serving until stdin closes.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::advice::{self, Lang};
use crate::error::{Error, Result};
use crate::model::resolve_station;
use crate::{
    VERSION, aqhi, brief, digest, doctor, epd_app, http, pollutants, praise, radiation, recycling,
    redtide, stats, uv, watch, water, weather,
};

/// Newest MCP protocol revision this server implements.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Revisions this server still answers for older clients.
pub const SUPPORTED_VERSIONS: [&str; 2] = ["2025-06-18", "2024-11-05"];

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
        "initialize" => Some(success(id, initialize_result(request.get("params")))),
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

/// Answer `initialize`, echoing the client's revision when it is one we speak.
fn initialize_result(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|value| value.get("protocolVersion"))
        .and_then(Value::as_str);
    let version = requested
        .filter(|value| SUPPORTED_VERSIONS.contains(value))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "hongkong-pollution-skill", "version": VERSION }
    })
}

fn tools_list() -> Value {
    let mut value = json!({
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
            },
            {
                "name": "street_level_aqhi",
                "description": "PRAISE-HK street-level AQHI, added health risk and pollutant \
                                forecast at a Hong Kong latitude/longitude (undocumented endpoint).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude in Hong Kong (22.1-22.6)."
                        },
                        "lng": {
                            "type": "number",
                            "description": "Longitude in Hong Kong (113.8-114.5)."
                        },
                        "hours": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 48,
                            "description": "Hourly rows to return (default 24)."
                        }
                    },
                    "required": ["lat", "lng"],
                    "additionalProperties": false
                }
            },
            {
                "name": "air_quality_briefing",
                "description": "One-call briefing for a Hong Kong coordinate: place name, \
                                street-level AQHI and added health risk, the nearest EPD \
                                station's official AQHI, HKO weather, and the advice that \
                                applies to the worse of the two AQHI values.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": {
                            "type": "number",
                            "description": "Latitude in Hong Kong (22.1-22.6)."
                        },
                        "lng": {
                            "type": "number",
                            "description": "Longitude in Hong Kong (113.8-114.5)."
                        },
                        "no_cache": {
                            "type": "boolean",
                            "description": "Bypass the on-disk cache for this call."
                        }
                    },
                    "required": ["lat", "lng"],
                    "additionalProperties": false
                }
            },
            {
                "name": "health_advice",
                "description": "Official EPD precautionary actions per AQHI risk band, for the \
                                general public, people with heart or respiratory illnesses, \
                                children and the elderly, and outdoor workers. English is \
                                compiled in; tc/both read the EPD app API.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "risk": {
                            "type": "string",
                            "description": "low, moderate, high, very-high, serious, or an AQHI range such as 4-6."
                        },
                        "lang": {
                            "type": "string",
                            "enum": ["en", "tc", "both"],
                            "description": "Advice language (default en)."
                        },
                        "no_cache": {
                            "type": "boolean",
                            "description": "Bypass the on-disk cache for this call."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "beach_water_quality",
                "description": "Latest EPD beach water-quality grading for Hong Kong gazetted \
                                beaches, with district, grade, open/closed status, WGS84 \
                                coordinates and the most recent published E. coli sampling \
                                history (latest count per 100 mL, recent samples, geometric \
                                mean).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Beach name or district substring, e.g. \"Silver Mine\"."
                        },
                        "district": {
                            "type": "string",
                            "description": "District substring, e.g. \"Islands\"."
                        },
                        "grade": {
                            "type": "string",
                            "enum": ["good", "fair", "poor", "very-poor", "graded", "ungraded", "open"],
                            "description": "Keep only this grade band."
                        },
                        "lat": {
                            "type": "number",
                            "description": "Latitude in Hong Kong; with lng, sort beaches by distance."
                        },
                        "lng": {
                            "type": "number",
                            "description": "Longitude in Hong Kong; with lat, sort beaches by distance."
                        },
                        "history": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 10,
                            "description": "Extra published E. coli years per beach (default 0)."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "water_quality",
                "description": "EPD monthly river or marine water quality: dissolved oxygen \
                                and BOD5 per station, newest sampling month first. Not live.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["river", "marine"],
                            "description": "Which network to read (default river)."
                        },
                        "zone": {
                            "type": "string",
                            "description": "Water control zone substring, e.g. \"Deep Bay\"."
                        },
                        "station": {
                            "type": "string",
                            "description": "Station code or river name substring."
                        },
                        "months": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 60,
                            "description": "Months kept per station (default 6)."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "red_tide",
                "description": "AFCD red tide sightings: active reports and those within a \
                                recent window, optionally sorted by distance from a coordinate.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "days": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 3650,
                            "description": "Recent window in days (default 90)."
                        },
                        "active": {
                            "type": "boolean",
                            "description": "Keep only sightings with no dissipation date."
                        },
                        "lat": {
                            "type": "number",
                            "description": "Latitude in Hong Kong; with lng, sort by distance."
                        },
                        "lng": {
                            "type": "number",
                            "description": "Longitude in Hong Kong; with lat, sort by distance."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "radiation_level",
                "description": "HKO hourly ambient gamma radiation dose rate per station, \
                                in microsievert per hour, with each reading relative to the \
                                network median.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "station": {
                            "type": "string",
                            "description": "Station name substring, e.g. \"Tap Mun\"."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "uv_index",
                "description": "HKO latest 15-minute mean UV index with the WHO exposure band.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "air_quality_stats",
                "description": "AQHI statistics over a window: min, max, mean and hours at or \
                                above a threshold per station, from a local observation log \
                                topped up by the documented 24-hour feed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "days": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 365,
                            "description": "Window in days (default 30)."
                        },
                        "station": {
                            "type": "string",
                            "description": "Station substring."
                        },
                        "threshold": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 11,
                            "description": "Exceedance threshold (default 7)."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "station_comparison",
                "description": "Current AQHI ranked across stations, separated into general \
                                and roadside means so the comparison is fair.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "environment_digest",
                "description": "One line per Hong Kong environmental source (air, beach, red \
                                tide, radiation, river, UV, warnings); each part degrades \
                                independently.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "recycling",
                "description": "Nearest recyclable collection points with accepted waste types, \
                                opening hours and distance.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "lat": { "type": "number", "description": "Latitude in Hong Kong." },
                        "lng": { "type": "number", "description": "Longitude in Hong Kong." },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 50,
                            "description": "How many points to return (default 5)."
                        },
                        "type": {
                            "type": "string",
                            "description": "Waste type substring, e.g. \"Plastics\" or \"Glass\"."
                        }
                    },
                    "required": ["lat", "lng"],
                    "additionalProperties": false
                }
            },
            {
                "name": "environment_alerts",
                "description": "One alert evaluation across live sources: worst AQHI against a \
                                threshold, active red tides and radiation stations above twice \
                                the network median. Each source degrades independently.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "aqhi": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 11,
                            "description": "AQHI alert threshold (default 7)."
                        },
                        "red_tide": {
                            "type": "boolean",
                            "description": "Also alert on active red tide sightings."
                        },
                        "radiation": {
                            "type": "boolean",
                            "description": "Also alert on radiation above twice the median."
                        }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "source_health",
                "description": "Probe every upstream source once and report latency, failures, \
                                schema mismatches and stale payloads, plus cache statistics.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }
        ]
    });
    // Every tool is a read-only, idempotent query against public data, and every
    // result is a JSON object, so the hints and the permissive output schema are
    // identical for all of them.
    if let Some(tools) = value.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools.iter_mut() {
            if let Some(object) = tool.as_object_mut() {
                object.insert(
                    "annotations".to_string(),
                    json!({
                        "readOnlyHint": true,
                        "idempotentHint": true,
                        "openWorldHint": true
                    }),
                );
                object.insert(
                    "outputSchema".to_string(),
                    json!({ "type": "object", "additionalProperties": true }),
                );
            }
        }
    }
    value
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
    apply_call_cache(&arguments);
    let outcome = match name {
        "air_quality_aqhi" => tool_aqhi(&arguments),
        "pollutant_concentrations" => tool_pollutants(&arguments),
        "pollution_summary" => tool_summary(&arguments),
        "hong_kong_weather" => tool_weather(),
        "beach_water_quality" => tool_beach(&arguments),
        "aqhi_forecast" => tool_forecast(),
        "station_history" => tool_history(&arguments),
        "street_level_aqhi" => tool_street(&arguments),
        "air_quality_briefing" => tool_brief(&arguments),
        "health_advice" => tool_advice(&arguments),
        "water_quality" => tool_water(&arguments),
        "red_tide" => tool_red_tide(&arguments),
        "radiation_level" => tool_radiation(&arguments),
        "uv_index" => tool_uv(),
        "air_quality_stats" => tool_stats(&arguments),
        "station_comparison" => tool_compare(),
        "environment_digest" => serde_json::to_value(digest::build()).map_err(Error::from),
        "recycling" => tool_recycling(&arguments),
        "environment_alerts" => tool_alerts(&arguments),
        "source_health" => tool_source_health(),
        other => {
            return error_response(id, INVALID_PARAMS, &format!("unknown tool: {other}"));
        }
    };
    // A per-call cache override must not leak into the next call.
    http::configure_cache(false, None);
    success(id, tool_payload(outcome))
}

/// Honour the optional `no_cache` and `max_age` arguments of a single call.
fn apply_call_cache(arguments: &Value) {
    let no_cache = arguments
        .get("no_cache")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max_age = arguments.get("max_age").and_then(Value::as_u64);
    http::configure_cache(no_cache, max_age);
}

fn tool_payload(outcome: Result<Value>) -> Value {
    match outcome {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value)
                .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"));
            json!({
                "content": [{ "type": "text", "text": text }],
                "structuredContent": value,
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

fn tool_brief(arguments: &Value) -> Result<Value> {
    let lat = arguments
        .get("lat")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::usage("lat must be a number between 22.1 and 22.6"))?;
    let lng = arguments
        .get("lng")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::usage("lng must be a number between 113.8 and 114.5"))?;
    let report = brief::build(lat, lng)?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_advice(arguments: &Value) -> Result<Value> {
    let lang = match arguments.get("lang").and_then(Value::as_str) {
        Some(value) => Lang::parse(value)?,
        None => Lang::En,
    };
    let risk = match arguments.get("risk").and_then(Value::as_str) {
        Some(value) => Some(advice::parse_risk(value)?),
        None => None,
    };
    let report = advice::fetch(lang, risk)?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_source_health() -> Result<Value> {
    let report = doctor::run(&doctor::default_probes());
    Ok(serde_json::to_value(&report)?)
}

fn tool_weather() -> Result<Value> {
    let report = weather::fetch()?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_beach(arguments: &Value) -> Result<Value> {
    let history = match arguments.get("history") {
        None | Some(Value::Null) => 0,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("history must be an integer between 0 and 10"))?,
    };
    let mut report = crate::beach::fetch_with_history(history)?;
    if let Some(query) = arguments.get("name").and_then(Value::as_str) {
        crate::beach::retain_query(&mut report, query);
    }
    if let Some(query) = arguments.get("district").and_then(Value::as_str) {
        let needle = query.trim().to_lowercase();
        report
            .beaches
            .retain(|item| item.district.to_lowercase().contains(&needle));
        report.count = report.beaches.len();
    }
    if let Some(band) = arguments.get("grade").and_then(Value::as_str) {
        crate::beach::retain_grade(&mut report, band)?;
    }
    match (
        arguments.get("lat").and_then(Value::as_f64),
        arguments.get("lng").and_then(Value::as_f64),
    ) {
        (Some(lat), Some(lng)) => crate::beach::sort_by_distance(&mut report, lat, lng),
        (None, None) => {}
        _ => return Err(Error::usage("lat and lng must be given together")),
    }
    Ok(serde_json::to_value(&report)?)
}

fn tool_water(arguments: &Value) -> Result<Value> {
    let kind = match arguments.get("kind").and_then(Value::as_str) {
        None => water::Kind::River,
        Some(value) => water::Kind::parse(value)?,
    };
    let months = match arguments.get("months") {
        None | Some(Value::Null) => 6,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("months must be an integer between 1 and 60"))?,
    };
    let mut report = water::fetch(kind, months)?;
    if let Some(zone) = arguments.get("zone").and_then(Value::as_str) {
        water::retain_zone(&mut report, zone);
    }
    if let Some(query) = arguments.get("station").and_then(Value::as_str) {
        water::retain_query(&mut report, query);
    }
    Ok(serde_json::to_value(&report)?)
}

fn tool_red_tide(arguments: &Value) -> Result<Value> {
    let days = match arguments.get("days") {
        None | Some(Value::Null) => redtide::DEFAULT_DAYS,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("days must be an integer between 1 and 3650"))?,
    };
    let mut report = redtide::fetch(days)?;
    if arguments
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        redtide::retain_active(&mut report);
    }
    match (
        arguments.get("lat").and_then(Value::as_f64),
        arguments.get("lng").and_then(Value::as_f64),
    ) {
        (Some(lat), Some(lng)) => redtide::sort_by_distance(&mut report, lat, lng),
        (None, None) => {}
        _ => return Err(Error::usage("lat and lng must be given together")),
    }
    Ok(serde_json::to_value(&report)?)
}

fn tool_radiation(arguments: &Value) -> Result<Value> {
    let mut report = radiation::fetch()?;
    if let Some(query) = arguments.get("station").and_then(Value::as_str) {
        radiation::retain_query(&mut report, query);
    }
    Ok(serde_json::to_value(&report)?)
}

fn tool_recycling(arguments: &Value) -> Result<Value> {
    let lat = arguments
        .get("lat")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::usage("lat is required"))?;
    let lng = arguments
        .get("lng")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::usage("lng is required"))?;
    let limit = match arguments.get("limit") {
        None | Some(Value::Null) => recycling::DEFAULT_LIMIT,
        Some(value) => value
            .as_u64()
            .and_then(|raw| usize::try_from(raw).ok())
            .ok_or_else(|| Error::usage("limit must be an integer between 1 and 50"))?,
    };
    let waste = arguments.get("type").and_then(Value::as_str);
    Ok(serde_json::to_value(recycling::near(
        lat, lng, limit, waste,
    )?)?)
}

fn tool_alerts(arguments: &Value) -> Result<Value> {
    let mut thresholds = watch::Thresholds::default();
    if let Some(value) = arguments.get("aqhi") {
        thresholds.aqhi = value
            .as_i64()
            .ok_or_else(|| Error::usage("aqhi must be an integer between 1 and 11"))?;
    }
    thresholds.red_tide = arguments
        .get("red_tide")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    thresholds.radiation = arguments
        .get("radiation")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(serde_json::to_value(watch::check(&thresholds)?)?)
}

fn tool_uv() -> Result<Value> {
    Ok(serde_json::to_value(uv::fetch()?)?)
}

fn tool_stats(arguments: &Value) -> Result<Value> {
    let days = match arguments.get("days") {
        None | Some(Value::Null) => 30,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("days must be an integer between 1 and 365"))?,
    };
    let threshold = match arguments.get("threshold") {
        None | Some(Value::Null) => stats::DEFAULT_THRESHOLD,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| Error::usage("threshold must be an integer between 1 and 11"))?,
    };
    let station = arguments.get("station").and_then(Value::as_str);
    Ok(serde_json::to_value(stats::build(
        days, station, threshold,
    )?)?)
}

fn tool_compare() -> Result<Value> {
    Ok(serde_json::to_value(stats::compare()?)?)
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

fn tool_street(arguments: &Value) -> Result<Value> {
    let lat = arguments
        .get("lat")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::usage("lat must be a number"))?;
    let lng = arguments
        .get("lng")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::usage("lng must be a number"))?;
    let hours = match arguments.get("hours") {
        None | Some(Value::Null) => praise::DEFAULT_HOURS,
        Some(value) => value
            .as_u64()
            .and_then(|raw| u32::try_from(raw).ok())
            .ok_or_else(|| Error::usage("hours must be an integer between 1 and 48"))?,
    };
    if hours == 0 || hours > praise::MAX_HOURS {
        return Err(Error::usage("hours must be between 1 and 48"));
    }
    let report = praise::fetch(lat, lng, hours)?;
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
