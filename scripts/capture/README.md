# Linux release capture

This harness captures the existing Tauri/WebKitGTK UI in a rootless Podman container,
using private Xvfb `:97`, Mesa llvmpipe, and FFmpeg X11 capture. It never attaches to
the owner's X11/Wayland display. No user configuration, accounts, game directories,
Discord sockets, GPU devices, signing secrets, or production feeds are mounted.
Runtime networking is disabled. All runtime processes belong to the registered
`agent-process` container supervisor and are stopped after each capture.

## Build once

Requirements on the host: Podman, Node/pnpm, Python 3, `agent-process`, Git. Build
`localhost/blockfield-tauri-builder:latest` using `scripts/tauri-builder.Containerfile`
if it does not already exist. The capture image adds dependencies inside the
container only:

```sh
podman build -t localhost/blockfield-capture:latest \
    -f scripts/capture/Containerfile scripts/capture
agent-process start --name blockfield-capture-build --cwd "$PWD" \
    -- bash scripts/capture/build.sh
```

Inspect the returned exact process ID with `agent-process status ID` and its log.
The one release build uses `CARGO_BUILD_JOBS=4`. Artifacts stay in `.cache/capture/`.
`build.sh` stages tracked source plus the owned fixture files, excluding `.env`
and unrelated untracked files. It changes font CSS and adds WebKit zoom permission
**only in that staged capture source**. It builds with Tauri `custom-protocol` and
`VITE_CAPTURE_FIXTURES=1`; do not distribute this test executable.

The capture font stylesheet uses the same Google Fonts request as production
`src/styles/fonts.css`. `prepare-fonts.py` downloads all requested Inter and IBM Plex
Mono weights into the capture cache and records resource URLs and SHA-256 hashes.
The staged CSS uses local font URLs, so runtime font loading requires no network.
Each capture waits for `document.fonts.ready`, checks loaded Inter FontFace entries
and `document.fonts.check`, and fails if Inter is unavailable. The manifest records
loaded faces, computed font-family lists, and font-source hashes. The shipped
monospace stack remains the one defined by the existing UI; loading IBM Plex Mono
does not replace that stack.

## Capture

```sh
scripts/capture/run.sh main --size 1280x720 --scale 1 --lang en --out /tmp/baseline
scripts/capture/run.sh settings-skin --size 1600x900 --scale 1.5 --lang ru --out /tmp/baseline
scripts/capture/run.sh main-launching --record 3 --fps 60 --out /tmp/baseline
scripts/capture/run.sh settings-skin --record 3 --fps 60 --out /tmp/baseline
```

`--size` is the logical viewport, at least 1280×720. `--scale` supports 1, 1.25,
1.5 using actual WebKit webview zoom; Xvfb output is logical size times scale.
This exercises WebKit rasterization and responsive layout, not a physical monitor's
GTK/Wayland fractional DPI implementation. PNGs exclude the pointer. Recordings
use lossless H.264 4:4:4 at 60 capture frames/second, retaining duplicate frames when
the software renderer runs slower. They do not assert 60 distinct rendered frames
per second. Recordings preserve the existing CSS and skin walking/rotation animation.
Static captures pause CSS animations and transitions, fix JS wall/animation clocks,
and hide the caret. Motion capture preserves the animation clock but fixes wall time.

Available states: `first-run`, `login`, `main`, `main-launching`, `update-42`, `stats`,
`settings-general`, `settings-runtime`, `settings-commands`, `settings-skin`,
`settings-launcher`, `offline`, `error`.
The launcher-update available state is visible throughout the shell and in
`settings-launcher`. Skin and cape are synthetic bundled UV test PNGs, never a
real player's assets. Navigation invokes the existing UI buttons; the fixture
does not substitute components or artwork for UI or animation.

The current source supports only Russian (`Lang = 'ru'`, fixed App context).
`--lang en/uk` records the requested language and `effectiveLanguage: ru` without
inventing translations. EN/UK localization parity is **not covered** by this baseline.

Each call writes PNG, optional MP4, per-capture JSON, app/display logs, and appends
to `manifest.json`. Run sequentially into each output directory. Existing capture
names are not overwritten. The manifest includes build/source/binary fingerprints,
state, logical/physical dimensions, zoom, requested/effective language, font proof,
WebGL/GLX renderer, and JS errors. The runtime exchanges selection and readiness
through a disposable native config file: one real `load_settings` before the IPC
mock is installed and one real `save_settings` afterward. Application commands are
mocked and unexpected commands fail closed; native window controls remain native.
A URL query (`?state=main&lang=ru&scale=1&motion=0`) can select the fixture when the
capture build is embedded in another isolated Tauri harness.

The WebKit subprocess sandbox is disabled only inside this private rootless,
networkless container because nested sandbox user namespaces are unavailable on
some hosts. Container isolation remains enabled; no privilege, host namespaces,
Docker socket, host-system binds, or desktop sockets are used.

## Extend and verify

Add the name/data to `src/lib/fixtures/data.ts` and its navigation/readiness condition
to `index.ts`, then mirror the name in `run.py`. Reuse actual components and IPC
contracts. Unexpected IPC/HTTP requests should fail instead of falling through to
production. Add a small fixture-data assertion when adding nontrivial behavior.

```sh
pnpm test
pnpm exec tsc --noEmit
pnpm exec eslint src/main.tsx src/lib/fixtures/*.ts
python3 -m unittest discover -s scripts/capture -p 'test_*.py'
pnpm build
# Must produce no matches; fixture chunks/textures must also be absent.
rg 'Capture fixture|CaptureOperator|capture-ready:|VITE_CAPTURE_FIXTURES' dist
scripts/capture/run.sh main --out /tmp/repeat-a
scripts/capture/run.sh main --out /tmp/repeat-b
magick compare -metric AE /tmp/repeat-a/main-1280x720@1x-en.png \
    /tmp/repeat-b/main-1280x720@1x-en.png null:
```

Known nondeterminism: software rendering/frame pacing, system font rasterizer
versions, and startup timing in motion recordings. Compare static captures only
with matching image/font hashes and renderer. Compare motion frame sequences with
time alignment; recordings do not freeze or replace the animation.
