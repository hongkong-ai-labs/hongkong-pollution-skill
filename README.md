<div align="center">

<img src="assets/logo.svg" width="84" height="84" alt="hkpollution">

# hkpollution

**Hong Kong air quality in one binary.**

Current AQHI, pollutant concentrations, forecasts, street-level model data and
the official health advice — from official open data, as a CLI for humans and an
MCP server for agents.

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

**From source or crates.io** — Rust 1.85 or newer, no C toolchain needed:

```sh
cargo install hkpollution --locked
```

**Homebrew** (once the tap is published):

```sh
brew tap hongkong-ai-labs/tap && brew install hkpollution
```

**Container** — a small multi-arch image, or build it locally:

```sh
docker run --rm ghcr.io/hongkong-ai-labs/hkpollution aqhi --json
docker build -t hkpollution .
docker run --rm hkpollution aqhi --json
```

Apple's `container` runtime uses the same OCI image:

```sh
container build -t hkpollution .
container run --rm hkpollution aqhi --json
container run --rm -i hkpollution mcp
```

Whichever route you take:

```console
$ hkpollution --version
hkpollution 0.3.0
```

## Self-update

`hkpollution update --check` reports the current version, latest release and
the detected install method. `hkpollution update` updates with that same
method: cargo install, the release installer, a source rebuild or Homebrew.

## What you get

| Command | Data | Output |
| --- | --- | --- |
| `hkpollution aqhi` | Current Air Quality Health Index for all 18 EPD stations, with risk band and publish time | table or `--json` |
| `hkpollution pollutants` | Past 24 h NO2, O3, SO2, CO, PM10, PM2.5 per station, with per-pollutant averages | table or `--json` |
| `hkpollution forecast` | Tomorrow's AQHI range per period, split into general and roadside stations, from the EPD app API | table or `--json` |
| `hkpollution history` | Hourly AQHI and NO2/O3/SO2/CO/PM10/PM2.5 per station, newest first, from the EPD app API | table or `--json` |
| `hkpollution street` | PRAISE-HK street-level AQHI, added health risk and pollutants for one coordinate | table or `--json` |
| `hkpollution summary` | Network worst/mean AQHI, risk counts, worst PM2.5 and NO2 station, 4-hour trend, optional HKO weather | table or `--json` |
| `hkpollution weather` | HKO temperature, humidity and warnings, optionally the nine-day forecast | table or `--json` |
| `hkpollution advice` | Official EPD precautionary actions per AQHI risk band, English and/or Chinese | table or `--json` |
| `hkpollution brief` | One call for a coordinate: place, street AQHI, nearest station, weather and advice | report or `--json` |
| `hkpollution beach` | EPD beach grading (grade, status, district, WGS84 point) joined to published E. coli monitoring: latest count, five recent samples, geometric mean; `--near` sorts by distance | table or `--json` |
| `hkpollution river` | EPD monthly river water quality: dissolved oxygen and BOD5 per station, newest sampling month | table or `--json` |
| `hkpollution marine` | EPD monthly marine water quality, same shape without the river name | table or `--json` |
| `hkpollution red-tide` | AFCD red tide sightings, active and recent, with WGS84 points and distance sorting | table or `--json` |
| `hkpollution radiation` | HKO hourly ambient gamma dose rate per station, relative to the network median | table or `--json` |
| `hkpollution uv` | HKO UV index with the WHO exposure band | report or `--json` |
| `hkpollution stats` | AQHI min/max/mean and exceedance hours per station over 1-365 days | table or `--json` |
| `hkpollution compare` | Current AQHI ranked, with general and roadside means kept apart | table or `--json` |
| `hkpollution digest` | One line per source for a cron job or morning check | report or `--json` |
| `hkpollution map` | PRAISE-HK raster tile link for a bounding box | report or `--json` |
| `hkpollution about` | The app's about pages, privacy policy and long-term air quality links | report or `--json` |
| `hkpollution mcp` | All of the above as MCP tools over stdio | JSON-RPC |

