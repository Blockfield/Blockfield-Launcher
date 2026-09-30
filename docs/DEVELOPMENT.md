# Launcher development

> **2026-08 architecture.** The launcher no longer talks to the Rust API server or the Filament CMS.
> Everything it needs is static, served from the packwiz pack host (`VITE_BLOCKFIELD_PACK_URL`,
> currently https://blockfield.pro, repo `blockfield-modpack`):
>
> | File                       | Purpose                                                                                                                                                                                        |
> | -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
> | `pack.toml` / `index.toml` | packwiz pack — version, Minecraft/Fabric loader versions, mod list                                                                                                                             |
> | `launcher.json`            | game server (`host:port`, used for `--quickPlayMultiplayer`), packwiz bootstrap + installer jar hashes, Temurin 21 JRE per supported platform (Windows x86_64; Linux/macOS x86_64 and aarch64) |
> | `content.json`             | UI texts / feed                                                                                                                                                                                |
>
> Flow on "Play": download Java (zip on Windows, tar.gz on Linux/macOS) → Fabric launch profile from meta.fabricmc.net →
> `java -jar packwiz-installer-bootstrap.jar --bootstrap-no-update --bootstrap-main-jar packwiz-installer.jar -g -s client --pack-folder <game dir> <pack.toml>` (it downloads,
> verifies and prunes the pack) → vanilla runtime files → launch with the offline username from
> Settings. Server status is a direct Server List Ping from Rust (`src-tauri/src/status.rs`).
> Launcher builds and the updater's `latest.json` are published by CI to the public
> https://github.com/Blockfield/Blockfield-Launcher/releases. Linux install without root: `scripts/install-linux.sh` (plain `cargo build --release` needs `--features tauri/custom-protocol`, otherwise the window tries to load the Vite dev server).
> The retired API/CMS source has been removed from the active tree. See [historical notes](legacy/README.md) for recovery from Git history.

The launcher checks the installed pack on startup; **Settings → Check at startup** also enables
packwiz verification and repair of an up-to-date installation. **Verify files** runs this manually.
A new pack version uses **Update**; missing Java/runtime files use **Prepare**. Navigation reuses
the current check and keeps an ongoing installation alive. Default maximum heap is 4 GiB; saved
memory choices remain unchanged. Java chooses its GC tuning; initial heap is 512 MiB.

**Settings → Launch commands** supports a shell command before Minecraft and after its process
exits (including a nonzero exit). A failing pre-launch command cancels the launch. Commands run
in the game directory with `sh -c` on Linux/macOS or `cmd /D /S /C` on Windows. Environment:
`INST_DIR`, `INST_MC_DIR`, `INST_JAVA`, and `INST_EXIT_CODE` (post-exit only; empty for termination
without an exit code). Keep the launcher open for post-exit commands. Command output is appended
to `logs/launcher-hooks.log`; the current game's stdout/stderr goes to `logs/launcher-game.log`.
Commands configured in the launcher are local settings, never supplied by the hosted pack.

Server region is resolved from the game server's public IP via HTTPS to `ipwho.is`, with a
24-hour in-memory cache (five minutes for failed lookups). The displayed country describes the
server IP's approximate location; failed/private-address lookups display a dash. Ping and player
counts still come directly from Minecraft and do not depend on a successful location lookup.

Native Linux installs made with `scripts/install-linux.sh` check for launcher releases at startup
and show their status under Settings → Launcher and in the clickable footer version.
Installation starts with the Install button, reports downloaded bytes and signature/install
phases, and leaves a result with a Restart button. Errors can be retried without closing the
app; progress survives navigation between screens. The `.blockfield-native-install` marker opts this
installation into native updates; unmarked binaries and debug builds do not self-update.
The updater downloads the existing signed `linux-<arch>-deb` release asset, verifies it with
Tauri's configured public key, and reads only `usr/bin/blockfield-launcher` from `data.tar.gz`.
It does not install a Debian package or replace system libraries. The native ELF/CPU and `ldd`
dependency checks run before an atomic replacement; `blockfield-launcher.previous` keeps the
previous binary for rollback. AppImage and Windows installations retain Tauri's standard updater.
The game and file operations must finish before the launcher can install an update.

A new version must be published in the existing public release feed before other installations
can receive it; local edits and same-version builds are not updates. To exercise signature
verification against the actual public Debian asset without replacing the installed launcher:

```sh
scripts/rust-env.sh cargo test -p blockfield-launcher --lib \
  native_update::tests::published_deb_has_a_valid_signature_and_native_payload -- --ignored
```

## Active components

| Component                | Location                                 | Responsibility                                                                |
| ------------------------ | ---------------------------------------- | ----------------------------------------------------------------------------- |
| Desktop UI               | `src/`                                   | Launcher screens, local identity/settings and UI state                        |
| Native launcher          | `src-tauri/`                             | Pack/runtime installation, launching, status, integrations and signed updates |
| Shared Rust contracts    | `shared/`                                | Types still imported by the native launcher; not a deployed service           |
| Static pack/content host | Separate `blockfield-modpack` repository | `pack.toml`, `index.toml`, `launcher.json`, `content.json`                    |
| Launcher update feed     | Releases in this public repository       | Signed bundles and `latest.json`                                              |

## Local development

Install Node.js 22, Python 3.12+, Just 1.57.0, Rust 1.98.1, and the platform dependencies required by Tauri. Then:

```sh
just setup
just tauri-dev
```

`VITE_BLOCKFIELD_PACK_URL` points at the static pack directory. Local `.env`
files stay untracked. No API container, CMS, database, CMS token, or remote
launcher account is required for this architecture.

On Linux, `scripts/install-tauri-deps.sh` installs the build dependencies;
`scripts/rust-env.sh` can use the existing local container-builder fallback.
For frontend-only development use `just dev` with the existing Tauri mocks.

## Content, accounts and integrations

Modpack/content changes belong in the separate pack repository and its
publishing workflow, not in this launcher repository. Keep the installer
and Java artifact hashes in `launcher.json` consistent with the pack.
Do not use the retired CMS publication workflow.

The launcher uses local/offline Minecraft identities and the saved nickname.
This is not proof of ownership of a Microsoft account. Authentication and
gameplay access rules on the Minecraft server are a separate concern; the
retired API's HMAC ticket flow is not part of this launcher setup.

Rooms, deep links and Discord integration remain supported. Follow
[the current rooms and Discord guide](ROOMS-AND-DISCORD.md).

## Validation

```sh
just check
just config-test
just version-check
just build
```

Release checks run only on version tags; plain pushes and pull requests do not consume release runners.
Automated tests do not replace the [manual launcher smoke checklist](LAUNCHER_SMOKE.md).
The [archived API/CMS reports](legacy/README.md) are historical only.

## CI/CD

[`.github/workflows/release.yml`](../.github/workflows/release.yml) publishes stable releases from `main`:

1. Run frontend and Rust checks, dependency audits, and the Git history secret scan.
2. Build and sign Linux, Windows, and macOS (Intel and Apple Silicon) bundles under the versioned `launcher-v<version>` tag.
3. Validate all signed updater assets, then publish the completed draft in [this repository](https://github.com/Blockfield/Blockfield-Launcher/releases).

Quality checks and secret scanning gate the platform builds. Standard GitHub-hosted runners build Linux on `ubuntu-22.04` (preserving the glibc baseline), Windows on `windows-2025`, Apple Silicon on `macos-15`, and Intel Macs on `macos-15-intel`. Platform builds run in parallel, with a 60-minute limit, and upload to a draft release. Publication waits for all four builds and updater validation. No self-hosted runner is required.

Actions caches are scoped per ref, so a tag run cannot read caches saved by earlier tags. The same workflow runs on `main` every Monday (and on demand via `workflow_dispatch`): it runs the checks and builds, publishes nothing and saves the Rust and pnpm caches that tag runs restore read-only. Run it by hand after a dependency or Rust toolchain update.

macOS apps are ad-hoc signed, without Apple notarization. After copying the app from the DMG into Applications, allow its first launch in System Settings → Privacy & Security. Updater archives are separately signed with the existing Tauri key. The hosted pack provides Java 21 downloads for Windows, Linux and both macOS architectures. The launcher selects, downloads, verifies and configures its runtime automatically; no manual Java installation or path selection is required.

Before releasing, update `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` together, then run `pnpm version:check`. The signing secrets (`TAURI_PRIVATE_KEY`, `TAURI_KEY_PASSWORD`) and build variables remain in the existing GitHub environment named `dev`; its deployment policy must permit `main` and `launcher-v*` tags only. Tag builds also verify that the commit belongs to `main`. Publishing uses this repository’s `GITHUB_TOKEN`; no cross-repository token is needed.

The launcher tracks the game through preparation, running, and post-exit commands. Both launch
buttons use that shared status, and the backend rejects overlapping launches. Opening the
launcher again focuses its existing window. While a game session is active, closing the window
hides it so the process monitor and exit hooks continue to run. Settings → General includes
an opt-in “Hide while playing” toggle; a hidden window returns after exit, including crashes.

## Dev build

`src-tauri/src/dev.rs` holds every developer-only behaviour, and all of it hangs off the
compile-time `BLOCKFIELD_DEV_BUILD=1`. A production build compiles those functions to constants
(`dev_status` returns `null`, the panel in Settings → Launcher renders nothing), so this code can
live on `main` without changing the player launcher — merging the dev branch is safe by
construction. Everything else is configured through the environment at runtime, never through a
saved setting:

| Variable                     | Effect                                                         |
| ---------------------------- | -------------------------------------------------------------- |
| `BLOCKFIELD_DEV_SERVER`      | `host:port` for Quick Play, or `off` to start at the main menu |
| `BLOCKFIELD_DEV_FREEZE_PACK` | skip packwiz entirely: no update prompt, no download, no prune |
| `BLOCKFIELD_DEV_JVM_ARGS`    | extra JVM arguments for the game process                       |
| `BLOCKFIELD_DEV_GAME_DIR`    | default game directory of a fresh dev profile                  |

A dev build uses its own bundle identifier and `launcher-config-dev.json`, and defaults to the
`BlockField-Dev` game directory, so it installs and runs next to a player installation.

```sh
BLOCKFIELD_DEV_BUILD=1 pnpm tauri build --config src-tauri/tauri.dev.conf.json
```

Actions → **Launcher Dev Build** does the same for Windows, Linux or macOS and uploads the bundle
to the rolling `launcher-dev` prerelease; it never changes the stable updater manifest. The local stack the
build is meant for (server, testbot client, AI tester) is `blockfield-modpack/docs/DEVELOPING.md`.

## License

Original launcher code is licensed under [GPL-3.0-only](../LICENSE). Third-party code and assets retain their respective licenses, including [Lucide/Feather icons](discord-assets/LICENSE). This code license does not grant rights to third-party game content or trademarks.

## Legacy updater channel

Versions through 1.0.4 check `netherg-io/blockfield-launcher-releases`. Its final transition release installs 1.0.6, which checks this repository instead. Keep that legacy repository and its transition assets available; do not delete it or reuse its name. Future releases are published only here.
