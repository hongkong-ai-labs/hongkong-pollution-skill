# PLAN: distributable skill + tagged releases

Implement this plan exactly. Where something is ambiguous, take the simplest reading and
note it in NOTES.md. Do not add dependencies. Do not change the CLI or the MCP contract.

Repo: Rust CLI + MCP stdio server, binary `hkpollution`, version lives in Cargo.toml.
It is *also* an agent skill: this repo root is meant to be installable with
`npx skills add hongkong-ai-labs/hongkong-pollution-skill`, so `SKILL.md` must sit at the
repo root and `name` must equal `hongkong-pollution-skill`.

## Chunk 1 — the skill package

### 1.1 `SKILL.md` (repo root)

YAML frontmatter with exactly `name`, `description`, `license: MIT` (no other keys).
`name: hongkong-pollution-skill`.

Description is the routing trigger, one sentence, < 200 chars, must mention Hong Kong,
AQHI, pollution/air quality and weather.

Body, under 200 lines, plain markdown, no HTML. Sections:

1. What it gives the agent: current AQHI for the 18 EPD stations, past-24h NO2/O3/SO2/
   CO/PM10/PM2.5, a network summary with optional HKO weather, HKO current readings —
   as CLI commands and as four MCP tools.
2. **Before first use** — run `scripts/ensure-binary.sh`; it prints the absolute path of a
   working `hkpollution` binary and exits non-zero only if it could not download or build
   one. Use the printed path for every later command; do not assume `hkpollution` is on
   PATH.
3. Command table + 3 short `console` examples taken from real output (station filter,
   `--json` piped to `jq`, `summary --with-weather`).
4. `--json` contract: exactly one object on stdout, no footer, timestamps ISO-8601 +08:00,
   concentrations µg/m³, `null` when a station does not measure a pollutant.
5. MCP: `hkpollution mcp` speaks JSON-RPC 2.0 over stdio, tools `air_quality_aqhi`,
   `pollutant_concentrations`, `pollution_summary`, `hong_kong_weather`; give the client
   JSON snippet with `command` set to the binary path from step 2.
6. Exit codes 0/1/2 and "errors go to stderr".
7. Attribution: data © Government of the HKSAR (EPD, HKO); keep it when redistributing.
8. A note that the Rust source ships with the skill, so `scripts/ensure-binary.sh` can
   `cargo build --release --locked` when no prebuilt binary is reachable.

### 1.2 `scripts/install.sh`

POSIX `sh`, `set -eu`, no dependencies beyond curl/wget, tar, and shasum or sha256sum.

- Target detection from `uname -s` / `uname -m`:
  Darwin/x86_64 → `x86_64-apple-darwin`, Darwin/arm64 → `aarch64-apple-darwin`,
  Linux/x86_64|amd64 → `x86_64-unknown-linux-musl`, Linux/aarch64|arm64 →
  `aarch64-unknown-linux-musl`. Anything else: clear error on stderr, exit 1.
- Env overrides: `HKPOLLUTION_REPO` (default `hongkong-ai-labs/hongkong-pollution-skill`),
  `HKPOLLUTION_VERSION` (default: resolve `.../releases/latest` via the GitHub API),
  `HKPOLLUTION_BIN_DIR` (default `$HOME/.local/bin`).
- Download `hkpollution-${TAG}-${TARGET}.tar.gz` and `SHA256SUMS` from
  `https://github.com/$REPO/releases/download/$TAG/...`, verify the checksum before
  extracting, extract the binary, `chmod +x`, then move it into place atomically
  (temp file + `mv`).
- Print the installed absolute path as the only stdout line. All progress logs go to
  stderr. Honour `HKPOLLUTION_FORCE_INSTALL=1` to reinstall over an existing binary.
- If the download fails and the script's parent directory contains `Cargo.toml` **and**
  cargo is on PATH: `cargo build --release --locked` in that directory and install
  `target/release/hkpollution` instead. Otherwise fail with a message naming both paths
  that were tried.
- Works via `curl -fsSL .../main/scripts/install.sh | sh`.

### 1.3 `scripts/ensure-binary.sh`

`bash`, `set -euo pipefail`. Resolution order:

1. `$HKPOLLUTION_BIN` if set and executable.
2. `hkpollution` on PATH.
3. `scripts/install.sh` (its stdout is the path).
4. `cargo build --release --locked` in the repo root next to `scripts/`.

