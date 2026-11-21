# Implementation notes

The places where `SPEC.md` left a choice open, and the reading taken.

## Output and filtering

- **`--json` is compact, one line.** "A single JSON object" is printed with
  `serde_json::to_string`, so `| jq` sees exactly one value and no footer.
  The MCP tools use the same payloads pretty-printed.
- **`--station` resolves against the canonical 18-station list, not against the
  live response.** That makes zero-match an error even if a feed is down, and it
  gives a stable list to print in the error message. Filtering then compares
  station names case-insensitively for exact equality with the resolved names,
  so `--station central` returns both `Central/Western` and `Central`.
- **Zero-match exits 1, not 2.** The spec says the no-match error exits 1 and
  separately reserves 2 for usage errors; `Error::NoMatch` is therefore a
  runtime error and only `Error::Usage` maps to exit code 2.
- **`--station` / `--hours` are only accepted by the commands that document
  them.** `hkpollution weather --station X` is a usage error rather than a
  silently ignored flag. Both `--flag value` and `--flag=value` are accepted.
- **Averages are rounded to one decimal place** (matching the one-decimal input
  values) and are computed over the already-windowed series, so `--hours 2`
  averages the two newest readings.
- **`series` is oldest-first and ends with `latest`.** The feed order is not
  relied on: readings are sorted by their normalised ISO-8601 timestamp.
- **The pollutants table adds an `avg N` row per station** when more than one
  hour is shown; with the default `--hours 1` the table is just the latest
  reading per station.

## Sources

- **The RSS fallback is reported as `"source": "EPD"`.** Both endpoints are EPD
  data; the transport used is not part of the payload.
- **RSS scanning only looks at `<title>` and `<description>`.** `<pubDate>`
  contributes day/month integers that would otherwise be mistaken for an AQHI
  value (a publish date of 20 Nov yields `20`, and the month `11` would read as
  AQHI 11). The AQHI is the first 1..=11 integer token in title plus description:
  `11` is the "Serious" band, so the accepted range is one wider than the
  numeric 1-10 scale.
- **`health_risk` is matched by keyword** (`Very High`, `Serious`, `High`,
  `Moderate`, `Low`, longest first) when the feed only gives prose.
- **`last_build_date` is kept exactly as published** (`Sat, 21 Nov 2026
  17:30:00 +0800`). Per-reading `<DateTime>` values are normalised to ISO-8601
  with the original offset, because the JSON contract shows
  `2026-11-20T18:00:00+08:00`.
- **Warning messages have their HTML tags stripped** and inner whitespace
  collapsed, since HKO wraps warnings in `<b>`/`<br>` markup.
- **`fetched_at` is computed without a date-time crate** from
  `SystemTime::now()` plus a fixed `+08:00` offset (Hong Kong has had no DST
  since 1979).

## summary

- **`summary` fetches both EPD endpoints** (AQHI plus the 1-hour pollutant
  window) so it can name the worst PM2.5 and NO2 station; HKO is only contacted
  with `--with-weather`, and `source` becomes `"EPD / HKO"` in that case.
- **`worst` is the station with the highest AQHI**; `mean_aqhi` averages the
  stations that reported a value. Risk counts only include reported bands.
- The summary JSON shape is not pinned by the spec, so it is:
  `{fetched_at, stations, worst, mean_aqhi, risk_counts, worst_pm25, worst_no2,
  weather?, source}` where `weather` is omitted unless requested.

## MCP

- **`initialize` always answers `"protocolVersion": "2024-11-05"`**, as the spec
  words it, rather than negotiating the client's requested revision.
- **Any method starting with `notifications/` produces no reply**, whatever it
  is; `notifications/initialized` is therefore handled by the same rule.
- **Blank input lines are skipped.** Only non-empty lines that fail to parse get
  `-32700`, which avoids answering stray newlines.
- **Unknown tool names return JSON-RPC error `-32602`** (invalid params) because
  the method `tools/call` itself exists; upstream failures inside a real tool
  return `result.isError = true` with a readable message instead.
- **A missing `arguments` object is treated as `{}`**, so `pollution_summary`
  and `hong_kong_weather` can be called with only a name.

## App APIs (forecast, history, street)

- **The app APIs are additive, not primary.** `app.epd-asmg.gov.hk/api/v2`
  and `praise-web.ust.hk:8443` are undocumented endpoints behind the mobile
  apps, so they can change without notice. `aqhi`, `pollutants`, `summary`
  and `weather` keep reading the documented open-data feeds; the new commands
  only add to them, and an app API failure is an ordinary `Error` (exit 1) or
  MCP `isError: true`, never a panic.
- **Source labels separate the transports.** `"source": "EPD"` still means the
  documented EPD feeds, the app API reports `"source": "EPD app API"`, and
  street-level reports `"source": "PRAISE-HK"`. That keeps the provenance of
  an undocumented endpoint visible without changing existing payloads.
- **The `--hours` maxima follow each upstream.** `pollutants` keeps 1-24
  because the XML feed publishes 24 hours; `history` caps at 23 because
  `/monitors/{id}` returns exactly 23 history entries; `street` allows the
  plan's 1-48 over the 48-hour forecast window. Defaults are 1 for `history`
  and 24 for `street`, so the street report is useful without a flag.
- **The id→name table in `model.rs` is static.** The app API names stations in
  Traditional Chinese only, and station identity is part of the JSON
  contract, so the 18 rows were verified against the live `/monitors` list on
  2026-11-21 and hard-coded. `--station` still resolves against the canonical
  English list first, then the resolved names map to monitor ids, so an
  unknown or ambiguous name behaves exactly as it does for `aqhi`.
- **`history --json` always returns the full series.** `hours` only trims the
  table; the payload carries all 23 upstream entries newest first, with
  `latest` repeating the first one.
- **`street` always requests the 48-hour window** regardless of `--hours`,
  then truncates, so `current` is stable and a short table does not change
  the upstream query.
- **The forecast join is positional plus by English label.** The two lists are
  paired by position and `forecast_period_en`; a period present on only one
  side is still emitted with `null` for the other.

## Misc

- **Summary logic lives in `aqhi.rs`** (plus the pure `model::rollup`) rather
  than a new module, to keep the module list from the spec exact. Similarly
  `model.rs` owns station lookup and `pollutants.rs` owns the pollutant
  extremes.
- **Timestamp conversion handles RFC 2822, ISO-8601 and `GMT`/`Z` offsets** and
  falls back to the trimmed input when it understands neither.
