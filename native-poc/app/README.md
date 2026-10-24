# Native shell POC (u06a)

Standalone Cargo workspace for issue #59 phase 1. Slint 1.18.1, winit and Skia/OpenGL; the software renderer is also compiled for diagnostics. No WebView, network data, Tauri integration or production launcher changes.

The default window is frameless, resizable, initially/minimally 1280×720 logical pixels. `WindowMoveArea` implements native dragging without unstable winit access. The chrome uses native minimize/maximize/close APIs. Debug builds show the original DEV badges; `--dev` also enables them in a release build.

## Build

From `native-poc/app` with the repository Rust 1.99.0 toolchain:

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
podman build -f native-poc/app/Headless.Containerfile -t localhost/blockfield-native-headless native-poc/app
```

Then start the bounded harness through the ownership helper (use an absolute output directory):

```sh
agent-process start --name native-shell-capture --cwd "$PWD" -- native-poc/app/scripts/headless.sh /absolute/output/1x 1
```

Run `python3 native-poc/app/scripts/verify_captures.py OUTPUT SCALE` once captures finish. The harness clears the host display variables and runs Xvfb `:99` inside a network-isolated, read-only Podman container with Mesa llvmpipe. It mounts only this POC, the output directory, and the existing benchmark script. X11 sockets, user D-Bus, browser sessions, devices and home directories are not mounted. The container exits after its exact app child completes; the outer deadline is 45 seconds. Use `agent-process status <id>` / `agent-process stop <id>` for a current-session process that remains live. The harness closes only its container.

Scale arguments `1.25` and `1.5` produce 1600×900 and 1920×1080 PNGs with the same 1280×720 logical layout. `capture.rs` asserts dimensions, mouse activation of all four nav items, home/logo, footer update routing, Tab/Return activation and actual close-button dispatch. It captures normal, hover, active, focus, DEV/backdrop/pulse and close press states using the renderer's `take_snapshot`; transient PPM files are converted to PNG and deleted.

```sh
python3 native-poc/app/scripts/verify_captures.py /absolute/output/1x 1
agent-process start --name native-shell-idle --cwd "$PWD" -- native-poc/app/scripts/headless.sh /absolute/output/idle 1 idle
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
