# hongkong-pollution-skill

Hong Kong air pollution data in one binary. `hkpollution` works two ways:

- as a **CLI** for humans and shell scripts, with aligned tables or `--json`, and
- as an **MCP server over stdio** (`hkpollution mcp`), so an AI agent can call the
  data as tools.

It reads official Hong Kong government open data only — no API keys, no
third-party services. Dependencies are limited to `serde`, `serde_json` and
`ureq`; XML is parsed with a small hand-written tag scanner.

## What you get

| Command | Data |
| --- | --- |
| `hkpollution aqhi` | Current Air Quality Health Index for all 18 EPD stations |
| `hkpollution pollutants` | Past 24 h NO2, O3, SO2, CO, PM10 and PM2.5 per station |
| `hkpollution summary` | Network worst/mean AQHI, risk counts, worst PM2.5 and NO2 station, optional HKO weather |
| `hkpollution weather` | HKO temperature, humidity and active warnings |
| `hkpollution mcp` | The same four datasets as MCP tools |

## Data sources

All values come from these official endpoints, exactly as documented:

| Source | Endpoint | Used for |
| --- | --- | --- |
| EPD AQHI dashboard (JSON) | `https://dashboard.data.gov.hk/api/aqhi-individual?format=json` | Primary current AQHI per station |
| EPD AQHI (RSS) | `https://www.aqhi.gov.hk/epd/ddata/html/out/aqhi_ind_rss_Eng.xml` | Fallback when the JSON host fails |
| EPD 24-hour pollutant concentrations (XML) | `https://www.aqhi.gov.hk/epd/ddata/html/out/24pc_Eng.xml` | Hourly NO2/O3/SO2/CO/PM10/PM2.5, µg/m³ |
| HKO current readings (JSON) | `https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en` | Temperature, humidity, warnings |

