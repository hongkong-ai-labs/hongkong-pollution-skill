# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `river` and `marine`: EPD monthly water quality (dissolved oxygen and BOD5)
  per station, with `--zone`, `--station`, `--months` and the sampling month
  named in every report so it is never read as live data.
- `red-tide`: AFCD red tide sightings since 1975, with `--active` for reports
  that have no dissipation date yet, `--days` for the recent window and
  `--near` for distance sorting.
- `radiation`: HKO hourly ambient gamma dose rate per station in µSv/h, with
  each reading relative to the network median and an `elevated` flag above
  twice that median.
- `uv`: HKO UV index with the WHO exposure band, also added to `weather` and
  to the `brief` payload and table.
- `stats`: AQHI min, max, mean and hours at or above `--threshold` per station
  over 1-365 days, from a local observation log in the cache directory topped
  up by the documented 24-hour feed.
- `compare`: current AQHI ranked across stations with general and roadside
  means kept apart.
- `digest`: one line per source (air, beach, red tide, radiation, river, UV,
  warnings), each degrading to `unavailable` independently.
- `brief` now also carries `uv`, the nearest `beach` (grade, status and latest
  E. coli sample) and `red_tide` within ten kilometres.
- `history` falls back to the documented past-24-hour AQHI XML when the app
  API is unavailable, keeping the same schema with pollutant fields null.
- `doctor` probes all seventeen sources and reports the age of the newest
  timestamp in each payload, flagging `stale` when a feed keeps answering with
  old data; the summary now counts `ok`, `stale` and `failed` separately.
- `beach` joins EPD's yearly E. coli monitoring file to the grading feed, so
  each beach now carries `ecoli`: the latest count per 100 mL, its sampling
  date, the five most recent samples and their geometric mean — the same
  window EPD grades on (Good ≤ 24, Fair 25-180, Poor 181-610, Very Poor > 610).
  A missing or unpublished file degrades to `ecoli: null` with `ecoli_error`
  instead of failing the command, and `ecoli_source` names the year used.
- `beach --near lat,lng` sorts beaches nearest first, adds `distance_km`, and
  the MCP `beach_water_quality` tool takes the same `lat`/`lng` pair.
- `beach` reports `graded` and `open` counts, plus a WGS84 `latitude` and
  `longitude` per beach.
- `doctor` probes the beach grading feed and the yearly E. coli file, taking it
  to eleven sources.

### Changed

- Expired cache entries are revalidated with `If-Modified-Since`; a `304`
  restarts the freshness window without rewriting the body, and a transport
  failure serves the stale body instead of failing the command.
- `beach` reads `beachgrading` (real WGS84 geometry) instead of
  `beachgrading_GeoXY`, whose published points were far outside the territory;
  a point outside the Hong Kong bounds is dropped rather than reported.

## [0.3.0] - 2026-11-21

### Added

- `hkpollution brief --lat --lng`: one call for "what is the air like here" —
  reverse-geocoded place, street-level AQHI and added health risk, nearest EPD
  station with distance, HKO weather, and the advice for the worse of the two
  AQHI values. Parts degrade independently (`partial: true`, `*_error` fields).
- `hkpollution advice [--risk] [--lang en|tc|both]`: the official EPD
  precautionary actions per AQHI band. The English table is compiled in from
  aqhi.gov.hk, so it survives an app API outage; `tc`/`both` add the upstream
  Traditional Chinese text.
- `hkpollution weather --forecast`: the HKO nine-day forecast (general situation
  plus temperature, humidity and rain probability per day).
- `hkpollution map --bbox`: the PRAISE-HK raster tile URL for a bounding box,
  documented as a link to a generated PNG rather than data.
- `hkpollution about`: the app API about pages.
- `hkpollution doctor`: probes all nine sources with latency and a schema check,
  and reports the cache directory and size.
- `summary` gains `trend` (rising/falling/steady over the last hours at the worst
  station), with `--no-trend` to skip the extra request.
- `--near lat,lng` resolves coordinates to the nearest of the 18 stations, as an
  alternative to `--station`.
- `--fail-above N` exits 3 when the worst AQHI reaches N, for cron and CI guards.
- On-disk TTL cache under `~/.cache/hkpollution` with `--no-cache`, `--max-age`
  and `--refresh`; MCP tools accept `no_cache`.
