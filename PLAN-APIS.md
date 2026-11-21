# PLAN: add the app-API data sources to `hkpollution`

Implement in two bounded chunks. Take the simplest reading of anything ambiguous and
record the choice in NOTES.md. Add no new dependencies: `serde`, `serde_json` and `ureq`
only, and no date-time crate (the existing `+08:00` arithmetic stays).

The recovered API surface is documented in `~/Development/hk-air-apps-reversing/FINDINGS.md`
with live samples in that directory's `samples/`. Read it first. Everything below was
re-verified against the live hosts on 2026-11-21.

## Ground rules

- The documented open-data feeds stay the primary source for `aqhi`, `pollutants`,
  `summary` and `weather`. Do not swap them out.
- The new commands are additive and are labelled as coming from an app API:
  `source: "EPD"` keeps its current meaning, the new EPD feed reports
  `"source": "EPD app API"`, and PRAISE reports `"source": "PRAISE-HK"`.
- These endpoints are undocumented. Put every request behind the existing `http.rs`
  timeouts, send the same User-Agent, and let failures surface as the usual `Error`
  (exit 1) or an MCP `isError: true` result — never a panic.
- Attribution: EPD data © Government of the HKSAR; PRAISE-HK street-level model
  © HKUST Institute for the Environment. Both must appear in README, SKILL.md and on
  the `source:` footer line.

## Chunk A — EPD app API: forecast and per-station history

### A.1 `GET https://app.epd-asmg.gov.hk/api/v2/forecast`

```json
{"general_stations":[{"min":0,"max":1,"forecast_period_en":"Tomorrow A.M.",
                      "forecast_period_tc":"明天 上午","forecast_period_sc":"明天 上午"}],
 "roadside_stations":[ ...same shape... ]}
```

New command:

```
hkpollution forecast [--json]
```

Table:

```
Period         General  Roadside
-------------  -------  --------
Tomorrow A.M.  0-1      0-1
Tomorrow P.M.  0-1      0-1

source: EPD app API (fetched 2026-11-21T19:30:00+08:00)
```

JSON:

```json
{"periods":[{"period":"Tomorrow A.M.","general":{"min":0,"max":1},"roadside":{"min":0,"max":1}}],
 "source":"EPD app API","fetched_at":"..."}
```

Join the two lists by position and by `forecast_period_en`; if they differ in length,
emit the periods that exist in either and use `null` for the missing side.

### A.2 `GET https://app.epd-asmg.gov.hk/api/v2/monitors` and `/monitors/{id}`

`/monitors` is an array of 18 objects:

```json
[{"id":66,"name":"深水埗","category":0,"latitude":22.330099,"longitude":114.158867,
  "district_id":0,"aqhi":4,"o3":58.6,"no2":56.4,"so2":5.7,"co":null,
  "pm10":15.1,"pm25":7.8,"updated_at":"2026-11-21T18:00:00+08:00"}]
```

`/monitors/{id}` adds `aqhi_histories`, `o3_histories`, `no2_histories`, `so2_histories`,
`co_histories`, `pm10_histories`, `pm25_histories` — 23 entries each, newest first,
each `{"created_at":"...","value":4}`.

Station names are Traditional Chinese only, so add a static id mapping in `model.rs`,
verified against this live list:

| id | Chinese | canonical English name | category |
| --- | --- | --- | --- |
| 66 | 深水埗 | Sham Shui Po | 0 |
| 69 | 大埔 | Tai Po | 0 |
| 70 | 元朗 | Yuen Long | 0 |
| 71 | 銅鑼灣 | Causeway Bay | 1 |
| 72 | 葵涌 | Kwai Chung | 0 |
| 73 | 東區 | Eastern | 0 |
| 74 | 觀塘 | Kwun Tong | 0 |
| 75 | 沙田 | Sha Tin | 0 |
| 76 | 塔門 | Tap Mun | 0 |
| 77 | 荃灣 | Tsuen Wan | 0 |
| 78 | 東涌 | Tung Chung | 0 |
| 79 | 中環 | Central | 1 |
| 80 | 中西區 | Central/Western | 0 |
| 81 | 旺角 | Mong Kok | 1 |
| 82 | 屯門 | Tuen Mun | 0 |
| 83 | 將軍澳 | Tseung Kwan O | 0 |
| 84 | 南區 | Southern | 0 |
| 85 | 北區 | North | 0 |

Resolve `--station` with the existing substring matcher against the canonical names,
then pick the id from this table; an ambiguous or unknown name behaves exactly as it
does today.

New command:

```
hkpollution history [--station <name>] [--hours <1-23>] [--json]
```

- Default: every station gets one row of the newest reading; `--station` narrows it.
- `--hours N` shows the newest N hourly entries per station in the table, and the full
  series in JSON, newest first.
- Table columns: Station, Time, AQHI, NO2, O3, SO2, CO, PM10, PM2.5 — same `-` for null
  and the same `avg N` row convention as `pollutants` when more than one hour is shown.

