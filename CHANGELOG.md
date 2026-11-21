# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/tag/v0.2.0
[0.1.0]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/tag/v0.1.0
