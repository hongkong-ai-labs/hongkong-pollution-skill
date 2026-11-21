# SPEC — hongkong-pollution-skill

Build a Rust project in this directory that ships **one binary, `hkpollution`**, usable two ways:

1. as a **CLI** for humans and scripts, and
2. as an **MCP server** over stdio (`hkpollution mcp`), so an AI agent can call it as tools.

Data comes only from official Hong Kong government open-data endpoints (no API keys).

---

## 1. Sources (already verified live — use these exact URLs)

- AQHI, per station, JSON:
  `https://dashboard.data.gov.hk/api/aqhi-individual?format=json`
  Returns an array: `[{"station":"Central/Western","aqhi":4,"health_risk":"Moderate","publish_date":"2026-11-21T17:30:00"}, ...]`
  18 stations: Central/Western, Southern, Eastern, Kwun Tong, Sham Shui Po, Kwai Chung, Tsuen Wan,
  Tseung Kwan O, Yuen Long, Tuen Mun, Tung Chung, Tai Po, Sha Tin, North, Tap Mun,
  Causeway Bay, Central, Mong Kok (the last three are roadside stations).

- AQHI, fallback when the JSON host fails, RSS/XML:
  `https://www.aqhi.gov.hk/epd/ddata/html/out/aqhi_ind_rss_Eng.xml`
  One `<item>` per station with `<title>`, `<description>` (contains the AQHI and risk) or similar
  per-item fields; write a tolerant parser that scans for station names and integers in the text.

- Past 24 h pollutant concentrations, XML:
  `https://www.aqhi.gov.hk/epd/ddata/html/out/24pc_Eng.xml`
  Structure:
  `<AQHI24HrPollutantConcentration><lastBuildDate>Sat, 21 Nov 2026 17:30:00 +0800</lastBuildDate>`
  then 432 × `<PollutantConcentration>` blocks of
  `<StationName>Central/Western</StationName><DateTime>Fri, 20 Nov 2026 18:00:00 +0800</DateTime>
  <NO2>47.8</NO2><O3>45.6</O3><SO2>1.0</SO2><CO>-</CO><PM10>14.7</PM10><PM2.5>10.8</PM2.5>`
  Units are µg/m³. `-` means "not measured at that station" → `null`. Values can look like `.8`
  (no leading zero) → parse them as 0.8. The file is served with a UTF-8 **BOM**; strip it.

- HKO current readings, JSON:
  `https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en`
  Relevant keys: `updateTime` (ISO-8601 with +08:00), `temperature.data[]` (`place`,`value`,`unit`),
  `humidity.data[]`, `rainfall.data[]`, `warningMessage[]` (may be absent), `uvindex`.
  Use only `temperature`, `humidity`, `updateTime`, `warningMessage`.

---

## 2. CLI contract

```
hkpollution aqhi       [--station <name>] [--json]
hkpollution pollutants [--station <name>] [--hours <1-24>] [--json]
hkpollution summary    [--json] [--with-weather]
hkpollution weather    [--json]
hkpollution mcp
hkpollution --help | --version
```

- Default output is an aligned human table with a one-line "source:" footer naming EPD/HKO.
- `--json` prints a single JSON object (no logs, no footer) to stdout — safe to pipe to `jq`.
- `--station` is a case-insensitive substring match; zero matches is an error (exit 1) that lists
  the valid station names. Ambiguous match (e.g. "Central" matching Central and Central/Western)
  returns all matches.
- `--hours N` keeps the newest N hourly readings per station (default 1 = latest only, max 24).
- `summary` = per-station latest AQHI, the network worst/mean, risk-level counts, the worst
  PM2.5 and NO2 station for the latest hour, and (with `--with-weather`) HKO temperature,
  humidity and any active warnings.
- Errors go to stderr; exit code 2 = usage error, 1 = network/upstream/io failure, 0 = success.
- Unknown flag or command → usage error, plus a short usage block.

## 3. MCP contract

`hkpollution mcp` speaks JSON-RPC 2.0 over stdio, one JSON object per line, UTF-8, in and out.

