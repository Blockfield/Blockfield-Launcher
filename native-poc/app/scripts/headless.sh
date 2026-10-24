#!/bin/sh
set -eu
app_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
report_dir=${1:?Usage: headless.sh REPORT_DIRECTORY [SCALE] [capture|idle]}
scale=${2:-1}
mode=${3:-capture}
mkdir -p "$report_dir"
report_dir=$(CDPATH= cd -- "$report_dir" && pwd)
case "$mode" in capture|idle) ;; *) echo 'Mode must be capture or idle' >&2; exit 2 ;; esac
case "$scale" in 1|1.25|1.5) ;; *) echo 'Scale must be 1, 1.25 or 1.5' >&2; exit 2 ;; esac
exec env -u DISPLAY -u WAYLAND_DISPLAY timeout 45 podman run --rm --network=none \
    --security-opt label=disable --read-only --tmpfs /tmp:rw,size=256m --tmpfs /run:rw,size=16m \
    -v "$app_dir:/app:ro" -v "$report_dir:/out:rw" \
    -v "$app_dir/../../scripts/bench/bench.py:/bench.py:ro" \
    -e "SLINT_SCALE_FACTOR=$scale" -e "CAPTURE_MODE=$mode" -e HOME=/tmp \
    localhost/blockfield-native-headless sh -ec '
        unset WAYLAND_DISPLAY
        Xvfb :99 -screen 0 1920x1080x24 -nolisten tcp >/out/xvfb.log 2>&1 &
        attempt=0
        until test -S /tmp/.X11-unix/X99; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 50 || { cat /out/xvfb.log; exit 1; }
            sleep 0.1
        done
        if test "$CAPTURE_MODE" = idle; then
            /app/target/release/blockfield-native-poc --idle-seconds 14 >/out/idle.log 2>&1 &
            app_pid=$!
            sleep 2
            python3 /bench.py sample --pid "$app_pid" --duration 10 --interval 0.5 --json-out /out/idle.json
            wait "$app_pid"
        else
            /app/target/release/blockfield-native-poc --capture-dir /out
            for capture in /out/*.ppm; do
                magick "$capture" "${capture%.ppm}.png"
                rm "$capture"
            done
        fi
    '
