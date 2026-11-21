# PLAN: fill the gaps — brief, advice, cache, retry, doctor, and distribution

Implement in four bounded chunks, in order. Take the simplest reading of anything
ambiguous and record the choice in NOTES.md. No new crates unless a chunk says so.

## Chunk 1 — the reliability and "just answer me" layer

### 1.1 HTTP retry (`src/http.rs`)

- Try up to 3 attempts for transport errors, 5xx and 429. Do not retry other 4xx.
- Backoff 300 ms then 900 ms. If a response carries `Retry-After: <seconds>` and the
  value is 1..=5, wait that instead.
- Env overrides: `HKPOLLUTION_RETRIES` (1-5, default 3 total attempts),
  `HKPOLLUTION_TIMEOUT_SECS` (default 20 read / 10 connect as today).
- Keep the existing User-Agent and error types.

### 1.2 On-disk TTL cache (`src/cache.rs`)

Purpose: an agent calls the MCP tools in a loop; we must not hammer EPD or PRAISE.

- Directory: `$HKPOLLUTION_CACHE_DIR`, else `$XDG_CACHE_HOME/hkpollution`, else
  `~/.cache/hkpollution`. Create lazily, `0700` on Unix.
- One file per URL: FNV-1a 64 of the URL as hex + `.json`, holding
  `{"url": ..., "stored_at": <unix secs>, "body": "..."}`.
- `http::get_cached(url, ttl_secs)` reads a fresh entry or fetches and writes one.
  Cache I/O never fails a command: any error is ignored and the live request runs.
- TTLs: AQHI JSON/RSS 300, 24pc 300, HKO rhrread 300, HKO fnd 900, EPD `/forecast` 900,
  EPD `/monitors` 300, EPD `/monitors/{id}` 1800, PRAISE `pollutant_index` 300, all other
  PRAISE reads 3600, `health_advices`/`abouts`/`remarks` 86400.
- Flags: `--no-cache` (bypass read and write), `--max-age <secs>` (override the TTL of
  this run), `--refresh` (alias for `--max-age 0`). Accepted by every network command.
- `--no-cache` must also be honoured by the MCP tools through an optional
  `"no_cache": true` argument on each tool.

### 1.3 `hkpollution advice [--risk <band>] [--lang en|tc|both] [--json]`

Source: `GET https://app.epd-asmg.gov.hk/api/v2/health_advices` (returns five objects,
`health_risk_category` 0-4, four HTML strings each, Traditional Chinese). Strip tags and
collapse whitespace exactly as the weather warnings already do.

English output comes from this **official EPD table** (aqhi.gov.hk Recommended
Precautionary Actions), verbatim except `physicalexertion` → `physical exertion`:

| Band | heart_and_respiratory | children_and_elderly | outdoor_workers | general_public |
| --- | --- | --- | --- | --- |
| 1-3 Low | No response action is required. | No response action is required. | No response action is required. | No response action is required. |
| 4-6 Moderate | No response action is normally required. Individuals who are experiencing symptoms are advised to consider reducing outdoor physical exertion. | No response action is required. | No response action is required. | No response action is required. |
| 7 High | People with existing heart or respiratory illnesses are advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic. They should also seek advice from a medical doctor before participating in sport activities and take more breaks during physical activities. | Children and the elderly are advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic. | No response action is required. | No response action is required. |
| 8-10 Very High | People with existing heart or respiratory illnesses are advised to reduce to the minimum outdoor physical exertion, and to reduce to the minimum the time of their stay outdoors, especially in areas with heavy traffic. | Children and the elderly are advised to reduce to the minimum outdoor physical exertion, and to reduce to the minimum the time of their stay outdoors, especially in areas with heavy traffic. | Employers of outdoor workers performing heavy manual work are advised to assess the risk of outdoor work, and take appropriate preventive measures to protect the health of their employees such as reducing outdoor physical exertion, and reducing the time of their stay outdoors, especially in areas with heavy traffic. | The general public is advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic. |
| 10+ Serious | People with existing heart or respiratory illnesses are advised to avoid outdoor physical exertion, and to avoid staying outdoors, especially in areas with heavy traffic. | Children and the elderly are advised to avoid outdoor physical exertion, and to avoid staying outdoors, especially in areas with heavy traffic. | Employers of all outdoor workers are advised to assess the risk of outdoor work, and take appropriate preventive measures to protect the health of their employees such as reducing outdoor physical exertion, and reducing the time of their stay outdoors, especially in areas with heavy traffic. | The general public is advised to reduce to the minimum outdoor physical exertion, and to reduce to the minimum the time of their stay outdoors, especially in areas with heavy traffic. |

