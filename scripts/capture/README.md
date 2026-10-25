# Linux release capture

Captures the real Tauri 2 release executable in rootless Podman, private Xvfb `:97`,
WebKitGTK, and Mesa llvmpipe. The runtime has no network, owner display sockets,
GPU devices, real accounts/game data, or signing credentials. Temporary home/data
folders are discarded. Every container is owned by an `agent-process` supervisor.

## Build and validate

Host requirements: Podman, Node/pnpm, Python 3, Git, `agent-process`, and ImageMagick.
Build `localhost/blockfield-tauri-builder:latest` with the existing
`scripts/tauri-builder.Containerfile` if needed. Add capture tools inside the image:

```sh
podman build -t localhost/blockfield-capture:latest \
    -f scripts/capture/Containerfile scripts/capture
agent-process start --name blockfield-capture-build --cwd "$PWD" \
    -- bash scripts/capture/build.sh
```

Inspect the returned exact process ID/log. Do not edit `build.sh` while it runs.
The release build uses `CARGO_BUILD_JOBS=4`, cached dependencies, and Tauri
`custom-protocol`. Staged source excludes `.env`, unrelated untracked files, and
copied tests that would otherwise be discovered twice by the host's Vitest run.
Only the staged capture source gains local font CSS and WebKit zoom permission.

The capture frontend uses `VITE_CAPTURE_FIXTURES=1` and
`VITE_BLOCKFIELD_PACK_URL=https://capture.invalid`. Preflight checks the URL,
fixture marker, and local-font CSS before Cargo. The URL is required because
content modules validate it during import, before IPC/HTTP mocks install.

A newly built binary remains invalid until the native smoke proves fixture render,
loaded Inter faces, and a screenshot:

```sh
scripts/capture/run.sh main --validate-build --out /tmp/blockfield-smoke
```

Never distribute this capture executable. Normal `pnpm build`/Tauri builds compile
out the fixture import and do not transform the Tauri API. The capture-only Vite
plugin intercepts the bundled API module because native Tauri's `invoke` property
is immutable; it does not modify installed dependencies or native internals.

## Capture

```sh
scripts/capture/run.sh main --size 1280x720 --scale 1 --lang ru --out /tmp/baseline
scripts/capture/run.sh settings-skin --size 1600x900 --scale 1.5 --out /tmp/baseline
scripts/capture/run.sh settings-skin --record 3 --fps 60 --out /tmp/baseline
scripts/capture/baseline.sh /tmp/baseline
```

The product is RU-only (`Lang = 'ru'` and a fixed App context). EN/UK are rejected;
no translations are invented. Size is the logical viewport, at least 1280×720.
Scale 1/1.25/1.5 uses actual WebKit webview zoom; physical output is size times scale.
This covers WebKit rasterization/layout scaling, not physical GTK/Wayland DPI.

States: `first-run`, `login`, `main`, `main-launching`, `update-42`, `stats`,
`settings-general`, `settings-runtime`, `settings-commands`, `settings-skin`,
`settings-launcher`, `offline`, `error`. Launcher-update availability is visible in
the shell and launcher settings. Skin/cape PNGs are synthetic local UV test assets.
Actual components, navigation buttons, IPC contracts, CSS animation, skin walking,
and auto-rotation remain the implementation under test.

`baseline.sh` captures all 13 states twice, main/skin at 1.25x/1.5x twice, and main
at 1600×900 twice. It records main, update-42, and skin for 3 seconds at 60 fps.
Existing successful captures resume only after artifact hashes match. Failed
captures preserve diagnostic screenshots/logs and do not become baseline entries.
Static pairs use `magick compare -metric AE`. `matrix-status.json` keeps residuals;
`manifest.json` indexes every accepted PNG/MP4, including repeat/smoke subfolders,
with SHA-256 hashes, build/source fingerprints, sizes/scales, RU language, font
proof, GL/WebGL renderer, and JS errors. Individual `run.sh` calls append locally;
run `baseline.sh` again to refresh the aggregate manifest after manual recovery.

FFmpeg records lossless H.264 4:4:4 at 60 capture frames/second. Software rendering
can produce duplicate frames; capture cadence is not a promise of 60 distinct
renders/second. Use ffprobe/framehash to inspect recorded cadence and motion.

## Fonts, launch selection, and static capture

