# core

Tauri-independent engine of the Blockfield Launcher. `src-tauri` is a thin
adapter over these crates; the native Slint app will call the same ops.

- `launcher-core`: config, game state, rooms links, dev flags. No I/O beyond
  files and processes.
- `launcher-net`: pack/mirror I/O, downloads, accounts, skins, status probes.
- `launcher-app`: shared `LauncherAppState`, operation gate, `LauncherEvent`
  seam and the `ops` bodies behind every Tauri command.

Dependency direction: `launcher-app` → `launcher-net` → `launcher-core`.
Nothing in `core/**` depends on `tauri` (CI pins this: no `tauri` in
`core/*/Cargo.toml`).

The Tauri adapter maps each `LauncherEvent` back to today's exact event
name and payload (`event_name` in `src-tauri/src/lib.rs`, pinned by
`src-tauri/tests/adapter_contract.rs`); command return shapes are
pinned by `core/launcher-app/tests/adapter_contract.rs` plus golden files.
JSON key order is not part of the contract (the frontend uses `JSON.parse`);
goldens compare as parsed JSON.

The native app drives `LauncherAppState` directly and subscribes its own UI
updates through the `EventSink` instead of the Tauri adapter.