- `--lang en` (default) uses the table; `--lang tc` uses the upstream text; `--lang both`
  returns both with `advice_zh` alongside `advice`.
- `--risk` accepts `low|moderate|high|very-high|serious` or a numeric band (`1-3`, `4-6`,
  `7`, `8-10`, `11`); anything else is a usage error (exit 2). Without it, all five print.
- Table: `Band | AQHI | Audience | Advice`, wrapped to a sane width.
- JSON: `{"bands":[{"risk":"Moderate","aqhi_range":"4-6","advice":{"heart_and_respiratory":"…","children_and_elderly":"…","outdoor_workers":"…","general_public":"…"},"advice_zh":{…}?}],"lang":"en","source":"EPD app API","fetched_at":"…"}`
- `advice_zh` present only for `--lang tc|both`. If the upstream request fails and the
  language is English, still print the English table with `"source":"EPD"` and a stderr
  warning — the English advice is static and must not be blocked by an app API outage.

### 1.4 Nearest-station resolution (`model.rs`)

Add the 18 station coordinates as static data (they are in the `/monitors` payload and in
`tests/fixtures/epd_monitors.json`):

Central/Western 22.2849,114.1441 · Southern 22.2475,114.16 · Eastern 22.2829,114.2194 ·
Kwun Tong 22.3096,114.2311 · Sham Shui Po 22.3301,114.1589 · Kwai Chung 22.3568,114.1293 ·
Tsuen Wan 22.3717,114.1144 · Tseung Kwan O 22.318,114.2594 · Yuen Long 22.4455,114.0232 ·
Tuen Mun 22.3911,113.9767 · Tung Chung 22.2889,113.9436 · Tai Po 22.451,114.1641 ·
Sha Tin 22.3765,114.1847 · North 22.4968,114.1262 · Tap Mun 22.4712,114.3608 ·
Causeway Bay 22.2801,114.1851 · Central 22.2819,114.158 · Mong Kok 22.3226,114.1683

- `nearest_station(lat, lng) -> (name, distance_km)` using the haversine formula.
- `--near <lat>,<lng>` becomes an alternative to `--station` on `aqhi`, `history`,
  `summary` and `brief`. Both flags at once is a usage error. Out-of-HK coordinates are a
  usage error, same bounds as `street` (lat 22.1-22.6, lng 113.8-114.5).
- `aqhi`/`history` print the resolved station and its distance in the footer:
  `station: Sham Shui Po (1.2 km away)`.

### 1.5 `hkpollution brief --lat L --lng L [--json]`

One call that answers "what is the air like where I am". Compose, each part optional:

1. `place`: PRAISE `GET /location/reverse_geocoding?mode=all&lang=en&lat&lng`, first result.
2. `street`: PRAISE `pollutant_index` for the current hour (`aqhi`, `added_health_risk`).
3. `station`: nearest EPD station from 1.4, its official AQHI and risk band from the
   `/monitors` payload, plus `distance_km`.
4. `weather`: HKO `rhrread` temperature, humidity, warnings (reuse the weather module).
5. `advice`: the row for the **worse** of the street and station bands, English text.

- A failing component sets `"<part>_error": "…"` and leaves the part out; the command
  still exits 0 when at least one of `station` or `street` succeeded, and 1 when both
  failed. `"partial": true` marks a partial payload, with a one-line warning on stderr.
