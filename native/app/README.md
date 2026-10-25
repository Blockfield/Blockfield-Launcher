# Native shell POC (u06a)

Standalone Cargo workspace for issue #59 phase 1. Slint 1.18.1, winit and Skia/OpenGL; the software renderer is also compiled for diagnostics. No WebView, network data, Tauri integration or production launcher changes.

The default window is frameless, resizable, initially/minimally 1280×720 logical pixels. `WindowMoveArea` implements native dragging without unstable winit access. The chrome uses native minimize/maximize/close APIs. Debug builds show the original DEV badges; `--dev` also enables them in a release build.

## Build

From `native/app` with the repository Rust 1.99.0 toolchain:

```sh
cargo fmt --check
CARGO_BUILD_JOBS=4 cargo clippy -- -D warnings
CARGO_BUILD_JOBS=4 cargo build --release --locked
```

Skia requires the platform C++ runtime plus fontconfig/freetype on Linux. On the owner's immutable OS, the runtime exists but its development symlink does not. The checked build used only this project-local linker workaround:

```sh
mkdir -p target/link-libs
ln -sf /usr/lib64/libstdc++.so.6 target/link-libs/libstdc++.so
LIBRARY_PATH="$PWD/target/link-libs" CARGO_BUILD_JOBS=4 cargo build --release --locked
```

No packages were installed into the base OS. Do not run a GUI on the owner's display for this POC.

## Headless evidence

From the repository root, first inspect existing processes according to AGENTS.md. Build the isolated capture image:

```sh
podman build -f native/app/Headless.Containerfile -t localhost/blockfield-main-headless native/app
```

Then start the bounded harness through the ownership helper (use an absolute output directory):

```sh
agent-process start --name native-shell-capture --cwd "$PWD" -- native/app/scripts/headless.sh /absolute/output/1x 1
```

Run `python3 native/app/scripts/verify_captures.py OUTPUT SCALE` once captures finish. The harness clears the host display variables and runs Xvfb `:99` inside a network-isolated, read-only Podman container with Mesa llvmpipe. It mounts only this POC, the output directory, and the existing benchmark script. X11 sockets, user D-Bus, browser sessions, devices and home directories are not mounted. The container exits after its exact app child completes; the outer deadline is 45 seconds. Use `agent-process status <id>` / `agent-process stop <id>` for a current-session process that remains live. The harness closes only its container.

Scale arguments `1.25` and `1.5` produce 1600×900 and 1920×1080 PNGs with the same 1280×720 logical layout. `capture.rs` asserts dimensions, mouse activation of all four nav items, home/logo, footer update routing, Tab/Return activation and actual close-button dispatch. It captures normal, hover, active, focus, DEV/backdrop/pulse and close press states using the renderer's `take_snapshot`; transient PPM files are converted to PNG and deleted.

```sh
python3 native/app/scripts/verify_captures.py /absolute/output/1x 1
agent-process start --name native-shell-idle --cwd "$PWD" -- native/app/scripts/headless.sh /absolute/output/idle 1 idle
```

Idle mode samples the **release app PID/tree**, using the unchanged `scripts/bench/bench.py sample --duration 10 --interval 0.5` after a two-second warmup. Xvfb and container overhead are excluded; Skia/driver memory mapped into the app is included. This empty-shell measurement is not a Tauri comparison or evidence of migration savings.

## Next unit

- `src/fixtures.rs` owns all dynamic shell fixture values. Defaults come from `App.tsx:342-343`, `constants.ts`, `i18n.tsx`, `Shell.tsx` and `src-tauri/tauri.conf.json:4`; username is the source's missing-account fallback `—`, not a real account.
- `ui/app.slint` composes `WindowChrome` and `Shell`. Add MainScreen as a child of Shell, in the currently empty main content slot. Capture-only `show-backdrop` / `show-pulse` properties exercise primitives without adding decoration to the real empty Shell.
- `ui/controls.slint`: ActionArea, ChromeButton, NavItem, StatusDot (150ms color transitions, genuine 2s pulse).
- `ui/backdrop.slint`: GridBackdrop, TopoBackdrop, CornerTicks, GlowPanel. These do not animate in the React source. CSS farthest-corner radial geometry is explicit.
- `ui/tokens.slint` / `TOKENS.md`: single token global and source-line mapping.
- `FONTS.md` / `assets/fonts`: exact Google-served TTF bytes, versions, source CSS, checksums and OFL licenses. Fonts and SVGs are embedded in the executable.
- Original logo/player-head geometry is copied from React assets. Lucide SVGs are extracted from the exact `lucide-react@1.52.0` package and retain its ISC license.