JSON:

```json
{"hours":1,"stations":[{"station":"Sham Shui Po","station_id":66,"category":"general",
  "latitude":22.330099,"longitude":114.158867,
  "latest":{"time":"2026-11-21T18:00:00+08:00","aqhi":4,"no2":56.4,"o3":58.6,"so2":5.7,
            "co":null,"pm10":15.1,"pm25":7.8},
  "series":[{"time":"2026-11-21T18:00:00+08:00","aqhi":4,"no2":56.4,"o3":58.6,"so2":5.7,
             "co":null,"pm10":15.1,"pm25":7.8}]}],
 "source":"EPD app API","fetched_at":"..."}
```

`category` is `"general"` for 0 and `"roadside"` for 1.

### A.3 MCP tools for chunk A

Add `aqhi_forecast` (`{}`) and `station_history` (`{"station"?: string, "hours"?: 1-23}`)
returning the same payloads pretty-printed. Leave the four existing tools untouched.

### A.4 Fixtures and tests

Save the live bodies you capture as `tests/fixtures/epd_forecast.json`,
`tests/fixtures/epd_monitors.json` and `tests/fixtures/epd_monitor_66.json` (run the
requests yourself with curl; keep them as-is, do not hand-edit). Unit-test the parsers,
the id→name mapping (all 18 entries map to a canonical station, no duplicates), the
forecast join, and the `--hours` window. Add CLI and MCP assertions in the existing
integration tests. Everything must pass with no network.

## Chunk B — PRAISE-HK street level

### B.1 `GET https://praise-web.ust.hk:8443/api/pollutant_index`

Query: `lat`, `lng`, `pids`, `t0`, `t1`.

- `pids`: `AQHIBN,AQHIER,AQHIPM,AQHIPM10,AQHIPM25,AQHINO2,AQHIO3,AQHISO2,NO2,PM2.5,PM10,O3,SO2`
  — use exactly this list. A 48-hour window returns 49 values per series.
- `t0`/`t1`: ISO-8601 with a `+08:00` offset, computed with the existing HK-time helper.
  For "current", send the same value for both bounds; for the forecast, `t0 = now` and
  `t1 = now + 48 h`. Both forms are verified working.
- Response: `{"AQHIBN":[4,...],"AQHINO2":[0.65,...],...,"isots":["2026-11-21T19:00+08:00",...],
  "ts":["2026112119",...],"status":0}`, values newest-first.
- Percent-encode the query values; write a tiny helper in `http.rs` if `ureq` does not
  provide one — no new crate.

New command:

```
hkpollution street --lat <22.x> --lng <114.x> [--hours <1-48>] [--json]
```

- Validates that `--lat` and `--lng` are present together, numeric, and inside
  Hong Kong bounds (lat 22.1–22.6, lng 113.8–114.5); anything else is a usage error
  (exit 2).
- Table: one row per hour with Time, AQHI, %AR, NO2, O3, PM10, PM2.5, SO2 — value `-`
  when null. Default `--hours 24`, maximum 48.
- `AQHIBN` is the AQHI and `AQHIER` the %AR; the pollutant series are the raw
  `NO2/O3/PM10/PM2.5/SO2` values from the same response. Round to one decimal.

JSON:

```json
{"location":{"lat":22.33751,"lng":114.263},
 "current":{"time":"2026-11-21T19:00+08:00","aqhi":4,"added_health_risk":6.3,
            "no2":58.2,"o3":45.6,"pm10":14.7,"pm25":10.8,"so2":1.0},
 "series":[...same shape, newest first...],
 "source":"PRAISE-HK","fetched_at":"..."}
```

### B.2 MCP tool

`street_level_aqhi` with `{"lat": number, "lng": number, "hours"?: 1-48}`. Same payload.

### B.3 Fixtures and tests

`tests/fixtures/praise_pollutant_index.json` from a real 48-hour request. Test the
latitude/longitude bounds validation (usage error), the mapping of `pids` to the JSON
keys, missing values (`null`), and the hours window.

## Chunk C — docs (do this last, in the same pass as chunk B)

- README: add the two new commands to the command table, a short "Forecast and
  street-level data" section with one real example each, the extra sources in the data
  table (marked as app APIs, undocumented), and the PRAISE-HK attribution line.
- SKILL.md: the command table, the two new MCP tools, and one sentence per new source.
  Keep the body under 500 lines and the frontmatter key set unchanged.
- CHANGELOG: a new `## [Unreleased]` → `### Added` entry for each new command and tool.
- NOTES.md: the decisions this plan left open (source labels, the `--hours` maxima, the
  id table being static, why the app APIs are additive rather than primary).

## Acceptance

- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all` and `cargo build --release` all pass.
- `just skill-check` passes.
- One live run of each new command against the real host, pasted into your report.
- No change to the existing four commands' output beyond the new source lines.