Print the absolute path as the only stdout line. Exit 1 with a one-line reason on stderr
if all four fail. Keep it under 40 lines.

## Chunk 2 — CI, release automation, justfile, changelog

### 2.1 `.github/workflows/release.yml`

Trigger: `push` on tags `v*` and `workflow_dispatch` with a `tag` input. Permissions:
`contents: write`. `env: CARGO_TERM_COLOR: always`.

Job `verify` (ubuntu-latest): resolve the version from `cargo metadata --no-deps
--format-version 1` with jq, fail unless the tag is exactly `v$version`; fail unless
CHANGELOG.md contains a `## [x.y.z]` heading for that version. Outputs: `version`, `tag`.

Job `build` (needs verify, matrix):

| target | runs-on |
| --- | --- |
| aarch64-apple-darwin | macos-14 |
| x86_64-apple-darwin | macos-14 |
| x86_64-unknown-linux-musl | ubuntu-24.04 |
| aarch64-unknown-linux-musl | ubuntu-24.04 |

Steps: checkout, `houseabsolute/actions-rust-cross@v1` with `command: build`,
`target`, `args: "--locked --release"`, `strip: true`; then package
`hkpollution-${TAG}-${TARGET}.tar.gz` containing the binary named `hkpollution` and
`LICENSE` (no leading directory), and upload it as an artifact. Verify the packaged
binary runs (`--version` for the native targets only, skip it for foreign targets).

Job `release` (needs build): download every artifact into `dist/`, write a sorted
`SHA256SUMS` (`shasum -a 256` of each tarball, basenames only), extract the CHANGELOG
section for the version into `notes.md` with a short python3 snippet (heading line →
next `## [` heading, exclusive; fall back to `--generate-notes` when empty), install
marker lines for the release body: the `curl … | sh` one-liner and the `npx skills add`
line. Then `gh release create "$TAG" dist/*.tar.gz dist/SHA256SUMS --title "$TAG"
--notes-file notes.md --verify-tag`. Use `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` and
`gh` (preinstalled) — do not add third-party release actions.

### 2.2 `ci.yml`: add a `skill` job

Name it `skill package`. On ubuntu-latest, no Rust: checkout, then a python3 step that
fails unless `SKILL.md` exists at the repo root, its frontmatter has `name` ==
`hongkong-pollution-skill` and a non-empty `description`, and the body is under 500
lines; plus `bash -n` on every `scripts/*.sh` and `sh -n` on `install.sh`. Keep the
existing `check` job untouched.

### 2.3 `justfile`

`set shell := ["bash", "-uc"]`, `set dotenv-load := false`. Recipes (each with a one-line
`#` comment so `just --list` reads well): `default` (→ `check`), `check` (fmt-check +
clippy + test), `fmt`, `fmt-check`, `clippy`, `test`, `audit` (cargo audit, tolerate
missing tool with a message), `build` (debug), `build-release`, `run *ARGS` (cargo run
--), `install` (`cargo install --path . --locked --force`), `skill-check` (the same
SKILL.md validation as CI, calling a python3 heredoc), `version` (print the tag CI
would use), `tag VERSION` (git tag -a "v$VERSION" -m ...), `release VERSION` (tag +
push the tag, which triggers the workflow), `clean`, `ci` (check + build-release).
No recipe may need network except `audit` and `release`.

### 2.4 `CHANGELOG.md`

Keep a Changelog format, `## [Unreleased]`, then `## [0.1.0] - 2026-11-21` with the
shipped features (four commands, MCP stdio server with four tools, EPD/HKO sources with
RSS fallback, `--json`, exit codes) and the packaging added in this change. Add
`[Unreleased]: .../compare/v0.1.0...HEAD` and `[0.1.0]: .../releases/tag/v0.1.0` link
refs at the bottom.

### 2.5 Cargo.toml release profile

Add `[profile.release] lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`,
`panic = "abort"`. Leave everything else alone.

## Acceptance

- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all`, `cargo build --release` all pass.
- `bash -n scripts/*.sh` and `sh -n scripts/install.sh` pass.
- `just --list` shows every recipe.
- `npx skills add ./ --list` lists `hongkong-pollution-skill`.
- Do not touch README.md, SPEC.md, src/, tests/, assets/ — another agent owns those.
- Do not commit. Leave the working tree dirty; the reviewer commits.
