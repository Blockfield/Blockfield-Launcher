# Blockfield Launcher

> **2026-08 architecture.** The launcher no longer talks to the Rust API server or the Filament CMS.
> Everything it needs is static, served from the packwiz pack host (`VITE_BLOCKFIELD_PACK_URL`,
> currently https://modpack.dev.nether.pp.ua, repo `blockfield-modpack`):
>
> | File | Purpose |
> | --- | --- |
> | `pack.toml` / `index.toml` | packwiz pack — version, Minecraft/Forge versions, mod list |
> | `launcher.json` | game server (`host:port`, used for `--quickPlayMultiplayer`), `packwiz-installer.jar` hash, Temurin 17 JRE per platform (`windows/linux/macos` × `x86_64/aarch64`) |
> | `content.json` | UI texts / feed |
>
> Flow on "Play": download Java (zip on Windows, tar.gz elsewhere) → Forge installer from Maven →
> `java -jar packwiz-installer.jar -g -s client --pack-folder <game dir> <pack.toml>` (it downloads,
> verifies and prunes the pack) → vanilla runtime files → launch with the offline username from
> Settings. Server status is a direct Server List Ping from Rust (`src-tauri/src/status.rs`).
> Launcher builds and the updater's `latest.json` are published by CI to the public
> https://github.com/netherg-io/blockfield-launcher-releases (this repo is private, so its own release
> assets are unusable by the updater). Linux install without root: `scripts/install-linux.sh`.
> `server/` (API + Filament CMS) is kept in the repo but is no longer deployed or required.

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
pnpm cms:publish-modpack ./server/files/modpack.zip 0.1.44 1.20.1
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

[`.github/workflows/dokploy-dev.yml`](.github/workflows/dokploy-dev.yml) runs on pushes to `dev`:

1. **`tauri-release`** — builds Windows Tauri `.exe`, publishes to GitHub release `develop`
2. **`backend`** — builds `blockfield-launcher-backend` image, pushes to GHCR, pings Dokploy webhook
3. **`filament`** — builds `blockfield-launcher-filament` image, pushes to GHCR, pings Dokploy webhook

Required GitHub secrets:

```
DOKPLOY_BACKEND_WEBHOOK_URL=https://dokploy.example.com/api/deploy/...
DOKPLOY_FILAMENT_WEBHOOK_URL=https://dokploy.example.com/api/deploy/...
```

If a secret is missing, the corresponding job is skipped.
