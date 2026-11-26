#!/bin/sh
set -eu
# Pacing self-check for the settings skin preview: in-app tab-switch/drag
# assertions plus OS minimize/restore under Openbox. Needs the Vulkan-capable
# headless image (Headless.Containerfile) and an isolated display; never the
# owner's desktop.
app_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
report_dir=${1:?Usage: skin_self_check.sh REPORT_DIRECTORY}
mkdir -p "$report_dir"
report_dir=$(CDPATH= cd -- "$report_dir" && pwd)
session_dir=$(mktemp -d "$report_dir/.session.XXXXXX")
cleanup() {
    if test -f "$session_dir/container.cid"; then
        container_id=$(cat "$session_dir/container.cid")
        podman rm --force "$container_id" >/dev/null 2>&1 || :
    fi
    if test -n "${run_pid:-}"; then wait "$run_pid" 2>/dev/null || :; fi
    rm -f "$session_dir/container.cid"
    rmdir "$session_dir"
}
trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
env -u DISPLAY -u WAYLAND_DISPLAY timeout --kill-after=5 120 podman run --rm --cidfile "$session_dir/container.cid" --network=none \
    --security-opt label=disable --read-only --tmpfs /tmp:rw,size=256m --tmpfs /run:rw,size=16m \
    -v "$app_dir:/app:ro" -v "$report_dir:/out:rw" \
    -e "SLINT_SCALE_FACTOR=1" -e HOME=/tmp \
    -e VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
    -e LP_NUM_THREADS=4 \
    localhost/blockfield-main-headless sh -ec '
        unset WAYLAND_DISPLAY
        Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/out/xvfb.log 2>&1 &
        attempt=0
        until test -S /tmp/.X11-unix/X99; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 50 || { cat /out/xvfb.log; exit 1; }
            sleep 0.1
        done
        openbox --sm-disable >/out/openbox.log 2>&1 &
        attempt=0
        until xprop -root _NET_SUPPORTING_WM_CHECK >/dev/null 2>&1; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 100 || { cat /out/openbox.log; exit 1; }
            sleep 0.1
        done
        /app/target/release/blockfield-native --state settings-skin --self-check --idle-seconds 30 >/out/self-check.log 2>&1 &
        app_pid=$!
        attempt=0
        until grep -q "^READY pid=" /out/self-check.log; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 100 && kill -0 "$app_pid" || { cat /out/self-check.log; exit 1; }
            sleep 0.2
        done
        attempt=0
        until grep -q "SELF_CHECK passed" /out/self-check.log; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 150 && kill -0 "$app_pid" || { cat /out/self-check.log; exit 1; }
            sleep 0.2
        done
        window=$(xdotool search --pid "$app_pid" --name "BLOCKFIELD" | head -n 1)
        test -n "$window"
        xdotool windowminimize "$window"
        attempt=0
        until test "$(grep -c "PACING running=false" /out/self-check.log)" -ge 2; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 30 && kill -0 "$app_pid" || { cat /out/self-check.log; exit 1; }
            sleep 0.2
        done
        sleep 0.5
        xdotool windowmap "$window"
        window2=$(xdotool search --pid "$app_pid" --name "BLOCKFIELD" | head -n 1)
        test -n "$window2"
        attempt=0
        until test "$(grep -c "PACING running=true" /out/self-check.log)" -ge 3; do
            attempt=$((attempt + 1))
            test "$attempt" -lt 30 && kill -0 "$app_pid" || { cat /out/self-check.log; exit 1; }
            sleep 0.2
        done
        grep -q "SELF_CHECK passed" /out/self-check.log
        # The app exits on its own idle deadline; closing via synthetic keys
        # races window-manager reparenting, so just wait for that exit.
        wait "$app_pid"
    ' &
run_pid=$!
wait "$run_pid"
grep -q "SELF_CHECK passed" "$report_dir/self-check.log"
test "$(grep -c "PACING running=false" "$report_dir/self-check.log")" -ge 2
test "$(grep -c "PACING running=true" "$report_dir/self-check.log")" -ge 3
printf '%s\n' "PASS skin self-check: drag, hidden timer/frame/clock, minimize/restore resume"
