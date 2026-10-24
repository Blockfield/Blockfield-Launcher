# Slint skin preview feasibility example

`src/slint-preview.slint` exports the reusable **SkinPreview** component
at 220×300 logical pixels. `PreviewWindow` is a small demonstration with a keyboard
accessible visibility toggle, separate from `native-poc/app`. This is a feasibility
example, not a port of the settings screen.

```sh
CARGO_BUILD_JOBS=4 cargo build --release --examples --features slint
# Use an isolated display, never the owner's desktop, for agent testing.
SLINT_BACKEND=winit-skia-opengl target/release/examples/slint
```

On the immutable Linux host, reuse the sibling POC's project-local runtime link
if the development symlink for libstdc++ is absent (no OS changes):

```sh
mkdir -p target/link-libs
ln -s /usr/lib64/libstdc++.so.6 target/link-libs/libstdc++.so
LIBRARY_PATH="$PWD/target/link-libs" CARGO_BUILD_JOBS=4 cargo build --release --examples --features slint
```

The optional feature uses Slint ~1.18 and the same winit, Skia OpenGL, software,
std and compatibility feature choices as `native-poc/app`. `unstable-winit-030`
is additionally required for native window lifecycle events; this API is explicitly
unstable upstream, so upgrades require checking the event hook. Default library
builds do not enable Slint. The exact Tauri capture fixture PNGs are embedded from
`src/lib/fixtures/{skin,cape}.png`, with `Model::Classic` matching `slim: false` in
`src/lib/fixtures/index.ts`.

## Ownership and pacing

- The UI thread owns all Slint objects, the frame deadline timer and the active
  animation clock. A worker owns wgpu, texture decoding, scene rendering and
  synchronous GPU readback. `SharedPixelBuffer<Rgba8Pixel>` crosses back through
  `invoke_from_event_loop`; `Image::from_rgba8` is created only on the UI thread.
  Straight alpha is preserved, following the renderer and Slint's contracts.
- At most one render/delivery is in flight. No frame backlog is accumulated.
  Absolute 60 Hz deadlines round up to Slint's whole-millisecond timer precision;
  missed deadlines are dropped. Slow renderers lower delivered FPS rather than
  making the UI wait or producing catch-up bursts. Each deadline also polls for
  deliveries: the invoke wakeup is a hint, but the timer (which only runs while
  visible) guarantees progress even when a posted user event is never dispatched.
- Hiding the preview stops its timer, freezes active animation time, cancels drag
  and invalidates outstanding delivery. The worker blocks in `recv()`. One render
  already executing may finish; its result is discarded. No hidden redraw or
  periodic worker polling is scheduled. A single exit deadline in benchmark mode
  and the self-check's explicit transition deadlines are not animation timers.
- Window visibility, zero-sized resize, winit occlusion and detectable minimization
  gate the same timer. Minimized state is also checked before submitting a frame
  because X11 does not always emit an occlusion event. Focus loss cancels dragging
  but does not hide a visible preview. Occlusion support depends on the platform;
  no promise is made for covered windows where winit cannot detect that state.
- `TouchArea` supplies pointer/touch capture and press/move/release. Both drag axes
  use logical deltas and the fixed 300px height, retaining the existing OrbitControls
  behavior and pausing only auto-rotation during dragging. DPI changes recreate
  the renderer with retained controls and the new physical pixel size.
- Render failures retain the last frame, show an error, stop the timer and return
  a nonzero process status at exit. There is no retry loop. Close stops the timer,
  sends a worker shutdown message, and joins the worker before returning.

For app wiring, use the `skin3d::slint::PreviewHost` library controller behind
the `slint` feature: point the app's Slint build at `src/slint-preview.slint`
for the `SkinPreview` component, feed `frame` from the host sink, forward
`drag-state`/`drag-by`, and call `set_tab_visible`/`refresh` on the same
visibility transitions the example uses. The pacing, worker and clock code lives
in the library; the example is a thin binary over it and keeps working.

Upstream contracts checked against Slint 1.18.1 source and documentation:
[Image / sending pixel buffers](https://docs.slint.dev/latest/docs/rust/slint/struct.Image),
[winit window accessor](https://docs.slint.dev/latest/docs/rust/slint/winit_030/trait.WinitWindowAccessor).

## Checks and isolated evidence

Run from `native-poc/skin3d` (or use `--manifest-path` from the checkout root):

```sh
cargo fmt --check
CARGO_BUILD_JOBS=4 cargo clippy --all-targets --features slint -- -D warnings
CARGO_BUILD_JOBS=4 cargo test --features slint
CARGO_BUILD_JOBS=4 cargo build --release --examples --features slint
```

The example's unit tests check active-time freeze/resume, idempotent visibility,
whole-millisecond deadline rounding and dropping missed frames. `--self-check`
adds a live Slint pointer-drag/hidden/resume assertion sequence and quits. A bounded
`--idle-seconds N` is available for harness runs. `--hidden` starts unmounted.

Build the disposable headless image; it extends the existing `blockfield-native-headless`
image from [the sibling POC](../app/Headless.Containerfile). No host packages or
device access are needed:

```sh
agent-process start --name skin-headless-image --cwd "$PWD" -- \
  timeout 300 podman build -t localhost/blockfield-skin-headless \
  -f examples/slint_preview/Headless.Containerfile .
agent-process start --name skin-headless-check --cwd "$PWD" -- \
  examples/slint_preview/headless.sh /absolute/report/self-check self-check
agent-process start --name skin-headless-capture --cwd "$PWD" -- \
  examples/slint_preview/headless.sh /absolute/report/capture capture 0
agent-process start --name skin-headless-bench --cwd "$PWD" -- \
  examples/slint_preview/headless.sh /absolute/report/bench bench
```

The harness uses a networkless, read-only rootless Podman container with its own
Xvfb :99, Openbox, GL llvmpipe and Vulkan lavapipe, with four Mesa worker threads.
It records a real window for 3 seconds at 60 fps, captures visible/dragged/hidden
PNGs, checks minimize/restore, then captures the actual Slint window at exact poses
`start_time + {0,1,2,3}`. The separate deterministic capture process uses delivered
frame index / 60 for pose time; it is not used for CPU measurements. Live recording
and benchmark mode use the active wall clock. Logs distinguish both modes.

Benchmark mode warms one process, then uses the repository's `scripts/bench/bench.py`
to sample the entire example process tree for **20 seconds each** while visible,
preview-hidden, and minimized, under the same renderer/container. JSON retains raw
samples and pooled median/p90. The compositor/Xvfb are outside the measured app tree;
GPU driver worker threads and their memory are included. This is not a whole-launcher
Tauri comparison or evidence of the phase-1 GO gate.

`--capture-dir PATH --start-time SECONDS` writes 4 RGBA PNGs at exact pose offsets
and exits after frame 180. The baseline recording has no per-pose time trace, so
its initial phase must be visually aligned and its uncertainty documented in the
external evidence report. Pixel-diff scoring, hardware GPUs, touch hardware,
Wayland, Windows, macOS and integration into `native-poc/app` remain later work.
