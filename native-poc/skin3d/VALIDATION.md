# u07 validation — 2026-10-24

Implemented on branch `native/skin3d`, based on `feat/native-slint` at 31938ca.
Only `native-poc/skin3d/**` is changed by this unit. The root Cargo workspace and
shipped Tauri UI are untouched. No PR was requested or opened.

## Checks

All checks ran with Rust/Cargo 1.99.0 and `CARGO_BUILD_JOBS=4`:

- `cargo fmt --check`: passed.
- `cargo clippy -- -D warnings`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test -- --include-ignored`: 12 CPU unit tests + 1 software GPU integration test passed; no ignored tests in this run.
- `cargo build --release --examples`: passed (thin LTO).
- Reference math: 468 upstream-derived vertices (classic/slim, skin/cape, three times, all six faces), positions/normals/UVs within 1e-5.
- Frame dump: all nine fixture/scale combinations produced exactly 121 PNGs each (1,089 total), t=0..2 seconds at 60 fps.
- Byte-identical frame at t=0 before/after each 121-frame sequence: passed in all nine runs.
- Inspected rendered classic t=0,1,2, slim t=1 and legacy t=1: model, inflated alpha layers, cape, walking/rotation visible; no clipping or obvious missing geometry. This is not a visual diff against skinview3d.

Commands used `--manifest-path native-poc/skin3d/Cargo.toml` from the checkout root.
The harness had a cargo/rustup proxy-name error when tools were launched with the
crate as working directory; the manifest-path invocation used the same pinned
compiler successfully and required no toolchain or system changes.

## Headless adapter and timing

Host: AMD Ryzen 5 7535U, 12 logical CPUs; Linux 7.2.8-200.fc44.x86_64.
Adapter: **llvmpipe (LLVM 22.1.8, 256 bits)**, Mesa 26.2.3, Vulkan, CPU device.
Environment: DISPLAY/WAYLAND_DISPLAY unset; `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json`; `LP_NUM_THREADS=4`.
No display server, GUI app, container, OS package install or CPU rasterizer fallback was needed.

Each run excludes one warm-up frame. Timing includes CPU pose/mesh construction,
GPU scene + FXAA, synchronized readback, row unpacking and straight-alpha conversion.
PNG encoding and file writes are excluded. These are POC timings, not a Tauri/Slint
resource comparison or a GO decision.

| Fixture | Scale / pixels  | Mean ms | Median ms | p95 ms | Max ms |
| ------- | --------------- | ------: | --------: | -----: | -----: |
| classic | 1.00× / 220x300 |   1.130 |     1.116 |  1.286 |  1.592 |
| classic | 1.25× / 275x375 |   1.530 |     1.501 |  1.764 |  2.263 |
| classic | 1.50× / 330x450 |   2.100 |     2.076 |  2.442 |  2.845 |
| slim    | 1.00× / 220x300 |   1.118 |     1.108 |  1.273 |  1.415 |
| slim    | 1.25× / 275x375 |   1.513 |     1.482 |  1.787 |  1.949 |
| slim    | 1.50× / 330x450 |   2.100 |     2.026 |  2.573 |  2.949 |
| legacy  | 1.00× / 220x300 |   1.200 |     1.184 |  1.384 |  1.735 |
| legacy  | 1.25× / 275x375 |   1.453 |     1.417 |  1.734 |  2.992 |
| legacy  | 1.50× / 330x450 |   1.906 |     1.872 |  2.187 |  2.434 |

## Evidence and remaining work

External evidence directory:
`/var/mnt/projects/Tauri/Blockfield Launcher/.dev/coord-20261024/plans/native-migration/reports/skin3d/`. It contains the nine frame directories and per-run `timings.txt`, `verification.log`, `render-frames.log` and `progress.md`. PNG evidence is intentionally outside git; fixture inputs, upstream vertex reference and reproduction example are committed.

Remaining: browser-recording pixel/animation alignment comparison, Metal/DX12 and
hardware-adapter checks, Slint image upload/thread scheduling, UI pointer capture
and touch events, hidden/minimized timer enforcement and device-loss integration.
No promise of byte-identical cross-driver pixels: JS f64 vs GPU/Rust f32, canvas
alpha rounding, depth bias/rasterization and transparent-sort ties need visual QA.
No known deliberate omissions from the requested player/cape geometry or walking
equations; unused skinview3d ears/elytra/name tags/other animations are outside scope.
Synchronous readback must run off the UI thread. Dropping requests while hidden
is a documented caller contract, not yet proven in a live Slint app.