## Parity gaps / validation limits

- No canonical Tauri image/frame diff was available to this unit. Geometry/state assertions and direct render inspection pass; pixel-perfect text rasterization, fractional-DPI antialiasing, shadows and animation curves still need paired baseline comparison. Google may serve Chromium WOFF2 transports; the bundled files are its unmodified TTF transports from the same CSS URL.
- CSS gradients can interpolate in Oklab (Tailwind 4), while Slint's brush uses its renderer's interpolation. The very dark GlowPanel gradient may have small color differences. Slint's text layout and shadow rasterization can also differ from Chromium.
- The outer CSS window shadow has negative spread and an inset highlight. Slint lacks the same spread/inset model; the original window border/radius are reproduced, while no compositor shadow outside the native window surface is synthesized. Validate against the actual Tauri native capture, where outer shadows may be clipped too.
- Nav/chrome focus uses an explicit amber keyboard-only ring; React's nav/chrome buttons also inherit browser focus styling. Exact browser-default focus pixels need the paired capture.
- Footer Discord is a callback stub that logs the fixture URL and does not open a browser. The launcher-update callback selects Settings; updater content and all MainScreen/backend behavior belong to later units. Avatar is the original fallback asset; there is no remote face lookup.
- Xvfb has no window manager. Native min/max/drag are wired through Slint's platform APIs but real window-manager behavior, Wayland, Windows, macOS, accessibility/screen readers and long CMS overrides are unverified.

The port preserved the incumbent design. Impeccable context/craft guidance was read; native code skips the HTML/CSS detector. The required no-delegation brief meant the finishing inspection ran in-thread. No design-system files outside this workspace were changed.

## Checked build (2026-10-24)

Release binary: 30,006,616 bytes (28.62 MiB), SHA-256 `1a34eabc4e027bcbd470d8645b19f9dce8e9feadedd100c447ee3a3c57c60895`.
Ten-second idle sample (2s warmup, 0.5s interval, 21 samples): peak app-tree RSS 146,808 KiB (143.37 MiB), PSS 132,969 KiB (129.85 MiB); sampled CPU max 0.0%, 28 threads.
Renderer: Slint 1.18.1 winit + Skia/OpenGL, Mesa llvmpipe in private Fedora 44 Podman/Xvfb. No FemtoVG fallback.

All 24 release captures and mouse/keyboard/close/pulse/font checks passed. The pulse test checks phase ordering (early bright frame > return-half dim frame), including a false→true→false toggle on the same StatusDot. Lazy binding initialization is forced before its first animation; stopping pulse restores full opacity immediately.

Evidence: `.dev/coord-20261024/plans/native-migration/reports/poc-shell/` in the coordinator checkout (`progress.md`, `review.md`, `1x/`, `1.25x/`, `1.5x/`, `idle/idle.json`). Required Rust formatting, Clippy and release build passed with four build jobs; shell syntax/Python compile and whitespace checks also passed. The repository pre-commit hook additionally passed root Tauri/shared formatting, Clippy and workspace tests (the existing public-release-download test is ignored). No task-owned app/container/watcher remains running.

## MainScreen fixture port (u06b)

`ui/main.slint` adds the MainScreen within Shell's content slot. Run `--state main` (the executable default), `main-launching`, `offline`, or `error`; these names and values match `origin/native/capture` commit `cb05a069cb2b41979e0d4dbad8a061e85ec8900c`. `BLOCKFIELD_CAPTURE_STATE` is the environment equivalent. `--state shell` retains u06a's empty-shell harness and assertions. Unknown state/scenario names fail closed.

The exact source fixture is retained at `assets/fixtures/tauri-data.ts`, with its `skin.png` used for the same base-face/hat texture crop as `player-face.ts`. Canonical values: CaptureOperator, role player, game profile; pack 2.75 installed/latest, 1.0 GB, Java/loader ready; online 24/100, 32 ms, EU/Frankfurt, play.blockfield.pro:25565; ALPHA (12 players, 8/12 ready, Casual/room/Citadel/waiting) and BRAVO (12 players, Ranked/campaign/Crossroads/running, queue admission). Content is the React fallback because capture returns `{}` for content.json. Launcher update: 1.0.20. Offline keeps EU and max 100 but clears rooms, players and ping; error leaves the pack unchecked with `Capture fixture: manifest unavailable`.

