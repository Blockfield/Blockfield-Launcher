#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:?Usage: baseline.sh OUT}"
for state in first-run login main main-launching update-42 stats \
    settings-general settings-runtime settings-commands settings-skin settings-launcher offline error; do
    "$ROOT/scripts/capture/run.sh" "$state" --out "$OUT"
done
for state in main settings-skin; do
    for scale in 1 1.25 1.5; do
        for lang in en ru uk; do
            if [[ "$scale" == 1 && "$lang" == en ]]; then continue; fi
            "$ROOT/scripts/capture/run.sh" "$state" --scale "$scale" --lang "$lang" --out "$OUT"
        done
    done
done
"$ROOT/scripts/capture/run.sh" main --size 1600x900 --out "$OUT"
for state in main main-launching update-42 settings-skin; do
    "$ROOT/scripts/capture/run.sh" "$state" --record 3 --fps 60 --out "$OUT"
done
