# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Prebuilt `hkpollution` binaries for macOS (Intel and Apple silicon) and
  Linux (x86_64 and aarch64, static musl), published on every `v*` tag with a
  `SHA256SUMS` file beside them.
- `scripts/install.sh`: POSIX `sh` installer that downloads the release
  tarball matching the host, verifies its SHA256 checksum, installs the binary
  atomically into `$HKPOLLUTION_BIN_DIR` (default `~/.local/bin`), and falls
  back to `cargo build --release --locked` from the checked-out source.
- `scripts/ensure-binary.sh`: prints the absolute path of a working binary,
  resolving `$HKPOLLUTION_BIN`, then `PATH`, then `install.sh`, then a local
  cargo build.
- `SKILL.md` at the repository root, so the repo doubles as an agent skill
  installable with `npx skills add hongkong-ai-labs/hongkong-pollution-skill`.
- `justfile` with `check`, `build-release`, `skill-check`, `version`, `tag`
  and `release` recipes, a `just`-driven release tag and push, and a
  `skill package` CI job that validates the skill manifest and shell syntax.
- Thin-LTO, single-codegen-unit release profile for smaller binaries.

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

[Unreleased]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hongkong-ai-labs/hongkong-pollution-skill/releases/tag/v0.1.0
