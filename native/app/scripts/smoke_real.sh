#!/bin/sh
# Real-data smoke for the native Slint launcher: boots the REAL binary (no
# fixture/capture flags) against the live public endpoints with a throwaway
# profile, captures one PNG per scale plus a UI-text snapshot (--smoke-text),
# and leaves assertion to verify_smoke.py. Signed-out screens only: with no
# credentials the app lands on login (or firstrun/init-error); there is no
# fake/local session without real tokens, so signed-in screens are out of
# scope and show_main must stay off.
set -eu

out=${1:?Usage: smoke_real.sh OUT_DIR [SCALE] [BINARY]}
scale=${2:-1}
bin=${3:-./target/ci-test/blockfield-native}
case "$out" in /*) ;; *)
    echo 'OUT_DIR must be absolute' >&2
    exit 2
    ;;
esac
case "$scale" in 1 | 1.5) ;; *)
    echo 'SCALE must be 1 or 1.5' >&2
    exit 2
    ;;
esac

mkdir -p "$out"
out=$(CDPATH= cd -- "$out" && pwd)
bin_dir=$(CDPATH= cd -- "$(dirname -- "$bin")" && pwd)
bin="$bin_dir/$(basename -- "$bin")"
test -x "$bin" || {
    echo "binary not found: $bin" >&2
    exit 2
}

profile=$(mktemp -d "$out/.profile.XXXXXX")
app_pid=""
xvfb_pid=""
cleanup() {
    if [ -n "$app_pid" ]; then
        kill "$app_pid" 2>/dev/null || true
        wait "$app_pid" 2>/dev/null || true
    fi
    if [ -n "$xvfb_pid" ]; then
        kill "$xvfb_pid" 2>/dev/null || true
        wait "$xvfb_pid" 2>/dev/null || true
    fi
    rm -rf "$profile"
}
trap cleanup EXIT INT TERM

# Private display owned by this run (never the owner session).
display=""
for candidate in $(awk 'BEGIN { for (i = 100; i < 150; i++) print i }'); do
    if [ ! -e /tmp/.X11-unix/X"$candidate" ] && [ ! -e /tmp/.X"$candidate"-lock ]; then
        display=":$candidate"
        break
    fi
done
[ -n "$display" ] || {
    echo 'no free X display' >&2
    exit 1
}
Xvfb "$display" -screen 0 2400x1500x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb_pid=$!
export DISPLAY="$display"
attempt=0
until xdotool search --onlyvisible --maxdepth 0 --classname '.*' >/dev/null 2>&1 || xdpyinfo -display "$display" >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    [ "$attempt" -lt 50 ] || {
        echo 'Xvfb did not start; see xvfb.log' >&2
        exit 1
    }
    sleep 0.1
done

export SLINT_SCALE_FACTOR="$scale"
# Throwaway profile: the run must never touch the owner launcher data.
export HOME="$profile/home" XDG_DATA_HOME="$profile/data" XDG_CONFIG_HOME="$profile/config" \
    XDG_CACHE_HOME="$profile/cache" XDG_RUNTIME_DIR="$profile/run" TMPDIR="$profile/tmp"
mkdir -p "$HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR" "$TMPDIR"
# Isolated session bus so the single-instance claim cannot see (or disturb)
# any launcher owned by another session; a claim failure continues alone.
unset DBUS_SESSION_BUS_ADDRESS
export DBUS_SESSION_BUS_ADDRESS="unix:path=$profile/bus"

start=$(date +%s)
"$bin" --idle-seconds 35 --smoke-text "$out/ui-text.txt" >"$out/app.log" 2>&1 &
app_pid=$!
if ! win=$(timeout 60 xdotool search --sync --onlyvisible --name 'BLOCKFIELD'); then
    echo "window did not open within 60s" | tee "$out/FAIL" >&2
    exit 1
fi
opened=$(date +%s)
echo "window_open_s=$((opened - start))" >"$out/timing.txt"
echo "window_id=$win scale=$scale" >>"$out/timing.txt"
sleep 8
import -display "$display" -window "$win" "$out/screen-$scale.png"
wait "$app_pid"
app_pid=""
test -f "$out/ui-text.txt" || {
    echo 'no ui-text snapshot (binary exited early?)' >&2
    exit 1
}
echo "smoke run complete: scale=$scale"
