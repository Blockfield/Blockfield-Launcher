# debug-log: u-launcher-platform flaky exit 127 (coordination#43)

Symptom: native.yml "Capture main and settings-skin", Windows, `default` renderer,
scale-1, `--state settings-skin` exits 127 ~0.5s after start. app.log ends after
ADAPTER line (skin wgpu Renderer created, no first frame delivered, no Rust panic).
Same binary/state/scale passes with `SLINT_BACKEND=winit-software` and at scale-1.5.
Seen 2x on Launcher#119 (jobs 130306131146/130318217569, run 42954427161),
green on #117/#121 at same base.

## Hypotheses

1. CI "default renderer" is not the default renderer (CONFIRMED, code-read):
   native.yml runs `SLINT_BACKEND=${SLINT_BACKEND-} ... run_guarded ...` after
   `unset SLINT_BACKEND`, so the exe inherits SLINT_BACKEND="" (present-but-empty)
   via the VAR= prefix + python env passthrough. main.rs gates the renderer probe
   on `var_os().is_none()` -> false for empty -> probe skipped -> Slint default
   renderer (Skia GL attempt on a GPU-less runner). Slint itself treats "" as
   default (selector lib.rs: `var().unwrap_or_default()`, empty -> default path),
   so the window still opens; only OUR probe is skipped.
   Evidence: no RENDERER_PROBE line in ANY default-run app.log in the failing
   artifact; all default runs log "Failed to initialize Skia opengl surface ...
   Falling back to the next surface candidate".
2. Death is in the Skia GL-init/fallback path on Microsoft Basic Render Driver
   (CONFIRMED as the only delta: software-backend runs of identical state/scale
   never attempt GL and never die; main-state default runs attempt GL and usually
   survive -> intermittent). Exit 127 with no Rust panic = death below the Rust
   unwind boundary (native GL/D3D init during surface fallback), before first
   skin frame. settings-skin is the only capture state that inits wgpu DX12
   concurrently, matching "first wgpu init in the job fails, later ones pass".
   Exact faulting call not provable without a Windows repro (no Windows here).
3. Fix must remove the GL attempt on software-only adapters (not retry/loop):
   treat empty SLINT_BACKEND as unset -> probe runs -> Basic Render Driver is
   Cpu device_type -> `skia-software` renderer (CPU raster, Slint source
   i-slint-backend-winit lib.rs: `(Some("skia-software"), None)` arm, no GL).
   Fail-safe: probe failure -> software renderer (existing `unwrap_or_else`).
   Layer-2 e2e already pins `winit-software` everywhere for the same reason (u17).

## Fix

- native/app/src/main.rs: `var_os().is_none()` -> nonempty check (one-liner).
- No CI loop change needed: with the fix, CI default runs probe -> skia-software.
- macOS dropped from native.yml / tauri-xplat.yml / standards.yml (owner decision).

## Verification (local, 2026-11-11)
- cargo fmt --all -- --check (native/app): clean
- cargo metadata --locked --no-deps (native/app): ok
- cargo clippy --locked --profile ci-test --all-targets: 0 warnings
- cargo test --locked --profile ci-test (SLINT_EMIT_DEBUG_INFO=1): all suites ok, 0 failed
- actionlint on native.yml / tauri-xplat.yml / standards.yml: clean
- No Windows runner here: 10x green Windows e2e is CI evidence, left for the coordinator to watch.