- Table output is a small aligned key/value report, not a wide table.
- JSON:
  `{"location":{"lat":…,"lng":…,"place":"…"},"street":{"time":"…","aqhi":4,"added_health_risk":6.3},"station":{"name":"Sham Shui Po","aqhi":4,"risk":"Moderate","time":"…","distance_km":1.2},"weather":{"temperature":20.0,"humidity":47,"warnings":["…"]},"advice":{"risk":"Moderate","general_public":"…","outdoor_workers":"…"},"partial":false,"source":"EPD / PRAISE-HK / HKO","fetched_at":"…"}`

### 1.6 `--fail-above <N>` on `aqhi`, `summary`, `history`, `brief`

- After printing the normal output, exit **3** when the worst AQHI in the payload is ≥ N.
- N outside 1..=11 is a usage error (exit 2). `brief` uses the worse of street and station.
- Document exit code 3 in the usage text, README and SKILL.md.

### 1.7 `hkpollution doctor [--json]`

Probe every source once (no cache) and print `SOURCE | STATUS | MS | DETAIL`:

`data.gov.hk AQHI`, `aqhi.gov.hk RSS`, `aqhi.gov.hk 24pc`, `HKO rhrread`, `HKO fnd`,
`EPD app forecast`, `EPD app monitors`, `PRAISE weather`, `PRAISE pollutant_index`.

- `STATUS` is `ok`, `fail:<reason>` or `schema:<missing key>`; the schema check asserts the
  required top-level keys of each payload (e.g. `stations` for AQHI JSON, `general_stations`
  for forecast, `AQHIBN` for pollutant_index).
- Also print version, cache directory, cached entry count and total bytes.
- Exit 1 if any source fails or mismatches. JSON: `{"checks":[{...}],"summary":{"ok":8,"failed":1},"version":"…","cache":{"dir":"…","entries":3,"bytes":1234},"fetched_at":"…"}`

### 1.8 MCP tools for chunk 1

`air_quality_briefing` (`{"lat","lng","no_cache"?}`), `health_advice`
(`{"risk"?,"lang"?}`), `source_health` (`{}`). Existing tools gain the optional
`no_cache` boolean. Leave tool names and payload shapes otherwise untouched.

### 1.9 Tests for chunk 1

Offline only. Fixtures: `tests/fixtures/epd_health_advices.json`,
`praise_reverse_geocoding.json`, `hko_fnd.json`. Unit-test: band mapping both ways, the
English table's five rows and four audiences, nearest-station maths (a known point must
resolve to Sham Shui Po within 0.1 km), `--fail-above` thresholds, cache TTL
read/write/expiry with a temp dir, and the `brief` assembly with a failing component.
Integration: `doctor --json` shape with a stub, CLI exit codes.

## Chunk 2 — the remaining data

### 2.1 `weather --forecast`

HKO 9-day forecast (`dataType=fnd&lang=en`): `generalSituation` plus the first three days
of `weatherForecast` (`forecastDate`, `week`, `forecastWeather`, `forecastMaxtemp`,
`forecastMintemp`, `forecastMaxRH`, `forecastMinRH`, `PSR`). Default off; `--forecast`
adds it to the table and the JSON under `"forecast"`.

### 2.2 `summary` trend

`trend` computed from the worst station's `aqhi_histories` (one extra request):
`{"station":"…","hours":3,"delta":2,"direction":"rising"|"falling"|"steady","from":4,"to":6}`.
Direction is `steady` when `delta` is 0. `--no-trend` skips the request.

### 2.3 `hkpollution map --bbox lat0,lng0,lat1,lng1 [--pid <id>] [--at <iso>] [--json]`

Calls PRAISE `pollutant_spatial_tiles` and prints the tile URL, its actual coverage and
`ts`. Defaults: `pid=AQHIBN`, `at=now` (top of the hour), `level=1`, `ver=1`. Validates the
bbox is inside Hong Kong and `lat0<lat1`, `lng0<lng1`; usage error otherwise (exit 2).
Make clear in output and docs that this is a link to a generated PNG, not data.

### 2.4 `hkpollution about [--json]`

`/abouts` (title + stripped content + url), `/long_term_aq` URL and `/general_remark`
strings in one report. Table plus JSON. Mostly for completeness and for agents asked
"what is AQHI?".

