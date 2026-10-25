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

## Final shared-image A/B matrix

`Shared.Containerfile` extends the existing Fedora headless POC image with
WebKitGTK 4.1, so both release executables use exactly the same Mesa stack.
Build both executables before starting measurements. The Tauri executable must
be the capture-only build from `scripts/capture/build.sh` (fixture flag, local
fonts, no production network). The Slint executable is the standalone release
build in `native-poc/app`.

```sh
podman build -f scripts/bench/Shared.Containerfile -t localhost/blockfield-final-ab .
agent-process start --name final-ab --cwd "$PWD" -- timeout 4200 \
  python3 scripts/bench/final_ab.py --out /absolute/report/raw \
  --tauri .cache/capture/cargo-target/release/blockfield-launcher \
  --slint native-poc/app/target/release/blockfield-native-poc
python3 scripts/bench/final_report.py /absolute/report/raw /absolute/report/report.md
python3 scripts/bench/test_final_ab.py
```

Follow AGENTS.md process discovery/ownership rules before starting. The supervisor
clears host display variables; containers mount only binaries, harness code and a
disposable fixture directory, with no network or GPU devices. Every launch has
a 120 s deadline and exact-container cleanup. A TERM to the supervisor also cleans
its current container. Keep the outer deadline within the task budget.

The five scenarios are animated main, visible skin preview, minimized skin,
minimized main, and process-cold startup readiness. Each runs Tauri, Slint Skia GL
with partial rendering, and Slint Skia software, interleaved by repeat. Resource
runs use 5 s warmup and 3 × 30 s; startup uses five runs. Openbox's IconicState is
asserted for minimized runs; the Slint skin must deliver its first preview frame
before minimization and stop pacing afterward. Add `--smoke --duration 1 --warmup 2`
for a short matrix with visible screenshots. Screenshots are excluded from measured
runs. Inspect smoke evidence before the full matrix.

Before **each** launch, `uptime` and host load are recorded; load ≥2 waits up to
20 minutes and then marks that run NOISY. The report retains all loads, PSS/RSS/USS,
CPU% (per-core sum and divided by CPU count), threads, p10/median/p90 and spread.
Raw JSON includes samples, image metadata, binary hashes, exact commands and gate
history; `progress.md` is appended beside the raw directory after each scenario.
Startup timing excludes Xvfb/Openbox setup and uses existing app ready signals,
which are **not equivalent first-frame signals**. Host page caches are not flushed.
This headless llvmpipe result does not establish real-GPU performance or UI parity.

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

## Main-screen minimize/restore self-check

After building `native-poc/app`, use the existing shared image and an owned
process (no host display is mounted):

```sh
agent-process start --name main-idle-check --cwd "$PWD" -- timeout 180 \
  python3 scripts/bench/final_ab.py --out /absolute/report/self-check \
  --tauri /path/to/capture/blockfield-launcher \
  --slint native-poc/app/target/release/blockfield-native-poc \
  --scenario main --variant slint-gl --variant slint-sw --self-check
```

`--idle-self-check` checks the default online pulse, then the busy spinner.
Each cycle counts redraw events delivered to Slint, minimizes through winit,
allows 1 s to settle, asserts no redraws or active animations for a 5 s interval,
and restores redraw delivery. The spinner cycle disables the online pulse so
each animation is tested independently, on both renderers.
The app exits on success; the container requires its success marker and zero exit
status within 30 s. `--variant` can also select just the Slint variants for a
`--scenario skin-minimized --smoke --duration 30` regression run.

## Matched cross-platform CI

The `tauri-xplat` workflow builds both apps from the same commit, then interleaves
Tauri/Slint for main idle and animated settings-skin: three launches per app/state,
5 seconds warmup after readiness (including Slint skin first-frame delivery),
30 seconds sampling at 1 second intervals.
Screenshots run separately, with motion frozen; measured apps keep real animation.
Raw samples retain process PID/birth time/name, memory and cumulative CPU time.
The report gives per-run means, their mean/sample standard deviation and savings
only when all three matched pairs exist. CPU 100% means one fully occupied core.

`xplat_metrics.py` uses psutil's private resident working set (USS via
QueryWorkingSet) on Windows, including WebView2 descendants; Linux uses PSS.
macOS uses an explicitly labeled RSS fallback, **not phys_footprint**. WebKit XPC
services are attributed by the launch window: identities absent before launch
and named `com.apple.WebKit.*`, plus all ordinary app descendants. Runs are
sequential on disposable CI VMs; unrelated new WebKit services would contaminate
this fallback, and reused preexisting services could be missed. Before-launch
identities are excluded and must never be stopped. Shared RSS may be double
counted; CPU from helpers exiting between samples can be missed. These limits
must remain with the results. No GPU memory metric is inferred. Slint is pinned
to winit-software; skin adapter and Tauri WebGL renderer evidence stay in logs and
fixture proofs. Hosted Windows lacks a hardware GPU; this is not real-GPU data.

Animated Tauri runs also retain `motion-proof.json`: the fixture counts actual
WebGL indexed draw calls and writes one visibility/draw-count witness 25 seconds
after readiness. Skin runs must add more than 100 draw calls and stay visible;
missing proof fails the benchmark. This adds one native config write during the
measurement window. Windows disables WebView2 background/occlusion throttling
only in this runner harness, so a visible animated skin is compared with Slint's
visible animation, rather than a browser suspended by the CI desktop. These flags
are not included in normal app builds or environment. macOS XPC matching uses
process names only; reading arbitrary process command lines exposed a psutil C
extension SystemError on a transient unrelated process.