The four documented datasets read official endpoints only. `forecast`,
`history` and `street` use the app APIs behind the EPD and PRAISE-HK mobile
apps — undocumented and keyless, but still **no API keys and no scraping.**
The dependency list is `serde`, `serde_json` and `ureq`; the XML feeds are
parsed by a small hand-written tag scanner, and the release binaries are
static and stripped.

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
| `3` | `--fail-above` threshold reached — a finding, not a failure |

Errors always go to stderr, so `--json` output stays pipeable.

```console
$ hkpollution aqhi --station kownloon
hkpollution: no station matches "kownloon"; valid stations: Central/Western, Southern, Eastern, Kwun Tong, Sham Shui Po, Kwai Chung, Tsuen Wan, Tseung Kwan O, Yuen Long, Tuen Mun, Tung Chung, Tai Po, Sha Tin, North, Tap Mun, Causeway Bay, Central, Mong Kok
$ echo $?
1
```

## Forecast and street-level data

Two more datasets come from the app APIs the mobile apps use rather than the
open-data portal. Both are undocumented, so they are additive: the documented
feeds stay the primary source for everything above, and a failure exits `1`
like any other upstream error.

### Forecast

`hkpollution forecast` shows the AQHI range the EPD forecasts for each
period, for general and roadside stations separately:

```console
$ hkpollution forecast
Period         General  Roadside
-------------  -------  --------
Tomorrow A.M.  0-1      0-1
Tomorrow P.M.  0-1      0-1

source: EPD app API (fetched 2026-11-21T19:43:42+08:00) — EPD data © Government of the HKSAR
```

### Station history

`hkpollution history` is the app-API counterpart of `pollutants`: one row per
station by default, or the newest N hourly readings with `--station` and
`--hours` (1-23), with the AQHI alongside the pollutants and the same `avg N`
row as `pollutants`:

```console
$ hkpollution history --station "mong kok" --hours 3
Station   Time              AQHI  NO2   O3    SO2  CO     PM10  PM2.5
--------  ----------------  ----  ----  ----  ---  -----  ----  -----
Mong Kok  2026-11-21 19:00  4     92.5  34.3  6.3  469.7  21.6  12.4
Mong Kok  2026-11-21 18:00  4     97.0  34.2  5.7  475.8  22.2  11.8
Mong Kok  2026-11-21 17:00  4     94.6  36.1  5.9  511.9  26.7  15.2
Mong Kok  avg 3             4.0   94.7  34.9  6.0  485.8  23.5  13.1

source: EPD app API (fetched 2026-11-21T19:43:42+08:00) — EPD data © Government of the HKSAR
```

### Street level

`hkpollution street --lat 22.33751 --lng 114.263` asks the PRAISE-HK model
for one Hong Kong coordinate. The default `--hours 24` covers the current
hour and the forecast, up to 48:

```console
$ hkpollution street --lat 22.33751 --lng 114.263 --hours 3
Time              AQHI  %AR  NO2   O3     PM10  PM2.5  SO2
----------------  ----  ---  ----  -----  ----  -----  ---
2026-11-21 19:00  4     6.3  12.8  103.9  16.1  10.1   4.7
2026-11-21 20:00  4     6.3  7.2   113.1  18.3  11.5   4.8
2026-11-21 21:00  4     6.6  3.4   120.6  20.4  13.2   4.8

source: PRAISE-HK street-level model © HKUST Institute for the Environment (fetched 2026-11-21T19:44:04+08:00)
```

`--lat` and `--lng` are required together and must lie inside Hong Kong
(22.1-22.6 and 113.8-114.5); anything else is a usage error, exit `2`. `%AR`
is the added health risk, and `-` is a value the model does not publish for
that hour.

### One-call briefing

`brief` answers the question people actually ask — "what is the air like where I am?"
— with one request: the place name, the street-level model, the nearest official
station and its distance, the weather, and the advice that applies.