`prepare-fonts.py` downloads the exact Google Fonts request in production
`src/styles/fonts.css`: Inter 400–800 and IBM Plex Mono 400–700. URLs and file
SHA-256 hashes are retained. The capture's staged stylesheet uses local URLs.
Every capture waits for `document.fonts.ready`, requires a loaded Inter FontFace,
and checks `document.fonts.check`. The manifest records face status/weight,
computed font-family lists, body font, and source hashes. IBM Plex Mono is loaded;
the shipped monospace stack remains the UI's existing stack.

The capture-only `console.c` preload uses public WebKit APIs to enable release
console logging, append launch query parameters, and add user styles for static
animation/transition freeze and caret hiding. Selection is `?state=...&lang=ru&scale=...&motion=0|1`.
The disposable native config keeps a valid synthetic username; native config
load/save is used only for bootstrap and the readiness/font evidence marker.
All application IPC/HTTP is mocked and unknown requests fail closed. Native
window controls remain native. Static JS wall/animation clocks are fixed; motion
preserves the animation clock while fixing wall time.

Tauri CSP blocks the fixture's inline style element; the native user stylesheet
applies the same capture rules without relaxing CSP. Tauri's first post-mock
native readiness save falls back from blocked mock HTTP to its own postMessage
transport. These expected capture-only warnings remain in app logs.
`progress-ready.js` adds an invisible readiness witness after the real UI shows
42.0%, for the embedded c2 bundle's strict `42%` predicate. The source predicate is
fixed; remove the compatibility witness after the next authorized frontend relink.
It never changes displayed text, progress, accessibility, or pixels.

WebKit's subprocess sandbox is disabled only inside the private rootless,
networkless container for nested user-namespace compatibility. Container isolation
remains enabled; no privilege, host namespaces, Docker socket, host-system bind,
or desktop socket is used.

## Extend and check

Add fixture names/data in `src/lib/fixtures/data.ts`, navigation/readiness in
`index.ts`, and the name in `run.py`. Keep unexpected IPC/HTTP fail-closed.

```sh
pnpm test
pnpm exec tsc --noEmit
pnpm exec eslint vite.config.ts scripts/capture/vite.ts src/main.tsx src/lib/fixtures/*.ts
python3 -m unittest discover -s scripts/capture -p 'test_*.py'
pnpm build
# Must produce no matches; fixture chunks/test texture payloads must also be absent.
rg 'Capture fixture|CaptureOperator|capture-ready:|__BLOCKFIELD_CAPTURE_INVOKE__' dist
```

Known nondeterminism: software raster edge variation, font/rasterizer versions,
startup phase in motion, and software frame pacing. The first c2 stats pair differs
in 118 RGBA8 pixels; 17/18 matrix pairs are exactly identical. Preserve and quantify
such residuals; do not silently apply a tolerance. Match image/font/bundle hashes
and renderer when comparing with a native POC. WebGL can expose vendor strings
that disagree with GLX; retain both, and use GLX to identify this software runtime.

## Cross-platform CI reference

`.github/workflows/tauri-xplat.yml` runs only on manual dispatch or pushes to
`native/tauri-xplat`. It stages tracked source with `xplat_prepare.py`, reuses
`prepare-fonts.py`, and builds one unsigned, unbundled Tauri release with capture
fixtures and a separate `com.blockfield.capture` identity. No Rust hooks or normal
build changes are needed: fixtures also accept `capture-selection:<JSON>` in the
disposable native config's language field. The old Linux query path is preserved.
`capture.css` freezes static captures without a CSP-blocked inline stylesheet.

`xplat.py` refuses to run outside GitHub Actions. It captures nine matched states
at 1280×720, verifies fixture state, viewport and loaded Inter faces, and records
WebGL renderer evidence for skin. Native screenshots use client bounds (Windows),
window-only `screencapture` (macOS), or Xvfb window geometry (Linux). Wrong scale or
size fails, rather than silently resizing the evidence. Capture errors and app
logs are artifacts even on failure. `xplat_report.py` compares committed per-OS
native goldens using ImageMagick RMSE and AE with fuzz 2%, writing JSON, diff PNGs
and Markdown. Differences are evidence, not an automatic visual-parity approval.
The workflow keeps artifacts for seven days and never signs or uploads releases.
