# Blockfield Launcher

> **2026-08 architecture.** The launcher no longer talks to the Rust API server or the Filament CMS.
> Everything it needs is static, served from the packwiz pack host (`VITE_BLOCKFIELD_PACK_URL`,
> currently https://modpack.nether.pp.ua, repo `blockfield-modpack`):
>
> | File | Purpose |
> | --- | --- |
> | `pack.toml` / `index.toml` | packwiz pack — version, Minecraft/Fabric loader versions, mod list |
> | `launcher.json` | game server (`host:port`, used for `--quickPlayMultiplayer`), packwiz bootstrap + installer jar hashes, Temurin 17 JRE per platform (`windows/linux` × `x86_64/aarch64`) |
> | `content.json` | UI texts / feed |
>
> Flow on "Play": download Java (zip on Windows, tar.gz on Linux) → Fabric launch profile from meta.fabricmc.net →
> `java -jar packwiz-installer-bootstrap.jar --bootstrap-no-update --bootstrap-main-jar packwiz-installer.jar -g -s client --pack-folder <game dir> <pack.toml>` (it downloads,
> verifies and prunes the pack) → vanilla runtime files → launch with the offline username from
> Settings. Server status is a direct Server List Ping from Rust (`src-tauri/src/status.rs`).
> Launcher builds and the updater's `latest.json` are published by CI to the public
> https://github.com/netherg-io/blockfield-launcher-releases (this repo is private, so its own release
> assets are unusable by the updater). Linux install without root: `scripts/install-linux.sh` (plain `cargo build --release` needs `--features tauri/custom-protocol`, otherwise the window tries to load the Vite dev server).
> `server/` (API + Filament CMS) is kept in the repo but is no longer deployed or required.

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

Three independent deployable parts:

| Part                   | Dir                                | Port   | Image                                             | Docs                                                 |
| ---------------------- | ---------------------------------- | ------ | ------------------------------------------------- | ---------------------------------------------------- |
| Tauri desktop launcher | `src-tauri/` `src/`                | —      | —                                                 | [`.env.example`](./.env.example)                     |
| Blockfield API server  | [`server/`](./server/)             | `3000` | `ghcr.io/netherg-io/blockfield-launcher-backend`  | [`server/README.md`](./server/README.md)             |
| Filament CMS           | [`server/admin/`](./server/admin/) | `8055` | `ghcr.io/netherg-io/blockfield-launcher-filament` | [`server/admin/README.md`](./server/admin/README.md) |

The API server reads launcher content, updates, and modpack releases from Filament through the CMS API (`GET /api/items/*`, `GET /api/assets/{id}`, `POST /api/files`).

## Tauri client — local dev

```sh
pnpm install
pnpm tauri:dev
```

Environment variables: copy [`.env.example`](./.env.example) to `.env` and adjust `VITE_BLOCKFIELD_API_URL` to point to your API server.

## Local server smoke test

From PowerShell (Docker inside WSL):

```powershell
wsl sh -lc "cd /mnt/d/External/Projects/Tauri/'Blockfield Launcher' && docker compose -f server/docker-compose.yml up -d --build"
```

Or from a Linux shell inside the repo:

```sh
docker compose -f server/docker-compose.yml up -d --build
```

Check:

```sh
curl -f http://localhost:3000/health
curl -I http://localhost:8055/admin
curl -H "Authorization: Bearer $CMS_TOKEN" "http://localhost:8055/api/items/launcher_content?limit=1"
curl -f http://localhost:3000/api/launcher/v1/content.json
```

The first Filament administrator is created from explicit `FILAMENT_ADMIN_EMAIL` and a password of at least 8 characters. Existing credentials are never overwritten on restart.

Stop:

```sh
docker compose -f server/docker-compose.yml down
```

> The compose file is for **local dev only**. In production, the API server and Filament CMS are deployed as **separate Dokploy apps** — each has its own README and env file.

## Publishing a modpack

Via Filament admin panel, or CLI:

```sh
CMS_URL=https://admin.blockfield.gg/api \
CMS_TOKEN=replace-with-long-random-token \
RELOAD_TOKEN=replace-with-a-different-random-token \
BLOCKFIELD_API_URL=https://play.blockfield.gg/api/launcher/v1 \
JAVA_VERSION=17.0.16+8 \
JAVA_PLATFORM=windows-x64 \
JAVA_URL=https://artifacts.example.com/java/jre-17.0.16+8-windows-x64.zip \
JAVA_SHA256=replace-with-64-hex-characters \
JAVA_SIZE=replace-with-exact-byte-size \
pnpm cms:publish-modpack ./server/files/modpack.zip 0.1.44 1.21.1
```

Filament invalidates edited content automatically and its Publish action activates releases. The protected endpoint is available only for operator recovery:

```sh
curl -X POST -H "Authorization: Bearer $RELOAD_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"releaseId": 42}' \
  https://play.blockfield.gg/api/launcher/v1/reload
```

## Authentication model

The MVP uses project-owned offline-mode identities. Filament manages separate launcher accounts; login issues a revocable 15-minute HMAC-signed game ticket plus a rotating refresh token. The launcher verifies `/auth/me` immediately before every game launch and passes the stable account username/UUID and ticket to Minecraft. The Minecraft server authentication plugin must validate that ticket against `/api/launcher/v1/auth/me`; Microsoft/Xbox authentication is intentionally not mixed into this model.

## CI/CD

[`.github/workflows/release.yml`](.github/workflows/release.yml) publishes stable releases from `main`:

1. Run frontend and Rust checks, dependency audits, and the Git history secret scan.
2. Build and sign Linux and Windows bundles under the versioned `launcher-v<version>` tag.
3. Publish bundles and `latest.json` to the public [releases repository](https://github.com/netherg-io/blockfield-launcher-releases), where installed launchers check for updates.

Before releasing, update `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` together, then run `pnpm version:check`. The signing secrets (`TAURI_PRIVATE_KEY`, `TAURI_KEY_PASSWORD`) and build variables remain in the existing GitHub environment named `dev`; that environment permits releases from `main`. `RELEASES_TOKEN` must have write access to the public releases repository.

The launcher tracks the game through preparation, running, and post-exit commands. Both launch
buttons use that shared status, and the backend rejects overlapping launches. Opening the
launcher again focuses its existing window. While a game session is active, closing the window
hides it so the process monitor and exit hooks continue to run. Settings → General includes
an opt-in “Hide while playing” toggle; a hidden window returns after exit, including crashes.