```console
$ hkpollution brief --lat 22.3375 --lng 114.263
place:    THE HONG KONG UNIVERSITY OF SCIENCE AND TECHNOLOGY (22.3375, 114.2630)
street:   AQHI 4 · added risk 6.1% · 2026-11-21 20:00
station:  Tseung Kwan O (2.2 km away) AQHI 4 · Moderate · 2026-11-21 19:30
weather:  20.0 C · 45 % humidity · The Fire Danger Warning is Red and the fire risk is extreme.

advice for Moderate (AQHI 4-6):
  general public         No response action is required.
  heart and respiratory  No response action is normally required. Individuals who are experiencing symptoms are advised to consider reducing outdoor physical exertion.

source: EPD / PRAISE-HK / HKO (fetched 2026-11-21T20:53:52+08:00)
```

Each part is fetched independently: if one upstream is down the report still comes
back with `partial: true` and a `<part>_error` field, and the command only fails when
both the street model and the station failed.

### Health advice

`advice` prints EPD's own precautionary actions. The English table is compiled in from
the AQHI website, so it works even when the app API is unreachable; `--lang tc` or
`--lang both` adds the upstream Traditional Chinese text.

```console
$ hkpollution advice --risk high
High (AQHI 7)
  heart and respiratory  People with existing heart or respiratory illnesses are advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic. They should also seek advice from a medical doctor before participating in sport activities and take more breaks during physical activities.
  children and elderly   Children and the elderly are advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic.
  outdoor workers        No response action is required.
  general public         No response action is required.

source: EPD health advice (fetched 2026-11-21T20:53:48+08:00)
```

### Beach water quality

`beach` joins EPD's weekly grading feed to the department's yearly E. coli
monitoring file. Each beach carries its grade and status, a WGS84 point, and
the most recent published E. coli samples:

```console
$ hkpollution beach --near 22.23,114.17 --grade graded | head -8
DISTRICT            BEACH                 GRADE           STATUS  DIST     E. COLI  SAMPLED
------------------  --------------------  --------------  ------  -------  -------  ----------
Southern District   Deep Water Bay Beach   Grade 1 - Good  open    2.4 km   2        2025-12-29
Southern District   Repulse Bay Beach     Grade 1 - Good  open    2.8 km   1        2025-12-02
Southern District   Middle Bay Beach      Grade 1 - Good  open    2.9 km   3        2025-12-02
```

