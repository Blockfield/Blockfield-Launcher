#!/bin/sh
set -eu
app_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=${1:?Usage: capture_main.sh ABSOLUTE_REPORT_DIRECTORY}
case "$out" in /*) ;; *) echo 'Report path must be absolute' >&2; exit 2 ;; esac
for scale in 1 1.25 1.5; do
    for state in main main-launching offline error; do
        "$app_dir/scripts/headless.sh" "$out/$state/${scale}x" "$scale" capture "$state"
        python3 "$app_dir/scripts/verify_main.py" "$out/$state/${scale}x" "$scale"
    done
    "$app_dir/scripts/headless.sh" "$out/shell/${scale}x" "$scale" capture shell
    python3 "$app_dir/scripts/verify_captures.py" "$out/shell/${scale}x" "$scale"
done
for scenario in builder workshop checking busy running finishing profile-changing profile-blocked profile-error empty unavailable maintenance updating update setup retry closed; do
    "$app_dir/scripts/headless.sh" "$out/scenarios/$scenario" 1 capture main "$scenario"
    python3 "$app_dir/scripts/verify_main.py" "$out/scenarios/$scenario" 1
done
for state in main main-launching error; do
    "$app_dir/scripts/headless.sh" "$out/$state/motion" 1 record "$state"
done
for scenario in checking busy; do
    "$app_dir/scripts/headless.sh" "$out/scenarios/$scenario/motion" 1 record main "$scenario"
done
"$app_dir/scripts/headless.sh" "$out/idle" 1 idle main
printf '%s\n' 'PASS complete MainScreen capture matrix, shell regressions, five motion recordings and idle sample'
