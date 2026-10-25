#!/bin/sh
# Host-side A/B sampler for the Tauri launcher. Mirrors scripts/capture/run.py
# container flags; runs scripts/bench/ab_session.py inside (Xvfb + motion=1 +
# bench.py tree sampling) and copies bench.json per repeat.
# Usage: perf-ab-tauri.sh OUTDIR TAG REPEATS DUR WARMUP
set -eu
ROOT=$(
    CDPATH=
    cd -- "$(dirname -- "$0")/../.." && pwd
)
outdir=${1:?OUTDIR}
tag=${2:?TAG}
repeats=${3:-3}
dur=${4:-30}
warmup=${5:-5}
mkdir -p "$outdir"
cache="$ROOT/.cache/capture"
i=0
while test "$i" -lt "$repeats"; do
    echo "uptime before $tag repeat $i: $(uptime)" | tee -a "$outdir/uptime.log"
    scratch=$(mktemp -d --tmpdir blockfield-ab-tauri.XXXXXX)
    # shellcheck disable=SC2064
    trap "rm -rf '$scratch'" EXIT
    printf '{"state":"main","lang":"ru","scale":1,"motion":true,"width":1280,"height":720,"seconds":0,"fps":60}' >"$scratch/request.json"
    env -u DISPLAY -u WAYLAND_DISPLAY timeout --kill-after=5 120 podman run --rm \
        --network none --shm-size 512m \
        -v "$cache/cargo-target/release/blockfield-launcher:/opt/blockfield-launcher:ro,Z" \
        -v "$ROOT/scripts/capture:/harness:ro,Z" \
        -v "$ROOT/scripts/bench/bench.py:/bench.py:ro,Z" \
        -v "$ROOT/scripts/bench/ab_session.py:/ab_session.py:ro,Z" \
        -v "$scratch:/capture:Z" \
        -e "AB_DURATION=$dur" -e "AB_WARMUP=$warmup" -e "AB_HIDDEN=${AB_HIDDEN:-}" \
        localhost/blockfield-capture:latest \
        timeout 110 python3 /ab_session.py
    if python3 -c "import json;d=json.load(open('$scratch/result.json'));exit(0 if 'bench' in d else 1)"; then
        cp "$scratch/bench.json" "$outdir/$tag-repeat$i.json"
        cp "$scratch/result.json" "$outdir/$tag-repeat$i.result.json"
        cp "$scratch/app.log" "$outdir/$tag-repeat$i.app.log"
    else
        echo "REPEAT $i FAILED:"
        tail -c 2000 "$scratch/result.json"
        cp "$scratch/result.json" "$outdir/$tag-repeat$i.FAIL.json"
        cp "$scratch/app.log" "$outdir/$tag-repeat$i.app.log" || true
    fi
    rm -rf "$scratch"
    trap - EXIT
    i=$((i + 1))
done
printf '%s\n' "DONE $tag x$repeats ${dur}s"
