# u-launcher-selfupdate — evidence

Base: `origin/feat/native-slint` @ `30a9116` (fetched 2026-11-11; HEAD == tip, 0 behind).
Branch: `feat/u-selfupdate-c1`. Preflight note: workspace `agent_preflight.py`
compares the checkout against `origin/main`, so any `feat/native-slint`-based
branch reports DIVERGED-vs-main; the task base itself is current (see above).

## What changed (3 files, all inside the unit's scope)

- `native/app/src/update.rs`: new `pub mod self_update` backend + wiring.
  - Same channel/key as Tauri: latest.json endpoint, pinned minisign pubkey
    (source of truth `src-tauri/tauri.conf.json`), `linux-{arch}-deb` target,
    `windows-{arch}` then `windows-{arch}-nsis` lookup order.
  - Verification mirrors `tauri-plugin-updater 2.13.1`: outer-base64 minisign
    text, key-id match, `verify(data, sig, allow_legacy=true)`, plus the
    `version:` trusted-comment pin when the signature carries one (a tampered
    manifest pairing a new version with an older signed release is rejected).
  - Linux install ports `src-tauri/src/native_update.rs` exactly: ar →
    `data.tar.gz` → single regular `usr/bin/blockfield-launcher`, 64-bit LE
    ELF for the host CPU, AppImage reject, `ldd` gate, atomic replace with
    `.previous` backup. `rollback()` restores the backup (format re-validated,
    backup never touched, no `ldd` gate on the recovery path).
  - Windows install: verify, stage to temp, hand to `msiexec /i /passive`
    (MSI magic) or run the setup exe — OS installer owns replace/rollback.
  - Panel wiring in `attach_core`: check → checking/available/current/error,
    install → downloading (+progress)/verifying/installing/installed + game-busy
    and DEV/debug guards with the Tauri error strings, restart → game-busy
    guard → spawn current exe → exit. Fixture backend untouched.
- `native/app/Cargo.toml` (+ `Cargo.lock`, additive only): direct deps
  `reqwest 0.13`, `semver`, `minisign-verify 0.2`, `base64 0.23`, `ar 0.9`,
  `tar 0.4`, `flate2 1`, `tempfile 3`, `serde` (derive). All versions match the
  main workspace lock / local cargo cache.
- No `.slint` change needed: `LauncherUpdatePanel` already implements every
  phase (checking/current/available/downloading/verifying/installing/
  installed/restarting/error). No `src-tauri` change: logic ported, not moved.

## Tests (fail-before: new backend; genuine-signature positive case proves the
verify path accepts valid signatures, tampered variants prove rejection)

`cargo test --lib update::` — 12 passed, incl. 7 new:
- `tampered_package_is_rejected` (flipped byte, bad base64, non-minisign,
  foreign key-id rejected; genuine pair accepted)
- `signed_version_pin_rejects_a_repaired_manifest` (pin enforced when present,
  absent pin allowed — Tauri default)
- `manifest_picks_the_platform_entry_and_compares_versions` (target order,
  semver newer/equal/older, bad JSON/versions, missing platform)
- `linux::foreign_arch_payload_is_rejected` (host CPU ok; foreign CPU,
  AppImage marker, truncation, bad magic rejected)
- `linux::deb_extract_keeps_only_the_native_executable`
- `linux::rollback_restores_the_previous_binary` (no-backup refuse, backup
  written on install, corrupt current restored, corrupt backup refused)
- `progress_bytes_use_russian_units`
Test signature vectors minted with openssl (ed25519) + python, verified
independently with `openssl pkeyutl -verify` (message + global signatures).

## Gates run in native/app

- `cargo fmt --check` ok; `cargo clippy --all-targets -- -D warnings` ok;
  `cargo metadata --locked` ok; `cargo check --all-targets` ok.
- `cargo test --lib` — 31 passed.
- `cargo test --test layer1_screens` with `SLINT_EMIT_DEBUG_INFO=1` (CI flag) —
  1 passed (without the flag it fails on the ElementHandle guard, pre-existing
  harness requirement, also documented in tests/README.md).

## Not verified / known limits

- Windows `install_package` compiles only under `cfg(windows)`; only the
  Linux target toolchain is installed here, so it is syntax-reviewed +
  std-only but not type-checked. CI `native.yml` (Windows job) must show it
  green; the coordinator merge gate (cross-engine update + rollback test
  pre-release) still owns end-to-end proof — no live network/install was run
  from this unit.
- Native binary version is `0.1.0`, so a check against the real channel
  reports the Tauri `1.0.x` release as available until native versioning
  aligns; installing it is the (gated) cross-engine path, unchanged by this unit.
