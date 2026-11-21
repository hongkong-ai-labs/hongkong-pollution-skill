---
name: hongkong-pollution-skill
description: Use when asked about Hong Kong air quality, AQHI, pollution levels, smog, beach water quality or HK weather — queries EPD and HKO open data via the hkpollution CLI or MCP server.
license: MIT
---

# Hong Kong pollution and weather

Official Hong Kong air quality and weather data in one binary, `hkpollution`.

## What it gives the agent

- Current AQHI for the 18 EPD monitoring stations, with health-risk band and
  publish time.
- Past-24-hour NO2, O3, SO2, CO, PM10 and PM2.5 concentrations per station,
  in µg/m³.
- A network summary: worst and mean AQHI, risk-level counts, worst PM2.5 and
  NO2 stations, and optionally HKO temperature, humidity and warnings.
- Tomorrow's AQHI range per period, split into general and roadside stations.
- Hourly AQHI and NO2/O3/SO2/CO/PM10/PM2.5 history per station, newest first.
- Street-level AQHI, added health risk (%AR) and pollutants at a Hong Kong
  latitude/longitude, current hour plus forecast.
- HKO current temperature and humidity readings plus active weather warnings.
- Latest EPD beach water-quality grading for every gazetted beach: district,
  grade (Good, Fair, Poor, Very Poor), open/closed status, WGS84 coordinates and
  the reason when a beach is not graded (winter, not open, closed, planned).
- Published E. coli monitoring for each beach: latest count per 100 mL, its
  sampling date, the five most recent samples, and the geometric mean of those
  five — the same figure EPD grades on.
- EPD monthly river and marine water quality (dissolved oxygen and BOD5 per
  station), with the sampling month named so nobody mistakes it for live data.
- AFCD red tide sightings back to 1975: active reports (no dissipation date)
  and recent ones, with WGS84 points and distance sorting.
- HKO hourly ambient gamma radiation per station in µSv/h, plus each station
  relative to the network median.
- HKO UV index with the WHO exposure band, also folded into `weather` and
  `brief`.
- AQHI statistics over 1-365 days (min, max, mean, hours above a threshold)
  from a local observation log topped up by the documented 24-hour feed, and a
  `compare` ranking that separates general from roadside stations.
- `digest`: one line per source for a cron job or morning check.

All of these are available as CLI commands and as MCP tools. The forecast and
history commands come from app APIs that the upstream mobile apps use; reports
label them `"source": "EPD app API"` and `"source": "PRAISE-HK"`.

## Before first use

Run `scripts/ensure-binary.sh` from the skill directory:

```console
$ bash scripts/ensure-binary.sh
/Users/you/.local/bin/hkpollution
```

It prints the absolute path of a working `hkpollution` binary and exits
non-zero only if it could not download or build one. Use the printed path for
every later command, for example `BIN="$(bash scripts/ensure-binary.sh)"`, and
do not assume `hkpollution` is on `PATH`.

It checks, in order: `$HKPOLLUTION_BIN`, `hkpollution` on `PATH`,
`scripts/install.sh`, then `cargo build --release --locked` in the repo root.

## Commands

