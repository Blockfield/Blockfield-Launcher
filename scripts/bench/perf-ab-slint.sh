#!/bin/sh
# A/B sampler for the Slint POC: private Xvfb + llvmpipe, animated --state main,
# whole-process-tree sampling via scripts/bench/bench.py inside the container.
# Usage: perf-ab-slint.sh OUTDIR TAG REPEATS DUR WARMUP
# Renderer selection via SLINT_BACKEND / SLINT_SKIA_PARTIAL_RENDERING env (exported).
set -eu
app_dir=$(
    CDPATH=
    cd -- "$(dirname -- "$0")/../../native-poc/app" && pwd
)
bench_py=$(
    CDPATH=
    cd -- "$(dirname -- "$0")" && pwd
)/bench.py
outdir=${1:?OUTDIR}
tag=${2:?TAG}
repeats=${3:-3}
dur=${4:-30}
warmup=${5:-5}
mkdir -p "$outdir"
total=$((warmup + dur + 4))
i=0
while test "$i" -lt "$repeats"; do
    echo "uptime before $tag repeat $i: $(uptime)" | tee -a "$outdir/uptime.log"
    session_dir=$(mktemp -d "$outdir/.session.XXXXXX")
    # shellcheck disable=SC2064
    trap "rm -rf '$session_dir'" EXIT
    extra_env=""
    if test -n "${SLINT_SKIA_PARTIAL_RENDERING:-}"; then
        extra_env="-e SLINT_SKIA_PARTIAL_RENDERING=${SLINT_SKIA_PARTIAL_RENDERING}"
    fi
    if test -n "${SLINT_DEBUG_PERFORMANCE:-}"; then
        extra_env="$extra_env -e SLINT_DEBUG_PERFORMANCE=${SLINT_DEBUG_PERFORMANCE}"
    fi
    env -u DISPLAY -u WAYLAND_DISPLAY timeout --kill-after=5 120 podman run --rm \
        --cidfile "$session_dir/container.cid" --network=none \
        --security-opt label=disable --read-only --tmpfs /tmp:rw,size=256m --tmpfs /run:rw,size=16m \
        -v "$app_dir:/app:ro" -v "$outdir:/out:rw" -v "$bench_py:/bench.py:ro" \
        -e "SLINT_BACKEND=${SLINT_BACKEND:-winit-skia-opengl}" \
        $extra_env \
        -e SLINT_SCALE_FACTOR=1 -e HOME=/tmp \
        localhost/blockfield-main-headless sh -ec "
      unset WAYLAND_DISPLAY
      Xvfb :99 -screen 0 1280x720x24 -nolisten tcp >/out/$tag-xvfb.log 2>&1 &
      attempt=0
      until test -S /tmp/.X11-unix/X99; do
        attempt=\$((attempt + 1))
        test \"\$attempt\" -lt 50 || { cat /out/$tag-xvfb.log; exit 1; }
        sleep 0.1
      done
      DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe WINIT_UNIX_BACKEND=x11 \
        /app/target/release/blockfield-native-poc --state main --idle-seconds $total >/out/$tag-repeat$i.log 2>&1 &
      app_pid=\$!
      sleep $warmup
      DISPLAY=:99 python3 /bench.py sample --pid \"\$app_pid\" --duration $dur --interval 0.5 --json-out /out/$tag-repeat$i.json
      wait \"\$app_pid\"
      glxinfo -B > /out/$tag-glxinfo.txt 2>/dev/null || true
    "
    rm -rf "$session_dir"
    trap - EXIT
    i=$((i + 1))
done
printf '%s\n' "DONE $tag x$repeats ${dur}s"
