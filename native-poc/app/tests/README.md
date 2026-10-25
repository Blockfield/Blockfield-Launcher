# Native e2e screen tests

Two layers gate every merge into `feat/native-slint` (workflow
`.github/workflows/native-poc.yml`, matrix Windows/macOS/Ubuntu).

- **Layer 1** — `tests/layer1_screens.rs`: in-process scenarios driving the
  real Slint UI tree headless via the Slint testing backend
  (`i-slint-backend-testing`: find by accessible label, invoke default
  action, mock drag; state asserted through the generated property API).
  Needs `SLINT_EMIT_DEBUG_INFO=1` at build time (set job-wide in CI);
  without it the label tree is empty and the suite fails with a guard
  message telling you to rebuild with the variable set.
- **Layer 2** — `src/e2e.rs` (`--e2e <out> --e2e-golden <dir>`): the release
  binary in a real window walks every ported screen, screenshots each state
  and compares pixels against `tests/golden/<os>/`. Any mismatch, missing
  element/golden, crash or hang exits non-zero. Tolerance: mean abs channel
  diff <= 2.0 and at most 0.2% of pixels with any-channel diff > 8
  (same-OS reruns with `--freeze-motion` sit an order of magnitude under
  these gates; see the u18 report). Renderer: `winit-software` on every OS
  (the one renderer all three runners support; the e2e log records
  `SLINT_BACKEND`). An explicit `SLINT_BACKEND` still wins over the app's
  Skia-first default (u33), so this pin is unaffected by it; scale 1x.

## Run locally

```sh
cd native-poc/app
SLINT_EMIT_DEBUG_INFO=1 CARGO_BUILD_JOBS=4 cargo test --locked --release
```

Layer 2 needs a display; use the isolated container (never the owner screen):

```sh
podman build -f native-poc/app/Headless.Containerfile -t localhost/blockfield-main-headless native-poc/app
podman run --rm --network=none --security-opt label=disable --read-only \
  -v "$PWD/native-poc/app:/app:ro" -v /tmp/e2e:/out:rw \
  localhost/blockfield-main-headless sh -ec '
  Xvfb :99 -screen 0 1920x1080x24 & sleep 1
  DISPLAY=:99 SLINT_BACKEND=winit-software BLOCKFIELD_E2E_STATE=main \
    /app/target/release/blockfield-native-poc --state main --freeze-motion \
    --e2e /out/main --e2e-golden /app/tests/golden/linux'
```

## Add a scenario + golden for a new screen (<= 10 min)

1. Layer 1: add a `scenario_*` fn in `tests/layer1_screens.rs` using the
   `given_/when_/all_labels` helpers, and call it from
   `layer1_screen_walk`. Give every new control an `accessible-label`
   (plus `accessible-role` and `accessible-action-default` where it acts)
   in `ui/*.slint` — metadata only, never layout or pixels.
2. Layer 2: if the screen is reachable by navigation, extend the walk in
   `src/e2e.rs` (new `step` + `check_shot`); if it needs fixtures, add the
   state to the CI `for state in ...` list in `native-poc.yml`.
3. Goldens: run the walk once per OS with `--e2e-update-goldens`
   (`workflow_dispatch` with `update_goldens=true` writes candidates as
   artifacts), **look at every image** against the parity reports, then
   commit them to `tests/golden/<os>/` as `<state>-<shot>.png`.
4. Prove the gate: break one assertion on one OS, watch CI turn red,
   revert. Record run IDs in `reports/e2e/progress.md`.
