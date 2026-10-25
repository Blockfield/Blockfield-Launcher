#!/bin/sh
set -eu
app_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
report_dir=${1:?Usage: headless.sh REPORT_DIRECTORY [SCALE] [capture|idle|record] [STATE] [SCENARIO]}
scale=${2:-1}
mode=${3:-capture}
state=${4:-shell}
scenario=${5:-default}
mkdir -p "$report_dir"
report_dir=$(CDPATH= cd -- "$report_dir" && pwd)
case "$mode" in capture | idle | record) ;; *)
    echo 'Mode must be capture, idle or record' >&2
    exit 2
    ;;
esac
case "$scale" in 1 | 1.25 | 1.5) ;; *)
    echo 'Scale must be 1, 1.25 or 1.5' >&2
    exit 2
    ;;
esac
session_dir=$(mktemp -d "$report_dir/.session.XXXXXX")
cleanup() {
    if test -f "$session_dir/container.cid"; then
        container_id=$(cat "$session_dir/container.cid")
        # The private cidfile identifies only this invocation's isolated container.
        podman rm --force "$container_id" >/dev/null 2>&1 || :
    fi
    if test -n "${run_pid:-}"; then wait "$run_pid" 2>/dev/null || :; fi
    rm -f "$session_dir/container.cid"
    rmdir "$session_dir"
}
trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
env -u DISPLAY -u WAYLAND_DISPLAY timeout --kill-after=5 45 podman run --rm --cidfile "$session_dir/container.cid" --network=none \
    --security-opt label=disable --read-only --tmpfs /tmp:rw,size=256m --tmpfs /run:rw,size=16m \
    -v "$app_dir:/app:ro" -v "$report_dir:/out:rw" \
    -v "$app_dir/../../scripts/bench/bench.py:/bench.py:ro" \
    -e "SLINT_SCALE_FACTOR=$scale" -e "CAPTURE_MODE=$mode" -e "CAPTURE_STATE=$state" -e "CAPTURE_SCENARIO=$scenario" -e HOME=/tmp \
    localhost/blockfield-main-headless sh -ec '
        unset WAYLAND_DISPLAY
        Xvfb :99 -screen 0 1920x1080x24 -nolisten tcp >/out/xvfb.log 2>&1 &
        attempt=0
        until test -S /tmp/.X11-unix/X99; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 50 || { cat /out/xvfb.log; exit 1; }
            sleep 0.1
        done
        if test "$CAPTURE_MODE" = idle; then
            /app/target/release/blockfield-native-poc --state "$CAPTURE_STATE" --scenario "$CAPTURE_SCENARIO" --idle-seconds 14 >/out/idle.log 2>&1 &
            app_pid=$!
            sleep 2
            python3 /bench.py sample --pid "$app_pid" --duration 10 --interval 0.5 --json-out /out/idle.json
            wait "$app_pid"
        elif test "$CAPTURE_MODE" = record; then
            /app/target/release/blockfield-native-poc --state "$CAPTURE_STATE" --scenario "$CAPTURE_SCENARIO" --idle-seconds 8 >/out/motion.log 2>&1 &
            app_pid=$!
            attempt=0
            until grep -q "^READY pid=" /out/motion.log; do
                attempt=$((attempt + 1))
                test "$attempt" -lt 50 && kill -0 "$app_pid" || { cat /out/motion.log; exit 1; }
                sleep 0.1
            done
            sleep 2
            case "$SLINT_SCALE_FACTOR" in 1) size=1280x720 ;; 1.25) size=1600x900 ;; 1.5) size=1920x1080 ;; esac
            ffmpeg -nostdin -hide_banner -loglevel warning -y -f x11grab -draw_mouse 0 -framerate 60 -video_size "$size" -i :99+0,0 -t 3 -vf setpts=PTS-STARTPTS -c:v ffv1 -level 3 -threads 2 /out/motion.mkv >/out/record.log 2>&1
            wait "$app_pid"
        else
            if test "$CAPTURE_STATE" = shell; then freeze=; else freeze=--freeze-motion; fi
            /app/target/release/blockfield-native-poc --state "$CAPTURE_STATE" --scenario "$CAPTURE_SCENARIO" $freeze --capture-dir /out
            for capture in /out/*.ppm; do
                magick "$capture" "${capture%.ppm}.png"
                rm "$capture"
            done
        fi
    ' &
run_pid=$!
wait "$run_pid"

if test "$mode" = record; then
    ffprobe -v error -select_streams v:0 -show_entries frame=best_effort_timestamp_time -of csv=p=0 "$report_dir/motion.mkv" | awk 'NF {print n++ "," $1}' >"$report_dir/frame-times.csv"
    ffmpeg -nostdin -hide_banner -loglevel warning -y -i "$report_dir/motion.mkv" -t 3 -vf tpad=stop_mode=clone:stop_duration=1,fps=60 -frames:v 180 -c:v libx264 -threads 2 -preset fast -crf 18 -pix_fmt yuv420p "$report_dir/motion.mp4"
    rm "$report_dir/motion.mkv"
    printf '%s\n' 'PASS: private Xvfb recording encoded at 60fps for 3 seconds'
fi