- HTTP retries with backoff and `Retry-After` for transport errors, 5xx and 429.
- Multi-arch container image `ghcr.io/hongkong-ai-labs/hkpollution` built from
  the static musl binaries on every tag, a `Dockerfile`, and a Homebrew formula
  published to `hongkong-ai-labs/homebrew-tap` when `BREW_TOKEN` is set.
- crates.io metadata (`cargo install hkpollution`) and a nightly drift workflow
  that compares live payload shapes with `tests/schema/` and files one issue.
- MCP: protocol revision `2025-06-18` with negotiation down to `2024-11-05`,
  `structuredContent` on every tool result, per-tool `outputSchema` and
  read-only annotations, plus `air_quality_briefing`, `health_advice` and
  `source_health` tools.

## [0.2.0] - 2026-11-21

### Added

- `hkpollution forecast`: tomorrow's AQHI range for general and roadside
  stations, from the undocumented EPD app API.
- `hkpollution history`: hourly AQHI and pollutant history per station,
  newest first, with `--station` and `--hours` (1-23), from the EPD app API.
- `hkpollution street`: PRAISE-HK street-level AQHI, added health risk and
  pollutant forecast for a Hong Kong latitude/longitude, with `--hours`
  (1-48, default 24).
- MCP tools `aqhi_forecast`, `station_history` and `street_level_aqhi`,
  exposing the same payloads as the new commands.
- EPD app API and PRAISE-HK sources, labelled `"source": "EPD app API"` and
  `"source": "PRAISE-HK"`; the documented feeds remain the primary source
  for `aqhi`, `pollutants`, `summary` and `weather`.

## [0.1.0] - 2026-11-21

### Added

- `hkpollution aqhi`: current Air Quality Health Index for the 18 EPD
  monitoring stations, with risk band and publish time.
- `hkpollution pollutants`: past-24-hour NO2, O3, SO2, CO, PM10 and PM2.5
  concentrations per station, with per-pollutant averages over the window.
- `hkpollution summary`: network rollup (worst and mean AQHI, risk-level
  counts, worst PM2.5 and NO2 stations), optionally joined with weather.
- `hkpollution weather`: current HKO temperature, humidity and active weather
  warnings.
- `hkpollution mcp`: JSON-RPC 2.0 over stdio, exposing `air_quality_aqhi`,
  `pollutant_concentrations`, `pollution_summary` and `hong_kong_weather`.
- EPD AQHI and HKO weather sources, with an RSS fallback when the JSON
  endpoint is unavailable.
- `--json` output for every command: exactly one object on stdout, ISO-8601
  timestamps in Hong Kong time, concentrations in µg/m³.
- Exit codes `0` (success), `1` (network, upstream or I/O failure) and `2`
  (usage error), with all diagnostics on stderr.
- `SKILL.md` at the repository root, so the repo doubles as an agent skill
  installable with `npx skills add hongkong-ai-labs/hongkong-pollution-skill`.
- Prebuilt `hkpollution` binaries for macOS (Intel and Apple silicon) and
  Linux (x86_64 and aarch64, static musl), published on every `v*` tag with a
  `SHA256SUMS` file beside them.
- `scripts/install.sh`: POSIX `sh` installer that downloads the release
  tarball matching the host, verifies its SHA256 checksum, installs the binary
  atomically into `$HKPOLLUTION_BIN_DIR` (default `~/.local/bin`), upgrades an
  out-of-date binary in place, and falls back to
  `cargo build --release --locked` from the checked-out source.
- `scripts/ensure-binary.sh`: prints the absolute path of a working binary,
  resolving `$HKPOLLUTION_BIN`, then `PATH`, then `install.sh`, then a local
  cargo build.
- `justfile` with `check`, `build-release`, `skill-check`, `version`, `tag`
  and `release` recipes, so a release is `just release 0.2.0`, plus a
  `skill package` CI job that validates the skill manifest and shell syntax.
- Thin-LTO, single-codegen-unit, stripped release profile for smaller
  binaries.

[Unreleased]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/tag/v0.3.0
[0.2.0]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/tag/v0.2.0
[0.1.0]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/tag/v0.1.0
