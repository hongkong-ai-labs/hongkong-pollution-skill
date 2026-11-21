<div align="center">

<img src="assets/logo.svg" width="84" height="84" alt="hkpollution">

# hkpollution

**Hong Kong air quality in one binary.**

Current AQHI, pollutant concentrations, a network summary and HKO weather —
from official open data, as a CLI for humans and an MCP server for agents.

[![CI](https://github.com/hongkong-ai-labs/hongkong-pollution-skill/actions/workflows/ci.yml/badge.svg)](https://github.com/hongkong-ai-labs/hongkong-pollution-skill/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hongkong-ai-labs/hongkong-pollution-skill?style=flat-square&color=0e9488)](https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-0e9488?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-0e9488?style=flat-square&logo=rust&logoColor=white)](Cargo.toml)
[![MCP](https://img.shields.io/badge/MCP-stdio-0e9488?style=flat-square)](https://modelcontextprotocol.io)

</div>

```console
$ hkpollution summary --with-weather | tail -6
network: worst 4 (Central/Western) / mean 3.9
risk levels: Low 1, Moderate 17
worst PM2.5: Causeway Bay 19.6 µg/m³ (2026-11-21 18:00)
worst NO2: Causeway Bay 102.6 µg/m³ (2026-11-21 18:00)
weather: 2026-11-21T18:02:00+08:00 temperature 20.0 °C, humidity 47.0 %, warnings: The Fire Danger Warning is Red and the fire risk is extreme.

source: EPD / HKO
```

## Install

**As an agent skill** — installs `SKILL.md` plus a script that fetches the
matching binary, so an agent can answer Hong Kong air-quality questions with
nothing else wired up:

```sh
npx skills add hongkong-ai-labs/hongkong-pollution-skill
```

**Prebuilt binary** — macOS (Intel or Apple silicon) and Linux (x86_64 or
aarch64, static musl), checksum-verified, into `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/hongkong-ai-labs/hongkong-pollution-skill/main/scripts/install.sh | sh
```

**From source** — Rust 1.85 or newer, no C toolchain needed:

```sh
cargo install --git https://github.com/hongkong-ai-labs/hongkong-pollution-skill --locked
```

Whichever route you take:

```console
$ hkpollution --version
hkpollution 0.1.0
```

## What you get

| Command | Data | Output |
| --- | --- | --- |
| `hkpollution aqhi` | Current Air Quality Health Index for all 18 EPD stations, with risk band and publish time | table or `--json` |
| `hkpollution pollutants` | Past 24 h NO2, O3, SO2, CO, PM10, PM2.5 per station, with per-pollutant averages | table or `--json` |
| `hkpollution summary` | Network worst/mean AQHI, risk counts, worst PM2.5 and NO2 station, optional HKO weather | table or `--json` |
| `hkpollution weather` | HKO temperature, humidity and active warnings | table or `--json` |
| `hkpollution mcp` | The same four datasets as MCP tools over stdio | JSON-RPC |

Everything reads official endpoints only: **no API keys, no third-party
services, no scraping.** The dependency list is `serde`, `serde_json` and
`ureq`; the XML feeds are parsed by a small hand-written tag scanner, and the
release binaries are static and stripped.

## Usage

```console
$ hkpollution aqhi --station "mong kok"
Station   AQHI  Risk      Published
--------  ----  --------  ----------------
Mong Kok  4     Moderate  2026-11-21 17:30

source: EPD Air Quality Health Index (fetched 2026-11-21T18:33:13+08:00)
```

`--station` is a case-insensitive substring match, so `central` deliberately
returns both stations it matches:

```console
$ hkpollution aqhi --station central --json | jq -r '.stations[].station'
Central/Western
Central
```

Hourly pollutant series — the newest three hours, with the window average:

```console
$ hkpollution pollutants --station "mong kok" --hours 3
Station   Time              NO2    O3    SO2  CO     PM10  PM2.5
--------  ----------------  -----  ----  ---  -----  ----  -----
Mong Kok  2026-11-21 16:00  102.6  31.2  6.1  557.9  25.7  15.1
Mong Kok  2026-11-21 17:00  94.6   36.1  5.9  511.9  26.7  15.2
Mong Kok  2026-11-21 18:00  97.0   34.2  5.7  475.8  22.2  11.8
Mong Kok  avg 3             98.1   33.8  5.9  515.2  24.9  14.0

source: EPD 24-hour pollutant concentrations in µg/m³ (last build: Sat, 21 Nov 2026 18:30:00 +0800)
```

A `-` in the table means the station does not measure that pollutant; in JSON
the value is `null`, and the average key is omitted when every reading in the
window is null.

### Exit codes

| Exit | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Network, upstream or I/O failure — including a `--station` that matches nothing |
| `2` | Usage error (unknown command or flag, bad `--hours`) |

Errors always go to stderr, so `--json` output stays pipeable.

```console
$ hkpollution aqhi --station kownloon
hkpollution: no station matches "kownloon"; valid stations: Central/Western, Southern, Eastern, Kwun Tong, Sham Shui Po, Kwai Chung, Tsuen Wan, Tseung Kwan O, Yuen Long, Tuen Mun, Tung Chung, Tai Po, Sha Tin, North, Tap Mun, Causeway Bay, Central, Mong Kok
$ echo $?
1
```

## MCP server

`hkpollution mcp` speaks JSON-RPC 2.0 over stdio — one JSON object per line,
protocol revision `2024-11-05`. Point any stdio-capable MCP client at the
binary:

```json
{
  "mcpServers": {
    "hongkong-pollution": {
      "command": "/absolute/path/to/hkpollution",
      "args": ["mcp"]
    }
  }
}
```

| Tool | Arguments | Returns |
| --- | --- | --- |
| `air_quality_aqhi` | `{ "station"?: string }` | Current AQHI per station, filtered when `station` is given |
| `pollutant_concentrations` | `{ "station"?: string, "hours"?: integer 1-24 }` | Latest reading plus the hourly series, per station |
| `pollution_summary` | `{ "include_weather"?: boolean }` | The CLI `summary` payload |
| `hong_kong_weather` | `{}` | The CLI `weather` payload |

Every result is `{"content":[{"type":"text","text":"<pretty JSON>"}],"isError":false}`.
An upstream failure returns `isError: true` with a readable message instead of
a transport error, so the session survives an outage.

```console
$ printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | hkpollution mcp
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"hongkong-pollution-skill","version":"0.1.0"}}}
{"jsonrpc":"2.0","id":2,"result":{"tools":[ ... ]}}
```

Unknown methods answer `-32601`, unparsable lines answer `-32700`, and the
server keeps serving until stdin closes.

## Data sources

| Source | Endpoint | Used for |
| --- | --- | --- |
| EPD AQHI dashboard (JSON) | `https://dashboard.data.gov.hk/api/aqhi-individual?format=json` | Primary current AQHI per station |
| EPD AQHI (RSS) | `https://www.aqhi.gov.hk/epd/ddata/html/out/aqhi_ind_rss_Eng.xml` | Fallback when the JSON host fails |
| EPD 24-hour pollutant concentrations (XML) | `https://www.aqhi.gov.hk/epd/ddata/html/out/24pc_Eng.xml` | Hourly NO2/O3/SO2/CO/PM10/PM2.5, µg/m³ |
| HKO current readings (JSON) | `https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en` | Temperature, humidity, warnings |

The 18 AQHI stations are Central/Western, Southern, Eastern, Kwun Tong, Sham
Shui Po, Kwai Chung, Tsuen Wan, Tseung Kwan O, Yuen Long, Tuen Mun, Tung
Chung, Tai Po, Sha Tin, North, Tap Mun, Causeway Bay, Central and Mong Kok —
the last three are roadside stations.

> **Attribution.** Data is published by the Government of the Hong Kong SAR:
> the [Environmental Protection Department](https://www.aqhi.gov.hk/) (AQHI,
> pollutant concentrations) and the [Hong Kong Observatory](https://www.hko.gov.hk/)
> (weather readings). Keep their attribution when you redistribute the data.

<details>
<summary><strong>JSON shapes</strong></summary>

`hkpollution aqhi --json`:

```json
{"stations":[{"station":"Central/Western","aqhi":4,"health_risk":"Moderate","publish_date":"2026-11-21T17:30:00"}],"source":"EPD","fetched_at":"2026-11-21T18:05:00+08:00"}
```

`hkpollution pollutants --json`:

```json
{"last_build_date":"Sat, 21 Nov 2026 17:30:00 +0800","hours":24,
 "stations":[{"station":"Central/Western",
   "latest":{"time":"2026-11-20T18:00:00+08:00","no2":47.8,"o3":45.6,"so2":1.0,"co":null,"pm10":14.7,"pm25":10.8},
   "series":[{"time":"2026-11-20T18:00:00+08:00","no2":47.8,"o3":45.6,"so2":1.0,"co":null,"pm10":14.7,"pm25":10.8}],
   "averages":{"no2":41.2,"pm25":9.4}}],"source":"EPD"}
```

`hkpollution weather --json`:

```json
{"update_time":"2026-11-21T18:02:00+08:00",
 "temperature":[{"place":"Hong Kong Observatory","value":20.0,"unit":"C"}],
 "humidity":[{"place":"Hong Kong Observatory","value":47,"unit":"percent"}],
 "warnings":["The Fire Danger Warning is Red and the fire risk is extreme."],
 "source":"HKO"}
```

`hkpollution summary --json` adds the rollup to the AQHI stations, plus
`worst`, `mean_aqhi`, `risk_counts`, `worst_pm25`, `worst_no2` and — only with
`--with-weather` — `weather`:

```json
{"fetched_at":"2026-11-21T18:33:13+08:00","stations":[...],
 "worst":{"station":"Central/Western","aqhi":4,"health_risk":"Moderate","publish_date":"2026-11-21T17:30:00"},
 "mean_aqhi":3.9,"risk_counts":{"Low":1,"Moderate":17},
 "worst_pm25":{"station":"Causeway Bay","value":19.6,"time":"2026-11-21T18:00:00+08:00"},
 "worst_no2":{"station":"Causeway Bay","value":102.6,"time":"2026-11-21T18:00:00+08:00"},
 "source":"EPD"}
```

Timestamps are ISO-8601 in Hong Kong time; concentrations are µg/m³.

</details>

## Development

A `justfile` holds the everyday commands (`brew install just` if you need it):

| Recipe | Does |
| --- | --- |
| `just` | `just check` — fmt check, clippy with `-D warnings`, tests |
| `just build-release` | Optimised binary at `target/release/hkpollution` |
| `just run aqhi --json` | Run the CLI, arguments passed through cargo |
| `just audit` | `cargo audit` against the lockfile |
| `just skill-check` | Validate `SKILL.md` and shell syntax the way CI does |
| `just version` | Print the release tag for the current `Cargo.toml` version |
| `just release 0.2.0` | Tag and push; the release workflow does the rest |

Layout:

| Path | Contents |
| --- | --- |
| `src/parse.rs` | Pure parsers for AQHI JSON/RSS, the 24-hour XML and HKO JSON |
| `src/model.rs` | Serialised payload types, station lookup, AQHI rollup |
| `src/http.rs` | `ureq` GET with 10 s connect / 20 s read timeouts and a User-Agent |
| `src/aqhi.rs` | AQHI fetch with RSS fallback, and the `summary` payload |
| `src/pollutants.rs` | Pollutant fetch, `--hours` windowing, averages, extremes |
| `src/weather.rs` | HKO fetch |
| `src/mcp.rs` | JSON-RPC 2.0 stdio server and the four tools |
| `src/main.rs` | CLI: argument parsing, tables, JSON output |
| `SKILL.md` | The agent-skill manifest: what the CLI reports and how to call it |
| `scripts/install.sh` | Release downloader with checksum verification and a cargo fallback |
| `scripts/ensure-binary.sh` | Resolves or builds a usable binary for the agent skill |

The test suite is offline: every parser test reads `tests/fixtures/*`, and the
MCP test drives the real binary over stdio. Decisions the spec left open are
recorded in [`NOTES.md`](NOTES.md).

## Releasing

```sh
just release 0.2.0
```

That creates and pushes the annotated tag `v0.2.0`;
[`.github/workflows/release.yml`](.github/workflows/release.yml) then checks the
tag against `Cargo.toml` and `CHANGELOG.md`, builds four targets, smoke-tests
the native ones, and publishes a GitHub release:

| Asset | Target |
| --- | --- |
| `hkpollution-v0.2.0-aarch64-apple-darwin.tar.gz` | macOS, Apple silicon |
| `hkpollution-v0.2.0-x86_64-apple-darwin.tar.gz` | macOS, Intel |
| `hkpollution-v0.2.0-x86_64-unknown-linux-musl.tar.gz` | Linux, x86_64 (static) |
| `hkpollution-v0.2.0-aarch64-unknown-linux-musl.tar.gz` | Linux, aarch64 (static) |
| `SHA256SUMS` | Checksums for every tarball |

`scripts/install.sh` reads the latest release, so re-running the one-liner
upgrades an installed binary and leaves an up-to-date one alone. Installed
skills update with `npx skills update`.

## License

MIT © [hongkong-ai-labs](https://github.com/hongkong-ai-labs). See [LICENSE](LICENSE).