**Attribution.** Data is published by the Government of the Hong Kong Special
Administrative Region: the [Environmental Protection Department](https://www.aqhi.gov.hk/)
(Air Quality Health Index, pollutant concentrations) and the
[Hong Kong Observatory](https://www.hko.gov.hk/) (weather readings). Please keep
their attribution when you redistribute the data.

The 18 AQHI stations are Central/Western, Southern, Eastern, Kwun Tong,
Sham Shui Po, Kwai Chung, Tsuen Wan, Tseung Kwan O, Yuen Long, Tuen Mun,
Tung Chung, Tai Po, Sha Tin, North, Tap Mun, Causeway Bay, Central and
Mong Kok. The last three are roadside stations.

## Install

```sh
git clone https://github.com/hongkong-ai-labs/hongkong-pollution-skill
cd hongkong-pollution-skill
cargo build --release
./target/release/hkpollution --version
```

The binary is `target/release/hkpollution`. Add it to `PATH` or symlink it if
you want to call it from anywhere.

## CLI

```
hkpollution aqhi       [--station <name>] [--json]
hkpollution pollutants [--station <name>] [--hours <1-24>] [--json]
hkpollution summary    [--json] [--with-weather]
hkpollution weather    [--json]
hkpollution mcp
hkpollution --help | --version
```

- `--station` is a case-insensitive **substring** match against the station
  list. Ambiguous names return every match; no match is an error that lists the
  valid stations.
- `--hours N` keeps the newest N hourly readings per station (default `1`,
  maximum `24`).
- `--json` prints exactly one JSON object and no footer, safe to pipe to `jq`.
- Default output is an aligned table with a one-line `source:` footer.

### `hkpollution aqhi`

```console
$ hkpollution aqhi
Station          AQHI  Risk      Published
---------------  ----  --------  ----------------
Central/Western  4     Moderate  2026-11-21 17:30
Southern         4     Moderate  2026-11-21 17:30
Eastern          4     Moderate  2026-11-21 17:30
Kwun Tong        4     Moderate  2026-11-21 17:30
...
source: EPD Air Quality Health Index (fetched 2026-11-21T18:33:13+08:00)
```

Only the roadside station:

```console
$ hkpollution aqhi --station "mong kok"
Station   AQHI  Risk      Published
--------  ----  --------  ----------------
Mong Kok  4     Moderate  2026-11-21 17:30

source: EPD Air Quality Health Index (fetched 2026-11-21T18:33:13+08:00)
```

`central` is ambiguous and returns both `Central/Western` and `Central`:

```console
$ hkpollution aqhi --station central --json | jq -r '.stations[].station'
Central/Western
Central
```

### `hkpollution pollutants`

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

A `-` means the station does not measure that pollutant; in JSON the value is
`null` and the average key is omitted when every reading in the window is null.

### `hkpollution summary`

```console
$ hkpollution summary --with-weather | tail -9

network: worst 4 (Central/Western) / mean 3.9
risk levels: Low 1, Moderate 17
worst PM2.5: Causeway Bay 19.6 µg/m³ (2026-11-21 18:00)
worst NO2: Causeway Bay 102.6 µg/m³ (2026-11-21 18:00)
weather: 2026-11-21T18:02:00+08:00 temperature 20.0 °C, humidity 47.0 %, warnings: The Fire Danger Warning is Red and the fire risk is extreme.

source: EPD / HKO
```

Without `--with-weather` no HKO request is made and the source line is `EPD`.

### `hkpollution weather`

```console
$ hkpollution weather
Reading      Location                    Value  Unit
-----------  --------------------------  -----  -------
Temperature  King's Park                 20.0   C
Temperature  Hong Kong Observatory       20.0   C
...
update time: 2026-11-21T18:02:00+08:00
warnings: The Fire Danger Warning is Red and the fire risk is extreme.

source: HKO current readings
```

### Errors and exit codes

| Exit | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Network, upstream or I/O failure — including a `--station` that matches nothing |
| `2` | Usage error (unknown command or flag, bad `--hours`) |

```console
$ hkpollution aqhi --station kownloon
hkpollution: no station matches "kownloon"; valid stations: Central/Western, Southern, Eastern, Kwun Tong, Sham Shui Po, Kwai Chung, Tsuen Wan, Tseung Kwan O, Yuen Long, Tuen Mun, Tung Chung, Tai Po, Sha Tin, North, Tap Mun, Causeway Bay, Central, Mong Kok
$ echo $?
1
```

Errors always go to stderr; stdout stays clean for `--json`.

## MCP server

`hkpollution mcp` speaks JSON-RPC 2.0 over stdio: one JSON object per line,
UTF-8, in and out. Protocol revision `2024-11-05`.

### Client configuration

```json
{
  "mcpServers": {
    "hongkong-pollution": {
      "command": "/absolute/path/to/target/release/hkpollution",
      "args": ["mcp"]
    }
  }
}
```

Any MCP client that supports stdio servers can use that snippet; point
`command` at the built binary.

### Tools

| Tool | Arguments | Returns |
| --- | --- | --- |
| `air_quality_aqhi` | `{ "station"?: string }` | Current AQHI per station, filtered when `station` is given |
| `pollutant_concentrations` | `{ "station"?: string, "hours"?: integer 1-24 }` | Latest readings plus the hourly series, per station |
| `pollution_summary` | `{ "include_weather"?: boolean }` | The CLI `summary` payload |
| `hong_kong_weather` | `{}` | The CLI `weather` payload |

Every result is `{"content":[{"type":"text","text":"<pretty JSON>"}],"isError":false}`.
An upstream failure returns `isError: true` with a readable message instead of a
transport error, so the session survives an outage.

### Session example

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

## JSON shapes

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
`worst`, `mean_aqhi`, `risk_counts`, `worst_pm25`, `worst_no2` and (only with
`--with-weather`) `weather`:

```json
{"fetched_at":"2026-11-21T18:33:13+08:00","stations":[...],
 "worst":{"station":"Central/Western","aqhi":4,"health_risk":"Moderate","publish_date":"2026-11-21T17:30:00"},
 "mean_aqhi":3.9,"risk_counts":{"Low":1,"Moderate":17},
 "worst_pm25":{"station":"Causeway Bay","value":19.6,"time":"2026-11-21T18:00:00+08:00"},
 "worst_no2":{"station":"Causeway Bay","value":102.6,"time":"2026-11-21T18:00:00+08:00"},
 "source":"EPD"}
```

Timestamps are ISO-8601 in Hong Kong time; concentrations are µg/m³.

## Development

```sh
cargo fmt
cargo clippy -- -D warnings
cargo test        # no network: every parser test uses tests/fixtures/*
cargo build --release
```

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

Implementation decisions for anything the spec left open are recorded in
[`NOTES.md`](NOTES.md).

## License

MIT © [hongkong-ai-labs](https://github.com/hongkong-ai-labs). See [LICENSE](LICENSE).