Implement: `initialize` (echo protocolVersion `2024-11-05`, `capabilities.tools = {}`,
`serverInfo {name: "hongkong-pollution-skill", version}`), `notifications/initialized` (no reply),
`ping`, `tools/list`, `tools/call`. Unknown method → JSON-RPC error `-32601`; unparsable line →
`-32700`. Never crash on a bad request; keep serving until stdin closes.

Tools (each result is `{"content":[{"type":"text","text":"<pretty JSON>"}],"isError":false}`):

| tool | arguments | returns |
|---|---|---|
| `air_quality_aqhi` | `{ "station"?: string }` | current AQHI per station, filtered when `station` given |
| `pollutant_concentrations` | `{ "station"?: string, "hours"?: integer 1-24 }` | latest readings plus the requested hourly series, per station |
| `pollution_summary` | `{ "include_weather"?: boolean }` | the CLI `summary` payload |
| `hong_kong_weather` | `{}` | the CLI `weather` payload |

Upstream failure inside `tools/call` → `isError: true` with a readable message, not a transport error.

## 4. JSON shapes (CLI `--json` must match these)

```json
{"stations":[{"station":"Central/Western","aqhi":4,"health_risk":"Moderate","publish_date":"2026-11-21T17:30:00"}],"source":"EPD","fetched_at":"2026-11-21T18:05:00+08:00"}
```
```json
{"last_build_date":"Sat, 21 Nov 2026 17:30:00 +0800","hours":24,
 "stations":[{"station":"Central/Western",
   "latest":{"time":"2026-11-20T18:00:00+08:00","no2":47.8,"o3":45.6,"so2":1.0,"co":null,"pm10":14.7,"pm25":10.8},
   "series":[{"time":"2026-11-20T18:00:00+08:00","no2":47.8,"o3":45.6,"so2":1.0,"co":null,"pm10":14.7,"pm25":10.8}],
   "averages":{"no2":41.2,"pm25":9.4}}],"source":"EPD"}
```
`averages` averages the requested window, skipping nulls; omit a key when every value is null.

## 5. Code requirements

- Rust **edition 2024**; crate `hongkong-pollution-skill`, binary `hkpollution`.
- Dependencies limited to `serde` (derive), `serde_json`, `ureq` (tls + gzip). Standard library for
  everything else — in particular **no** `clap`, `reqwest`, `tokio`, `chrono`, or XML crates.
  Parse XML with a small hand-written tag scanner; no regex crate.
- Modules: `lib.rs` exports, `error.rs`, `http.rs`, `model.rs`, `parse.rs` (pure, no I/O),
  `aqhi.rs`, `pollutants.rs`, `weather.rs`, `mcp.rs`, `main.rs`. Keep parsing separate from
  fetching so it is testable offline.
- No `unwrap()`/`expect()` outside tests and `main`'s top level; return the error type.
- Timeouts on every HTTP call (≈10 s connect, ≈20 s read) and a sensible User-Agent.
- Rustdoc on public items; `cargo fmt` and `cargo clippy -- -D warnings` must both be clean.

## 6. Tests (must pass with no network)

- Fixture tests for the AQHI JSON array, the AQHI RSS fallback, the 24pc XML (including `-`,
  `.8`, and the BOM) and the HKO JSON — keep trimmed samples as `tests/fixtures/*`.
- Rollup tests: `--hours` windowing, null skipping in averages, worst/mean station selection.
- Station matching tests: substring, case-insensitive, ambiguous, no match.
- An integration test that spawns the built binary with `mcp`, sends `initialize` then `tools/list`,
  and asserts the server names all four tools and answers valid JSON-RPC.

## 7. Extras

- `README.md`: what it is, the data sources with attribution, install (`cargo build --release`),
  every CLI command with a real example, an MCP client config snippet for stdio, and the JSON shapes.
- `LICENSE` (MIT, "hongkong-ai-labs"), `.gitignore` (`/target`), and a GitHub Actions workflow
  `.github/workflows/ci.yml` running fmt, clippy and test on ubuntu-latest with a stable toolchain.
- Do not invent endpoints or fields. If something in this spec is genuinely ambiguous, pick the
  simplest reading, write the choice in `NOTES.md`, and continue.
