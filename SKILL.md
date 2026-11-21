---
name: hongkong-pollution-skill
description: Use when asked about Hong Kong air quality, AQHI, pollution levels, smog or HK weather — queries EPD and HKO open data via the hkpollution CLI or MCP server.
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

All ten are available as CLI commands and as MCP tools. The last three come
from app APIs that the upstream mobile apps use; reports label them
`"source": "EPD app API"` and `"source": "PRAISE-HK"`.

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
| `hkpollution advice [--risk BAND] [--lang en\|tc\|both] [--json]` | Official EPD precautionary actions per AQHI band |
| `hkpollution brief --lat L --lng L [--json]` | Place name, street AQHI, nearest station, weather and advice in one call |
| `hkpollution map --bbox lat0,lng0,lat1,lng1 [--json]` | PRAISE-HK raster tile link for a bounding box |
| `hkpollution about [--json]` | The app's about pages |
| `hkpollution mcp` | Every dataset above as MCP tools over stdio |

`--station` is a case-insensitive substring match; an ambiguous name returns
every match. `--hours` keeps the newest N hourly readings per station
(default 1, maximum 24 for `pollutants` and 23 for `history`); `street` shows
24 of a possible 48 by default.

`street` needs `--lat` and `--lng` together, inside Hong Kong (latitude
22.1-22.6, longitude 113.8-114.5), and exits 2 otherwise.

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
protocol revision `2024-11-05`) and exposes seven tools:

| Tool | Arguments |
| --- | --- |
| `air_quality_aqhi` | `{ "station"?: string }` |
| `pollutant_concentrations` | `{ "station"?: string, "hours"?: 1-24 }` |
| `pollution_summary` | `{ "include_weather"?: boolean }` |
| `hong_kong_weather` | `{}` |
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
- "What does 'High' mean for me?" → `advice --risk high`; the answer is the official
  EPD wording, not a paraphrase.
- Monitoring: `aqhi --fail-above 7` exits 3, so a cron job can act on it directly.
- Something looks broken → `doctor`, which probes every source and reports latency.

The cache is on by default (5 min for current readings, 15 min for forecasts,
30 min for station history, 24 h for the static advice pages). Pass `--no-cache` when the user asks for "right now"
after an earlier call in the same minute, and `--max-age 0` to force a refetch.

## Attribution

Data © Government of the Hong Kong Special Administrative Region: the
Environmental Protection Department (AQHI, pollutant concentrations, forecast
and station history) and the Hong Kong Observatory (weather readings).
Street-level data is the PRAISE-HK model © HKUST Institute for the
Environment. Keep this attribution when you redistribute the data.

## Source and offline builds

The Rust source ships with the skill, so `scripts/ensure-binary.sh` can run
`cargo build --release --locked` when no prebuilt binary is reachable.
`scripts/install.sh` follows the same rule, and honors
`HKPOLLUTION_REPO`, `HKPOLLUTION_VERSION` and `HKPOLLUTION_BIN_DIR`.

MIT licensed.