### 2.5 Tests for chunk 2

`tests/fixtures/hko_fnd.json`, `praise_spatial_tiles.json`, `epd_abouts.json`; unit tests
for the forecast mapping, trend direction, bbox validation and the map URL passthrough.

## Chunk 3 — MCP protocol, skill and docs

### 3.1 MCP protocol 2025-06-18

- `initialize` answers `"protocolVersion": "2025-06-18"` (keep 2024-11-05 accepted for a
  client that requests it — negotiate: reply with the requested revision if it is one of
  the two, else the newest).
- Every tool result gains `structuredContent` holding the same JSON object the CLI prints,
  and `tools/list` gains an `outputSchema` per tool (a permissive object schema is fine:
  `{"type":"object","additionalProperties":true}`), plus `annotations`:
  `{"readOnlyHint": true, "idempotentHint": true, "openWorldHint": true}`.
- Keep the `content[0].text` pretty JSON so older clients still work.

### 3.2 SKILL.md

Add an "Answering patterns" section mapping question shapes to commands, e.g.
"is it safe to run outside in Causeway Bay?" → `brief --lat --lng` or
`aqhi --station "causeway bay"` then `advice --risk <band>`; "will it be better tomorrow?"
→ `forecast`; "what's the forecast at my street?" → `street`. Mention `--fail-above` and
exit 3, the cache flags, and `doctor` for troubleshooting. Keep the frontmatter keys and
stay under 500 body lines.

### 3.3 README / CHANGELOG / NOTES

Document every new command and flag, the new sources, the exit code 3, the cache (location,
TTL table, how to disable), and the English advice provenance (official EPD table, TC comes
from the app API). CHANGELOG gets an `## [Unreleased]` entry. NOTES records the pinned
advice table, the cache key choice, the `--fail-above` code, and why brief degrades
per-part.

## Chunk 4 — distribution and drift detection

### 4.1 crates.io

Fill in `Cargo.toml` metadata (`homepage`, `documentation`, `keywords`, `categories`,
`readme`, `exclude` for `tests/fixtures`, `target`), confirm `cargo package --list` and
`cargo publish --dry-run` pass, and document `cargo install hkpollution`. Do not publish:
that needs a crates.io token the repo does not have.

### 4.2 Docker image

`Dockerfile` (`FROM scratch`, copy the static musl binary, `ENTRYPOINT ["/hkpollution"]`)
and a release-job step pushing `ghcr.io/hongkong-ai-labs/hkpollution:<tag>` and `:latest`
for both linux targets, logging in with `GITHUB_TOKEN` (`packages: write`). Document
`docker run --rm ghcr.io/hongkong-ai-labs/hkpollution aqhi`.

### 4.3 Homebrew

Release job generates `Formula/hkpollution.rb` from the two darwin tarballs' sha256
(`class Hkpollution < Formula`, `on_arm`/`on_intel` URLs, `bin.install "hkpollution"`,
`test do ... end` with `--version`) and pushes it to
`hongkong-ai-labs/homebrew-tap` on `main` using the existing token, tolerating a missing
repo by skipping with a warning. Document `brew tap hongkong-ai-labs/tap && brew install hkpollution`.

### 4.4 Nightly drift detection

`.github/workflows/drift.yml`, daily 01:30 UTC + `workflow_dispatch`:
- Run every command with `--json --no-cache`, then compare the top-level key set of each
  payload against `tests/schema/*.json` (committed snapshots, one per source).
- Run `doctor --json` and fail on any `fail:`/`schema:` status.
- On mismatch or failure, open or update a single issue labelled `drift` with the diff,
  using `gh issue create`/`gh issue comment`. Never fail silently.

### 4.5 Fixture refresh

`UPDATE_FIXTURES=1 cargo test --test parsing` rewrites the fixtures from the live sources
for the sources it covers, printing each file it writes. Guarded so CI never does it.

## Acceptance for every chunk

`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`,
`cargo build --release`, `just skill-check`, `actionlint`, and one live run of each new
command pasted into the report. No existing command's output may change except where this
plan says so.
