#!/usr/bin/env bash
# Detect upstream drift by comparing the top-level JSON keys of every command
# against the committed snapshots in tests/schema/.
#
#   scripts/check-drift.sh            compare, exit 1 on drift
#   scripts/check-drift.sh --update   rewrite the snapshots instead
#
# The nightly workflow runs the compare mode and opens an issue when it fails.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

BIN="${HKPOLLUTION_BIN:-./target/release/hkpollution}"
SNAPSHOTS="tests/schema"
MODE="${1:-check}"
failures=0

commands=(
  "aqhi --json"
  "pollutants --json"
  "forecast --json"
  "history --json"
  "street --lat 22.3375 --lng 114.263 --hours 2 --json"
  "summary --no-trend --json"
  "weather --forecast --json"
  "brief --lat 22.3375 --lng 114.263 --json"
  "advice --risk low --json"
  "about --json"
)

if [ ! -x "$BIN" ]; then
  echo "check-drift: $BIN is missing; run 'just build-release' first" >&2
  exit 1
fi

mkdir -p "$SNAPSHOTS"

for entry in "${commands[@]}"; do
  # Snapshot name is the command word plus the rest, slugified.
  name=$(printf '%s' "$entry" | awk '{print $1}')
  file="$SNAPSHOTS/$name.json"

  if ! output=$($BIN $entry 2>/dev/null); then
    echo "DRIFT $name: command failed to run" >&2
    failures=$((failures + 1))
    continue
  fi

  keys=$(printf '%s' "$output" | python3 -c '
import json, sys
try:
    payload = json.load(sys.stdin)
except Exception as error:
    print("unparsable:", error)
    sys.exit(0)
if not isinstance(payload, dict):
    print("not-an-object")
else:
    print(json.dumps(sorted(payload.keys()), indent=2) + "\n")
')

  if [ "$MODE" = "--update" ]; then
    printf '%s\n' "$keys" > "$file"
    echo "wrote $file: $(printf '%s' "$keys" | tr -d '\n ')"
    continue
  fi

  if [ ! -f "$file" ]; then
    echo "DRIFT $name: no snapshot at $file (run --update to record one)" >&2
    failures=$((failures + 1))
    continue
  fi

  if ! diff -q "$file" <(printf '%s\n' "$keys") >/dev/null; then
    echo "DRIFT $name: top-level keys changed" >&2
    diff "$file" <(printf '%s\n' "$keys") | sed 's/^/    /' >&2
    failures=$((failures + 1))
  fi
done

if [ "$MODE" = "--update" ]; then
  exit 0
fi

if [ "$failures" -ne 0 ]; then
  echo "check-drift: $failures source(s) changed shape" >&2
  exit 1
fi

echo "check-drift: all payload shapes match $SNAPSHOTS"