| Command | Data |
| --- | --- |
| `hkpollution aqhi [--station NAME] [--json]` | Current AQHI per station |
| `hkpollution pollutants [--station NAME] [--hours 1-24] [--json]` | Hourly NO2/O3/SO2/CO/PM10/PM2.5 per station, with averages |
| `hkpollution forecast [--json]` | Tomorrow's AQHI range, general and roadside |
| `hkpollution history [--station NAME] [--hours 1-23] [--json]` | Hourly AQHI and pollutants per station, newest first |
| `hkpollution street --lat L --lng L [--hours 1-48] [--json]` | PRAISE-HK street-level AQHI, %AR and pollutants |
| `hkpollution summary [--with-weather] [--json]` | Network rollup, optional HKO weather |
| `hkpollution weather [--forecast] [--json]` | HKO temperature, humidity and warnings, plus the nine-day forecast |
| `hkpollution beach [--name TEXT] [--district TEXT] [--grade BAND] [--near LAT,LNG] [--history 0-10] [--json]` | EPD beach grading plus E. coli history |
| `hkpollution river [--zone TEXT] [--station TEXT] [--months 1-60] [--json]` | EPD monthly river water quality |
| `hkpollution marine [--zone TEXT] [--station TEXT] [--months 1-60] [--json]` | EPD monthly marine water quality |
| `hkpollution red-tide [--days 1-3650] [--active] [--near LAT,LNG] [--json]` | AFCD red tide sightings |
| `hkpollution radiation [--station TEXT] [--json]` | HKO ambient gamma dose rate |
| `hkpollution uv [--json]` | HKO UV index and band |
| `hkpollution stats [--days 1-365] [--station TEXT] [--threshold 1-11] [--json]` | AQHI statistics over time |
| `hkpollution compare [--json]` | Current AQHI ranked, general vs roadside |
| `hkpollution digest [--json]` | One line per source |
| `hkpollution recycling --lat L --lng L [--limit 1-50] [--type TEXT] [--json]` | Nearest recyclable collection points |
| `hkpollution watch [--interval S] [--aqhi N] [--red-tide] [--radiation] [--once] [--json]` | Alert loop across live sources |
| `hkpollution advice [--risk BAND] [--lang en\|tc\|both] [--json]` | Official EPD precautionary actions per AQHI band |
| `hkpollution brief --lat L --lng L [--json]` | Place name, street AQHI, nearest station, weather and advice in one call |
| `hkpollution map --bbox lat0,lng0,lat1,lng1 [--json]` | PRAISE-HK raster tile link for a bounding box |
| `hkpollution about [--json]` | The app's about pages |
| `hkpollution update [--check] [--force]` | Self-update using the detected install method |
| `hkpollution mcp` | Every dataset above as MCP tools over stdio |

`--station` is a case-insensitive substring match; an ambiguous name returns
every match. `--hours` keeps the newest N hourly readings per station
(default 1, maximum 24 for `pollutants` and 23 for `history`); `street` shows
24 of a possible 48 by default.

`street` needs `--lat` and `--lng` together, inside Hong Kong (latitude
22.1-22.6, longitude 113.8-114.5), and exits 2 otherwise.

`beach --grade` accepts `good`, `fair`, `poor`, `very-poor`, `graded`,
`ungraded` or `open`. `--name` matches beach or district text; `--district`
matches the district only, and `--near` sorts by distance and adds
`distance_km`. In winter EPD stops grading many beaches, so an empty
`--grade good` result does not mean the water is bad — check `grade_name` and
`status`.

