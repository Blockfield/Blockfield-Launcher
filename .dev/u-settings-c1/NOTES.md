# U-settings evidence (feat/u-settings-c1)

Base: origin/feat/native-slint (85d3996) + merge of origin/feat/u-launcher-chrome
(89b0874, PR #122 head, still open per brief) -> merge commit 6511945.

## Changes (owned files only: settings Slint + src/settings_*.rs)

1. `native/app/ui/settings.slint`
   - Deleted dead `dirty` property on SettingsScreen (AppWindow never set it,
     so the skin/launcher footer save was permanently disabled).
   - One footer for all 5 tabs now (was: separate tab>=3 footer using the
     dead property, no busy/saved/error states). Tauri shows one save/reset
     footer on every tab; the form save applies from any tab.
2. `native/app/src/settings_fixtures.rs`
   - New `settings-skin` (tab 3) and `settings-launcher` (tab 4) states with
     default/edited/error scenarios. Skin picks stay outside the settings
     form (no fake dirty; upload stays separate). Launcher scenarios drive
     the self-update panel phases (available/error).
3. `native/app/src/settings_e2e.rs`
   - Tab->state mapping fixed for tabs 3/4 (were mislabeled settings-commands).

## Checks run (local, SLINT_EMIT_DEBUG_INFO=1 where needed)

- `cargo test --manifest-path native/app/Cargo.toml --locked` — all green
  (incl. settings_tabs, settings_backend, layer1_screens walk).
- Temporary five-tab fixture test (created, passed, deleted):
  5 states x 3 scenarios mount on the right tab; skin edited sets
  skin-path+slim without dirtying the form; upload enabled only with a picked
  file; launcher phases available/error; dirty-form save stays enabled after
  switching to the skin tab (the footer regression this unit fixes).
- `cargo clippy --all-targets -- -D warnings` (native/app) — clean.
- `cargo fmt --check` with pinned toolchain 1.99.0 — owned files clean
  (pre-existing fmt drift in native/app/tests/chrome.rs belongs to #122,
  left untouched).
- Layer-2 real-window e2e / capture / verify_settings.py: NOT run locally
  (no Xvfb/display on this machine); covered by CI (native.yml: Layer-1,
  capture main+settings-skin, e2e walk incl. settings-skin tabs 0-4).

## Known follow-ups (out of scope, for coordinator)

- Skin upload/browse backends are still stubs (lib.rs POC eprintln; browse
  dialogs owned by the platform unit). Real upload needs UI state for
  busy/message (new SettingsForm fields or AppWindow props — AppWindow is
  outside this unit's files) + launcher_net::skins wiring in settings_tabs.
- `--state settings-skin/launcher` CLI routing for the new fixture states
  lives in native/app/src/main.rs (U-platform file); settings_e2e covers
  tabs 3/4 once routing invokes it.
