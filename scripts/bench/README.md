# bench — whole-process-tree resource measurement

Dependency-free (Python 3 stdlib only) tool for the Tauri-vs-native GO/NO-GO
comparison (issue #59): it measures the **whole process tree** — WebKitGTK
spawns WebProcess/NetworkProcess children, so the main PID alone lies.

## Quick start

```sh
# Sample the tree of a running PID for 10 s
python3 scripts/bench/bench.py sample --pid 1234 --interval 0.5 --duration 10

# Launch a command, wait for readiness, measure 3 x 10 s windows
python3 scripts/bench/bench.py run --repeats 3 --duration 10 --warmup 2 \
  --ready-regex "READY" --out-dir /tmp/bench/tauri --name tauri-idle \
  -- ./target/release/blockfield-launcher

# Readiness via a file instead of a log line
python3 scripts/bench/bench.py run --ready-file /tmp/app.ready -- ...

# Tests (unit + synthetic-tree smoke, no GUI needed)
python3 scripts/bench/test_bench.py
```

`run` writes `<out-dir>/result.json` (config, machine info, git SHA, raw
samples per repeat) and `<out-dir>/summary.md` (per-repeat table + pooled
min/p10/median/p90/max). Processes that appear/disappear mid-run (WebKit
children re-spawning) are tolerated: each sample re-discovers the tree and
unreadable PIDs are skipped.

## What is measured

Per sample, per process and summed over the tree: PSS/RSS/USS from
`/proc/<pid>/smaps_rollup` (USS = Private_Clean + Private_Dirty), CPU% from
`/proc/<pid>/stat` ticks (percent of one CPU, `ps`-style; tree value is the
sum), thread count, process name. GPU memory sums `drm-memory-*` keys in
`/proc/<pid>/fdinfo/*`; without a GPU/driver exposing them the total is
`null` ("not measured"). Everything works without root; other users'
processes are skipped via their unreadable `/proc` entries.

## Owner run (final comparison, real desktop with GPU)

1. On the GPU machine, idle desktop, charger connected: run the same scenario
   for both builds with identical flags, e.g. `--repeats 5 --duration 20
   --warmup 5` and the app's real readiness signal.
2. Keep the builds' `git SHA` values from `result.json` for the report.
3. Compare the pooled `pss_kb` median (target: ≥30% tree-PSS reduction) and
   sustained `cpu_pct`; attach both `summary.md` files to the GO/NO-GO report.
4. Do not run other GPU apps concurrently; `fdinfo` accounting is per-process
   but driver-global pressure still skews results.