Grade bands follow the EPD 
[Beach Grading System](https://www.epd.gov.hk/epd/english/environmentinhk/water/beach_quality/grading_system.html),
measured on the geometric mean E. coli count of the five most recent sampling
occasions: Good ≤ 24, Fair 25-180, Poor 181-610, Very Poor > 610 (or a last
reading above 1,600).

`beach --history N` adds `ecoli_history`: the geometric mean and maximum for
each of the N previous published years, so "is this beach cleaner than last
year" is answerable without another dataset. Each year costs one cached CSV
request (maximum 10).

`ecoli` is joined from EPD's yearly monitoring CSV, which is published only
after a year ends, so it can be months older than the grade. `ecoli_source`
names the file used and `ecoli_error` says why it is missing; a missing E. coli
block never fails the command. Quote `latest_sampled_on` whenever you cite a
count, and remember EPD advises against swimming for up to three days after
a tropical cyclone or heavy rain.

`river`/`marine` are **monthly monitoring**, not live readings: the table names
each station's newest sampling month, and `--months` keeps 1-60 of them per
station (default 6) with `mean_dissolved_oxygen_mg_l` and `mean_bod5_mg_l` over
that window. Red tide reports are historical: an `active` sighting is one with
no dissipation date yet, and `--days` bounds the recent window (default 90).
Radiation `level: elevated` means more than twice the network median — a data
signal, not a health threshold. `stats` keeps a local log in the cache
directory (90 days, one line per station-hour) that grows with every call, so a
fresh install starts with the documented 24-hour feed and improves from there.

One station:

```console
$ hkpollution aqhi --station "mong kok"
Station   AQHI  Risk      Published
--------  ----  --------  ----------------
Mong Kok  4     Moderate  2026-11-21 17:30

source: EPD Air Quality Health Index (fetched 2026-11-21T18:33:13+08:00)
```

JSON piped to `jq`:

```console
$ hkpollution aqhi --station central --json | jq -r '.stations[].station'
Central/Western
Central
```

Summary with weather:

```console
$ hkpollution summary --with-weather | tail -9

network: worst 4 (Central/Western) / mean 3.9
risk levels: Low 1, Moderate 17
worst PM2.5: Causeway Bay 19.6 µg/m³ (2026-11-21 18:00)
worst NO2: Causeway Bay 102.6 µg/m³ (2026-11-21 18:00)
weather: 2026-11-21T18:02:00+08:00 temperature 20.0 °C, humidity 47.0 %, warnings: The Fire Danger Warning is Red and the fire risk is extreme.

source: EPD / HKO
```

## JSON contract

`--json` prints exactly one JSON object on stdout, with no footer, so it is
safe to pipe to `jq`.

- Timestamps are ISO-8601 in Hong Kong time, with a `+08:00` offset.
- Concentrations are in µg/m³.
- A value is `null` when a station does not measure that pollutant; a missing
  average key means every reading in the window was null.

## MCP server

`hkpollution mcp` speaks JSON-RPC 2.0 over stdio (one JSON object per line,
protocol revision `2024-11-05`) and exposes these tools:

| Tool | Arguments |
| --- | --- |
| `air_quality_aqhi` | `{ "station"?: string }` |
| `pollutant_concentrations` | `{ "station"?: string, "hours"?: 1-24 }` |
| `pollution_summary` | `{ "include_weather"?: boolean }` |
| `hong_kong_weather` | `{}` |
| `beach_water_quality` | `{ "name"?: string, "district"?: string, "grade"?: string, "lat"?: number, "lng"?: number }` |
| `water_quality` | `{ "kind"?: "river"\|"marine", "zone"?: string, "station"?: string, "months"?: 1-60 }` |
| `red_tide` | `{ "days"?: 1-3650, "active"?: boolean, "lat"?: number, "lng"?: number }` |
| `radiation_level` | `{ "station"?: string }` |
| `uv_index` | `{}` |
| `air_quality_stats` | `{ "days"?: 1-365, "station"?: string, "threshold"?: 1-11 }` |
| `station_comparison` | `{}` |
| `environment_digest` | `{}` |
| `recycling` | `{ "lat": number, "lng": number, "limit"?: 1-50, "type"?: string }` |
| `environment_alerts` | `{ "aqhi"?: 1-11, "red_tide"?: boolean, "radiation"?: boolean }` |
| `air_quality_briefing` | `{ "lat": number, "lng": number, "no_cache"?: boolean }` |
| `health_advice` | `{ "risk"?: string, "lang"?: "en"\|"tc"\|"both" }` |
| `source_health` | `{}` |
| `aqhi_forecast` | `{}` |
| `station_history` | `{ "station"?: string, "hours"?: 1-23 }` |
| `street_level_aqhi` | `{ "lat": number, "lng": number, "hours"?: 1-48 }` |

Client configuration, with `command` set to the path printed by
`scripts/ensure-binary.sh`:

```json
{
  "mcpServers": {
    "hongkong-pollution": {
      "command": "/absolute/path/printed/by/ensure-binary.sh",
      "args": ["mcp"]
    }
  }
}
```

An upstream failure comes back as a tool result with `isError: true` and a
readable message rather than a transport error, so the session survives an
outage.

## Exit codes

| Exit | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Network, upstream or I/O failure, including a `--station` that matches nothing |
| `2` | Usage error (unknown command or flag, bad `--hours`) |

Errors always go to stderr; stdout stays clean for `--json`.

## Answering patterns

- "Is the air bad where I am / can I run outside?" → `brief --lat <x> --lng <y>`
  (one call: street AQHI, nearest station, weather, advice). Follow with
  `advice --risk <band>` when the user wants the full precautionary table.
- "Which station is worst right now?" → `summary --json`, read `worst` and `trend`.
- "Where should I walk?" → `street --lat <x> --lng <y> --hours 3` for nearby hours.
- "Will it be better tomorrow?" → `forecast` (AQHI bands) or `weather --forecast`.
- "Is it safe to swim at X beach?" → `beach --name X`; quote the EPD grade,
  status, latest E. coli count with its sampling date, and do not call an
  ungraded winter beach dirty or clean. "Which beach is nearest / cleanest
  nearby?" → `beach --near <lat>,<lng>`; filter with `--grade good`.
- "What does 'High' mean for me?" → `advice --risk high`; the answer is the official
  EPD wording, not a paraphrase.
- "Is there a red tide near this beach?" → `red-tide --near <lat>,<lng> --active`;
  a sighting without a dissipation date is still active.
- "Should I worry about radiation / UV?" → `radiation` and `uv`; report the raw
  values and bands, and note that radiation is compared with the network
  median rather than a health limit.
- "Has the air been bad this month?" → `stats --days 30 --station X`; the local
  log starts at first use, so say how much coverage exists.
- "Where do I recycle glass near here?" → `recycling --lat <x> --lng <y>
  --type glass`; quote the distance, accepted types and opening hours.
- "Tell me when the air gets bad" → `watch --aqhi 7 --once` exits 3 on a breach
  for cron; omit `--once` to poll every `--interval` seconds. Add `--red-tide`
  and `--radiation` to include those sources.
- Monitoring: `aqhi --fail-above 7` exits 3, so a cron job can act on it directly.
- Something looks broken → `doctor`, which probes every source and reports latency.

The cache is on by default (5 min for current readings, 15 min for forecasts,
30 min for station history, 24 h for the static advice pages). Pass `--no-cache` when the user asks for "right now"
after an earlier call in the same minute, and `--max-age 0` to force a refetch.

## Attribution

Data © Government of the Hong Kong Special Administrative Region: the
Environmental Protection Department (AQHI, pollutant concentrations, forecast,
station history, beach water-quality grading, river and marine water quality),
the Hong Kong Observatory (weather, UV index and ambient gamma radiation) and
the Agriculture, Fisheries and Conservation Department (red tide sightings).
Street-level data is the PRAISE-HK model © HKUST Institute for the
Environment. Keep this attribution when you redistribute the data.

## Source and offline builds

The Rust source ships with the skill, so `scripts/ensure-binary.sh` can run
`cargo build --release --locked` when no prebuilt binary is reachable.
`scripts/install.sh` follows the same rule, and honors
`HKPOLLUTION_REPO`, `HKPOLLUTION_VERSION` and `HKPOLLUTION_BIN_DIR`.

MIT licensed.

### Install the binary directly

`npx skills add` installs the skill files. If the binary is missing, install it
with the release installer or cargo:

```bash
curl -fsSL https://raw.githubusercontent.com/hongkong-ai-labs/hongkong-pollution-skill/main/scripts/install.sh | sh
# or
cargo install --git https://github.com/hongkong-ai-labs/hongkong-pollution-skill --locked
```

Register it with any MCP client:

```bash
npx add-mcp "$(command -v hkpollution) mcp" --name hongkong-pollution -a claude-code -a codex -a cursor -y
claude mcp add hongkong-pollution -- hkpollution mcp
codex mcp add hongkong-pollution -- hkpollution mcp
```

`add-mcp` detects the client configuration files in the current project; add
`-g` for a global install. `$(command -v hkpollution)` keeps the entry correct
when the binary is on `PATH`; substitute an absolute path if your client does
not inherit the shell `PATH`.
