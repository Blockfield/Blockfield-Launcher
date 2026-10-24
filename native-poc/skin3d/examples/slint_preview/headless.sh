#!/bin/sh
set -eu
crate=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
out=${1:?Usage: headless.sh REPORT_DIR [capture|bench|self-check] [START_TIME]}
mode=${2:-capture}
phase=${3:-0}
case "$mode" in capture|bench|self-check) ;; *) exit 2 ;; esac
mkdir -p "$out"
out=$(CDPATH= cd -- "$out" && pwd)
exec env -u DISPLAY -u WAYLAND_DISPLAY timeout 160 podman run --rm --network=none \
  --security-opt label=disable --read-only --tmpfs /tmp:rw,size=256m --tmpfs /run:rw,size=16m \
  -v "$crate:/app:ro" -v "$out:/out:rw" \
  -v "$crate/../../scripts/bench/bench.py:/bench.py:ro" \
  -e VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json -e HOME=/tmp -e XDG_RUNTIME_DIR=/tmp/runtime -e "PREVIEW_MODE=$mode" -e "PREVIEW_PHASE=$phase" \
  localhost/blockfield-skin-headless python3 /app/examples/slint_preview/headless.py
