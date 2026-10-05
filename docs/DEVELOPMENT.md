# Launcher development

The [shared architecture map](https://github.com/Blockfield/blockfield-workspace/blob/main/docs/ARCHITECTURE.md)
describes the repositories and their contracts. The launcher installs the public client pack from
`VITE_BLOCKFIELD_PACK_URL` (currently https://blockfield.pro). Its static files are built by
[blockfield-client](https://github.com/Blockfield/blockfield-client):

| File                       | Purpose                                                                        |
| -------------------------- | ------------------------------------------------------------------------------ |
| `pack.toml` / `index.toml` | Pack version, Minecraft/Fabric versions and client files                       |
| `launcher.json`            | Game server, installer hashes, Java downloads and optional Drasl URL (`skins`) |
| `content.json`             | UI texts and feed                                                              |

On **Play**, the launcher validates or refreshes the Drasl session, prepares Java and Fabric,
runs the pinned packwiz installer, prepares the vanilla runtime and launches Minecraft with the
account's player name, UUID and access token. The pack's `server` field, including
`raknet;host:port`, passes unchanged as one `--quickPlayMultiplayer` argument and
as `BLOCKFIELD_SERVER`. The status/region path strips `raknet;` and uses TCP 25565
for SLP: Blockfield's proxy listens for RakNet on UDP 25566. Automatic pre-login
TCP retry belongs to the client mod; the launcher does not select a transport. The client mod completes authentication through
Drasl; the Velocity plugin verifies the session before routing the player.

`src-tauri/src/commands.rs` combines a direct Server List Ping to the public server with room data
from the site's `/api/rooms`. The site queries the internal game server's SLP extension and the
public proxy separately. The launcher polls this combined status every 10 seconds; rooms do not
come from the public proxy's ping response.

Launcher bundles and the signed updater's `latest.json` are published to
[this repository's releases](https://github.com/Blockfield/Blockfield-Launcher/releases).
Linux installation without root uses `scripts/install-linux.sh`.

The August 2026 migration replaced the old Rust API/Filament CMS installation flow with packwiz.
Those retired services remain documented in [historical notes](legacy/README.md); current accounts
and dynamic room/statistics APIs are provided by Drasl and blockfield-site.

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

| Component                | Location                           | Responsibility                                                                |
| ------------------------ | ---------------------------------- | ----------------------------------------------------------------------------- |
| Desktop UI               | `src/`                             | Launcher screens, account/settings and UI state                               |
| Native launcher          | `src-tauri/`                       | Pack/runtime installation, launching, status, integrations and signed updates |
| Shared Rust contracts    | `shared/`                          | Types still imported by the native launcher; not a deployed service           |
| Static pack/content host | `Blockfield/blockfield-client`     | `pack.toml`, `index.toml`, `launcher.json`, `content.json`                    |
| Accounts                 | Drasl at `skins.blockfield.pro`    | Yggdrasil sessions, account API, skins and capes                              |
| Site API                 | `Blockfield/blockfield-site`       | `/api/rooms`, public statistics and web accounts                              |
| Login and routing        | `Blockfield/blockfield-proxy`      | Drasl session verification, backend routing and login forwarding              |
| Game bridge              | `Blockfield/blockfield-mod`        | Client authentication, room requests, presence and server SLP snapshot        |
| Launcher update feed     | Releases in this public repository | Signed bundles and `latest.json`                                              |

## Local development

Install Node.js 24.21.0 LTS (see `.nvmrc`), Python 3.12.15+, Just 1.58.0, Rust 1.99.0, and the platform dependencies required by Tauri.

`just setup` installs the pinned pnpm, formatters, and linters into `.cache/quality`.
Build scripts use Python 3.12.15 with zlib 1.3.2 on Linux and Windows so archive
compression stays reproducible; stale cached tool versions are replaced automatically.

```sh
just setup
just tauri-dev
```

`VITE_BLOCKFIELD_PACK_URL` points at the pack and site API origin. Local `.env` files stay untracked.
`just dev` runs frontend-only development with the existing Tauri mocks. A real game launch requires
a Drasl account; the frontend mocks do not.

On Linux, `scripts/install-tauri-deps.sh` installs the build dependencies;
`scripts/rust-env.sh` can use the existing local container-builder fallback.
For game or mod development, use [blockfield-workspace](https://github.com/Blockfield/blockfield-workspace/blob/main/docs/DEVELOPING.md).
It owns the isolated server, direct Minecraft launch and testbot on Linux and Windows. Separate
dev-launcher releases are retired. The older integration-profile code in `src-tauri/src/dev.rs`
remains in the source tree while that cleanup is completed.

## Content, accounts and integrations

Modpack/content changes belong in [blockfield-client](https://github.com/Blockfield/blockfield-client) and its
publishing workflow, not in this launcher repository. Keep the installer
and Java artifact hashes in `launcher.json` consistent with the pack.
Do not use the retired CMS publication workflow.

`src-tauri/src/account.rs` signs in through Drasl's Yggdrasil API and API v3. The Yggdrasil access
token is passed to Minecraft; the API v3 token is used for account, skin and cape changes. The
client mod sends the game token only to Drasl's session service, then the proxy checks `hasJoined`.
Players using an offline launcher can instead authenticate with `/login` in the proxy lobby and
use the client mod's remembered-session protocol. This is separate from the retired Rust API's
HMAC ticket flow.

Changing authentication requires checking the launcher, client mod and proxy together; web
accounts use the same Drasl service. Room/schema changes also require the site's `/api/rooms`.

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

The pinned Tauri release action uses versioned macOS updater archives
(`blockfield-launcher_<version>_x64.app.tar.gz` and `_aarch64.app.tar.gz`). The
publication check requires every installer and its nonempty signature before
rebuilding `latest.json` with public release download URLs.

Actions caches are scoped per ref, so a tag run cannot read caches saved by earlier tags. The same workflow runs on `main` every Monday (and on demand via `workflow_dispatch`): it runs the checks and builds, publishes nothing and saves the Rust and pnpm caches that tag runs restore read-only. Run it by hand after a dependency or Rust toolchain update.

macOS apps are ad-hoc signed, without Apple notarization. After copying the app from the DMG into Applications, allow its first launch in System Settings → Privacy & Security. Updater archives are separately signed with the existing Tauri key. The hosted pack provides Java 21 downloads for Windows, Linux and both macOS architectures. The launcher selects, downloads, verifies and configures its runtime automatically; no manual Java installation or path selection is required.

Before releasing, update `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` together, then run `pnpm version:check`. The signing secrets (`TAURI_PRIVATE_KEY`, `TAURI_KEY_PASSWORD`) and build variables remain in the existing GitHub environment named `dev`; its deployment policy must permit `main` and `launcher-v*` tags only. Tag builds also verify that the commit belongs to `main`. Publishing uses this repository’s `GITHUB_TOKEN`; no cross-repository token is needed.

The launcher tracks the game through preparation, running, and post-exit commands. Both launch
buttons use that shared status, and the backend rejects overlapping launches. Opening the
launcher again focuses its existing window. While a game session is active, closing the window
hides it so the process monitor and exit hooks continue to run. Settings → General includes
an opt-in “Hide while playing” toggle; a hidden window returns after exit, including crashes.

## License

Original launcher code is licensed under [GPL-3.0-only](../LICENSE). Third-party code and assets retain their respective licenses, including [Lucide/Feather icons](discord-assets/LICENSE). This code license does not grant rights to third-party game content or trademarks.

## Legacy updater channel

Versions through 1.0.4 check `netherg-io/blockfield-launcher-releases`. Its final transition release installs 1.0.6, which checks this repository instead. Keep that legacy repository and its transition assets available; do not delete it or reuse its name. Future releases are published only here.