The fixture player's role hides profile selection in React, so canonical Slint captures hide it too. Explicit `--scenario builder` or `workshop` demonstrates the permitted profile selector without altering the canonical data. Further diagnostics: `checking`, `busy` (local launching with 42% text), `running`, `finishing`, `profile-changing`, `profile-blocked`, `profile-error`, `empty`, `unavailable`, `maintenance`, `updating`, `update`, `setup`, `retry`, `closed`. These diagnostic variations are POC-only and are not Tauri capture states.

MainScreen's launch percentage is text in the button subtitle; the gradient progress-width animation belongs to UpdateScreen, which is outside this unit. Likewise, `main-launching` sets **game phase** launching but leaves MainScreen's **local** launching flag false, exactly as the capture fixture does: its deploy is disabled, with no LoaderCircle. `busy` exercises the local-launch spinner. Status pulses reuse the existing 2s ease-in-out implementation; LoaderCircle rotates continuously over 1s with linear timing; colors, deploy opacity/border and select chevrons use 150ms cubic-bezier(.4,0,.2,1). The original React Lucide paths are bundled; Play's fill is enabled as in React.

Fixture actions stay inside this executable: deploy changes the fixture launch state; verify shows a 1s check; joining shows the source invitation message then starts the fixture launch. They never touch game files, accounts, a network or a browser. Update/setup actions show an explicit POC message because UpdateScreen is not part of this port.

### Reproduce MainScreen evidence

After checking existing processes/ownership, use the same isolated harness:

```sh
agent-process start --name native-main-capture --cwd "$PWD" -- native/app/scripts/headless.sh /absolute/report/main/1x 1 capture main
python3 native/app/scripts/verify_main.py /absolute/report/main/1x 1
agent-process start --name native-main-motion --cwd "$PWD" -- native/app/scripts/headless.sh /absolute/report/main/motion 1 record main
agent-process start --name native-main-idle --cwd "$PWD" -- native/app/scripts/headless.sh /absolute/report/main/idle 1 idle main
```

Repeat capture for each canonical state and scale 1, 1.25 and 1.5. A sixth argument is not required: the optional fifth argument is the diagnostic scenario. MainScreen still capture mode adds `--freeze-motion`, matching Tauri’s paused CSS animations and disabled transitions; interactive runs, shell regression tests, recordings and idle runs animate normally. Capture scripts assert room paging, tabs, keyboard select, verify and deploy/disabled behavior through actual dispatched input. Screenshots after `main.png` intentionally show interaction states, not independent canonical fixtures.

After a 2s renderer warmup, container ffmpeg records the private Xvfb display at 60fps for 3s into lossless FFV1; the host ffmpeg encodes H.264 while preserving input time. `frame-times.csv` retains the actual source frame timestamps. This avoids `take_snapshot`/PPM readback in the app UI thread, which reached only 11–15 snapshots/s during an initial diagnostic and cannot validate 60fps motion. Mouse capture is disabled. No host display socket, browser or audio source is used. The temporary lossless recording is deleted after encoding.

The container image adds Fedora's `ffmpeg-free` only inside its isolated filesystem. The renderer remains Skia/OpenGL on private Xvfb/Mesa llvmpipe. The unchanged existing `scripts/bench/bench.py` measures the idle app PID/tree. The harness has a private container cidfile and exact-container cleanup on normal exit, failure, deadline or task cancellation.

### MainScreen parity gaps / boundaries

- Xvfb capture requests 60fps, but source grabs can drop under CPU/software-GPU load. Output is exactly 3s/180 frames at 60fps, preserving source timestamps and holding frames where needed; source timing CSVs are retained. This does not prove sustained 60fps native rendering.
- Paired pixel/frame diffs are the next unit; this port does not assert pixel-perfect parity. Slint and Chromium text shaping, baseline metrics, shadow/gradient interpolation and antialiasing need that comparison. The inherited negative-spread/inset glow limitations still apply to deploy.
- Fixture rooms have no battle points; battle-score/point-pagination UI is not included. Only two fixture rooms/options are used; long arbitrary lists and custom scrollbars are not covered.
- Native title tooltips, browser-default disclosure marker pixels, screen-reader behavior and long CMS strings remain unverified. Error details and select keyboard focus are explicit native controls.
- Diagnostic states demonstrate Workshop and maintenance with explicitly labeled synthetic values. They are not additional baseline fixtures. There is no backend persistence, real game launch/profile authorization, or UpdateScreen implementation in this isolated POC.
- Window-manager behavior, Wayland, Windows and macOS remain outside the Linux headless check. No user's display/browser is used.