Grades follow the [EPD Beach Grading System](https://www.epd.gov.hk/epd/english/environmentinhk/water/beach_quality/grading_system.html):
Good ≤ 24, Fair 25-180, Poor 181-610, Very Poor > 610 E. coli per 100 mL, on
the geometric mean of the five most recent sampling occasions (a last reading
above 1,600 also means Very Poor). The grading feed and the E. coli file move
at different speeds — EPD publishes a year's monitoring file only after the
year ends — so the report names the file it used in `ecoli_source` and an
unreadable file degrades to `ecoli: null` with an `ecoli_error` instead of
failing. Quote `latest_sampled_on` whenever you cite a count, and note EPD's
advice against swimming for up to three days after a tropical cyclone or
heavy rain.

### Beyond air

Four more datasets answer questions air quality cannot:

```console
$ hkpollution red-tide --days 365 --near 22.23,114.17
REPORTED    LOCATION                              SPECIES               STATUS  DISSIPATED  DIST
----------  ------------------------------------  --------------------  ------  ----------  ----
2026-08-18  Repulse Bay Beach, Hong Kong Island   Phaeocystis globosa           2026-08-19  2.9 km

$ hkpollution radiation
STATION         µSv/H  VS MEDIAN  LEVEL    TIME
--------------  -----  ---------  -------  -------------------------
King's Park     0.15   1.36x      typical  2026-11-22T01:00:00+08:00

$ hkpollution river --zone "Deep Bay" --months 2
ZONE      RIVER             STATION  MONTH    DO mg/L  BOD5 mg/L
--------  ----------------  -------  -------  -------  ---------
Deep Bay  River Ganges      GR1      2026-10  6.0      19.0
```

`river` and `marine` are **monthly monitoring**, not live: every row names its
sampling month and the report names the newest one it holds. An `active` red
tide is a sighting with no dissipation date yet. Radiation `elevated` means
more than twice the network median, which is a data signal rather than a
health threshold.

`digest` runs the lot in one call and never fails: each source is a line, and a
broken upstream becomes `unavailable: <reason>`.

```console
$ hkpollution digest
air        worst AQHI 4 at Central/Western (Moderate), network mean 3.5
beach      11 graded, 11 open, 32 not graded, E. coli 2025
red tide   no active sightings; 1 reported in the last 90 days
radiation  11 stations, median 0.11 µSv/h, max 0.15 µSv/h at King's Park
river      13 stations, newest sampling month 2026-10 (EPD monthly)
uv         index 0.0 (low), observed 2026-11-21T18:00:00+08:00
weather    no active warnings
```

### Statistics and comparison

`stats` keeps a local observation log (one line per station-hour, 90 days) in
the cache directory, topped up by the documented 24-hour AQHI feed, so a fresh
install is already useful and coverage grows with every call. `compare` keeps
general and roadside stations apart, because ranking a roadside kerb against a
background station is not a fair comparison.

```console
$ hkpollution compare | head -5
STATION          CATEGORY  AQHI  RISK      VS CATEGORY
---------------  --------  ----  --------  -----------
Causeway Bay     roadside  4     Moderate  +0.0
Central          roadside  4     Moderate  +0.0
Central/Western  general   4     Moderate  +0.6
```

## Reliability

Built for agents that call the same tool in a loop, and for cron jobs that need a
verdict rather than prose.

| Concern | Behaviour |
| --- | --- |
| Repeat calls | On-disk TTL cache in `~/.cache/hkpollution`: 5 min for current readings, 15 min for forecasts, 30 min for station history, 24 h for static pages. `--no-cache`, `--max-age <secs>`, `--refresh` |
| Expired entries | Revalidated with `If-Modified-Since`; a `304` restarts the window without a rewrite, and a network failure serves the stale body instead of failing |
| Flaky upstreams | Up to 3 attempts with 300 ms / 900 ms backoff for transport errors, 5xx and 429, honouring `Retry-After` |
| Thresholds | `--fail-above 7` exits `3` when the worst AQHI reaches it, so `hkpollution aqhi --fail-above 7` is a monitoring check with no parsing |
| Needing a station name | `--near 22.3375,114.263` resolves to the nearest of the 18 stations and prints the distance |
| Drift and outages | `hkpollution doctor` probes all fifteen sources, reports latency, a schema check and the age of the newest data in each payload (flagging `stale`), and prints the cache location and size |
| Tuning | `HKPOLLUTION_CACHE_DIR`, `HKPOLLUTION_TIMEOUT_SECS`, `HKPOLLUTION_RETRIES` |

```console
$ hkpollution doctor
SOURCE                  STATUS  MS   DETAIL
----------------------  ------  ---  -----------
data.gov.hk AQHI        ok      103  1701 bytes
aqhi.gov.hk RSS         ok      40   7222 bytes
aqhi.gov.hk 24pc        ok      55   98211 bytes
HKO rhrread             ok      31   2951 bytes
HKO fnd                 ok      33   4364 bytes
EPD app forecast        ok      63   699 bytes
EPD app monitors        ok      29   3960 bytes
PRAISE weather          ok      74   47 bytes, data 0.0 h old
PRAISE pollutant_index  ok      607  13111 bytes, data 0.0 h old
EPD beach grading       ok      530  11643 bytes
EPD beach E. coli       ok      335  54863 bytes
EPD river water quality ok      262  19644 bytes
EPD marine water quality ok     54   10798 bytes
EPD 24-hour AQHI        ok      47   62712 bytes, data 0.3 h old
HKO UV index            ok      34   56 bytes, data 0.3 h old
HKO radiation           ok      35   413 bytes, data 0.8 h old
AFCD red tide           ok      1216 266998 bytes, data 0.0 h old

summary: 15 ok, 0 stale, 0 failed
```

## MCP server

`hkpollution mcp` speaks JSON-RPC 2.0 over stdio — one JSON object per line,
protocol revision `2025-06-18`, negotiated down to `2024-11-05` when a client
asks for it. Every tool is annotated read-only and idempotent, declares an output
schema, and returns `structuredContent` alongside the text payload, so clients can
cache and auto-approve calls. Point any stdio-capable MCP client at the binary:

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
| `air_quality_briefing` | `{ "lat": number, "lng": number, "no_cache"?: boolean }` | The CLI `brief` payload |
| `health_advice` | `{ "risk"?: string, "lang"?: "en"\|"tc"\|"both" }` | The CLI `advice` payload |
| `source_health` | `{}` | The CLI `doctor` payload |
| `aqhi_forecast` | `{}` | The CLI `forecast` payload |
| `station_history` | `{ "station"?: string, "hours"?: integer 1-23 }` | Hourly AQHI and pollutant history per station, newest first |
| `street_level_aqhi` | `{ "lat": number, "lng": number, "hours"?: integer 1-48 }` | The CLI `street` payload |

Every result is `{"content":[{"type":"text","text":"<pretty JSON>"}],"isError":false}`.
An upstream failure returns `isError: true` with a readable message instead of
a transport error, so the session survives an outage.

This is a **stdio-only** server: it listens on no port and has no network
authentication layer, so there is no API-key setting. Optional keys in this
project family apply only to HTTP MCP transports. For a container, keep stdin
attached (`docker run --rm -i hkpollution mcp`) and let the MCP client launch
and stop the process. There is no Worker/WASM deployment; the binary uses
`ureq` plus an on-disk cache and is packaged as a CLI/stdio server.

```console
$ printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | hkpollution mcp
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"hongkong-pollution-skill","version":"0.3.0"}}}
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
| EPD app API (JSON, undocumented) | `https://app.epd-asmg.gov.hk/api/v2/forecast`, `https://app.epd-asmg.gov.hk/api/v2/monitors` | Tomorrow's AQHI range, hourly per-station history |
| HKO nine-day forecast (JSON) | `https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=fnd&lang=en` | `weather --forecast` |
| PRAISE-HK model API (JSON, undocumented) | `https://praise-web.ust.hk:8443/api/pollutant_index` | Street-level AQHI, added health risk and pollutants at a coordinate |
| PRAISE-HK reverse geocoding | `https://praise-web.ust.hk:8443/location/reverse_geocoding` | Place names in `brief` |
| PRAISE-HK spatial tiles | `https://praise-web.ust.hk:8443/api/pollutant_spatial_tiles` | Raster tile links in `map` |
| EPD beach grading (GeoJSON) | `https://cd.epic.epd.gov.hk/EPICDI/json/beach/beachgrading?lang=en` | Grade, status, district and a WGS84 point per gazetted beach |
| EPD beach E. coli monitoring (CSV, yearly) | `https://cd.epic.epd.gov.hk/beachpsi/en/beach-historical-data-<year>-en.csv` | E. coli count per 100 mL per sampling date, from the latest complete year |
| EPD recent river water quality (CSV) | `https://cd.epic.epd.gov.hk/riverpsi/en/riverrecent/river-recent-en.csv` | Monthly dissolved oxygen and BOD5 per river station |
| EPD recent marine water quality (CSV) | `https://cd.epic.epd.gov.hk/marinepsi/en/marinerecent/marine-recent-en.csv` | Monthly dissolved oxygen and BOD5 per marine station |
| EPD past 24-hour AQHI (XML, documented) | `https://www.aqhi.gov.hk/epd/ddata/html/out/24aqhi_Eng.xml` | `history` fallback and the first run of `stats` |
| AFCD red tide sightings (CSV) | `https://redtide.afcd.gov.hk/data/RTMS_ob_RTLE.csv` | Active and recent red tides with coordinates |
| HKO ambient gamma radiation (CSV) | `https://data.weather.gov.hk/weatherAPI/hko_data/regional-weather/latest_hourly_rmn.csv` | Hourly mean dose rate per station, µSv/h |
| HKO UV index (CSV) | `https://data.weather.gov.hk/weatherAPI/hko_data/regional-weather/latest_15min_uvindex.csv` | 15-minute mean UV index |

A `source` of `EPD app API` or `PRAISE-HK` marks a report from one of those
undocumented app APIs; `EPD` and `HKO` keep their existing meaning.

The 18 AQHI stations are Central/Western, Southern, Eastern, Kwun Tong, Sham
Shui Po, Kwai Chung, Tsuen Wan, Tseung Kwan O, Yuen Long, Tuen Mun, Tung
Chung, Tai Po, Sha Tin, North, Tap Mun, Causeway Bay, Central and Mong Kok —
the last three are roadside stations.

> **Attribution.** Data is published by the Government of the Hong Kong SAR:
> the [Environmental Protection Department](https://www.aqhi.gov.hk/) (AQHI,
> pollutant concentrations, forecast and station history) and the
> [Hong Kong Observatory](https://www.hko.gov.hk/) (weather readings). The
> street-level model is the PRAISE-HK project of the HKUST Institute for the
> Environment. Keep their attribution when you redistribute the data.

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

`hkpollution forecast --json`:

```json
{"periods":[{"period":"Tomorrow A.M.","general":{"min":0,"max":1},"roadside":{"min":0,"max":1}},{"period":"Tomorrow P.M.","general":{"min":0,"max":1},"roadside":{"min":0,"max":1}}],"source":"EPD app API","fetched_at":"2026-11-21T19:43:42+08:00"}
```

`hkpollution history --json` — newest first, and `series` always carries the
full upstream window (23 hours) while `hours` trims only the table:

```json
{"hours":2,"stations":[{"station":"Mong Kok","station_id":81,"category":"roadside","latitude":22.322584,"longitude":114.168297,
 "latest":{"time":"2026-11-21T19:00:00+08:00","aqhi":4,"no2":92.5,"o3":34.3,"so2":6.3,"co":469.7,"pm10":21.6,"pm25":12.4},
 "series":[{"time":"2026-11-21T19:00:00+08:00","aqhi":4,"no2":92.5,"o3":34.3,"so2":6.3,"co":469.7,"pm10":21.6,"pm25":12.4},
           {"time":"2026-11-21T18:00:00+08:00","aqhi":4,"no2":97.0,"o3":34.2,"so2":5.7,"co":475.8,"pm10":22.2,"pm25":11.8}, ...]}],
 "source":"EPD app API","fetched_at":"2026-11-21T19:44:05+08:00"}
```

`hkpollution street --json`:

```json
{"location":{"lat":22.33751,"lng":114.263},
 "current":{"time":"2026-11-21T19:00+08:00","aqhi":4,"added_health_risk":6.3,"no2":12.8,"o3":103.9,"pm10":16.1,"pm25":10.1,"so2":4.7},
 "series":[{"time":"2026-11-21T19:00+08:00",...},{"time":"2026-11-21T20:00+08:00",...}],
 "source":"PRAISE-HK","fetched_at":"2026-11-21T19:44:05+08:00"}
```

`hkpollution beach --json`:

```json
{"source":"https://cd.epic.epd.gov.hk/EPICDI/json/beach/beachgrading?lang=en",
 "count":1,"graded":1,"open":1,
 "ecoli_source":"https://cd.epic.epd.gov.hk/beachpsi/en/beach-historical-data-2025-en.csv",
 "ecoli_error":null,
 "beaches":[{"name":"Silver Mine Bay Beach","district":"Islands District","grade":1,
   "grade_name":"Grade 1 - Good","status":"open","description":"Grade 1 - Good",
   "code":"sil","latitude":22.270278,"longitude":113.998889,"distance_km":null,
   "ecoli":{"latest_sampled_on":"2025-12-04","latest_per_100ml":140,
     "geometric_mean_per_100ml":29.6,"samples":160,
     "recent":[{"sampled_on":"2025-12-04","per_100ml":140},{"sampled_on":"2025-11-24","per_100ml":14}, ...]}}]}
```

Timestamps are ISO-8601 in Hong Kong time; concentrations are µg/m³ and beach
E. coli counts are per 100 mL.

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
| `just release 0.4.0` | Tag and push; the release workflow does the rest |

Layout:

| Path | Contents |
| --- | --- |
| `src/parse.rs` | Pure parsers for AQHI JSON/RSS, the 24-hour XML, HKO JSON and the app APIs |
| `src/model.rs` | Serialised payload types, station lookup, AQHI rollup |
| `src/http.rs` | `ureq` GET with 10 s connect / 20 s read timeouts and a User-Agent |
| `src/aqhi.rs` | AQHI fetch with RSS fallback, and the `summary` payload |
| `src/pollutants.rs` | Pollutant fetch, `--hours` windowing, averages, extremes |
| `src/weather.rs` | HKO fetch |
| `src/epd_app.rs` | EPD app API forecast and per-station history |
| `src/praise.rs` | PRAISE-HK street-level fetch and Hong Kong bounds validation |
| `src/mcp.rs` | JSON-RPC 2.0 stdio server and the MCP tools |
| `src/main.rs` | CLI: argument parsing, tables, JSON output |
| `SKILL.md` | The agent-skill manifest: what the CLI reports and how to call it |
| `scripts/install.sh` | Release downloader with checksum verification and a cargo fallback |
| `scripts/ensure-binary.sh` | Resolves or builds a usable binary for the agent skill |

The test suite is offline: every parser test reads `tests/fixtures/*`, and the
MCP test drives the real binary over stdio. Decisions the spec left open are
recorded in [`NOTES.md`](NOTES.md).

## Releasing

```sh
just release 0.4.0
```

That creates and pushes the annotated tag `v0.4.0`;
[`.github/workflows/release.yml`](.github/workflows/release.yml) then checks the
tag against `Cargo.toml` and `CHANGELOG.md`, builds four targets, smoke-tests
the native ones, and publishes a GitHub release:

| Asset | Target |
| --- | --- |
| `hkpollution-v0.4.0-aarch64-apple-darwin.tar.gz` | macOS, Apple silicon |
| `hkpollution-v0.4.0-x86_64-apple-darwin.tar.gz` | macOS, Intel |
| `hkpollution-v0.4.0-x86_64-unknown-linux-musl.tar.gz` | Linux, x86_64 (static) |
| `hkpollution-v0.4.0-aarch64-unknown-linux-musl.tar.gz` | Linux, aarch64 (static) |
| `SHA256SUMS` | Checksums for every tarball |

The same run also publishes `ghcr.io/hongkong-ai-labs/hkpollution:<tag>` and
`:latest` (multi-arch, from the static musl builds) and, when a `BREW_TOKEN`
secret with repo scope exists, regenerates `Formula/hkpollution.rb` in
`hongkong-ai-labs/homebrew-tap` from the two macOS sha256s.

`scripts/install.sh` reads the latest release, so re-running the one-liner
upgrades an installed binary and leaves an up-to-date one alone. Installed
skills update with `npx skills update`.

### Drift detection

Both app APIs are undocumented, so a nightly job runs every command against the
live sources, compares the top-level payload keys with `tests/schema/*.json`,
runs `doctor`, and opens (or updates) a single `drift` issue when something moved.
Locally the same check is `./scripts/check-drift.sh`, with `--update` to re-record
the snapshots after an intentional change.

## License

MIT © [hongkong-ai-labs](https://github.com/hongkong-ai-labs). See [LICENSE](LICENSE).
