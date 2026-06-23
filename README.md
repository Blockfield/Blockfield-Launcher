# Blockfield Launcher

Three independent deployable parts:

| Part | Dir | Port | Image | Docs |
|------|-----|------|-------|------|
| Tauri desktop launcher | `src-tauri/` `src/` | — | — | [`.env.example`](./.env.example) |
| Blockfield API server | [`server/`](./server/) | `3000` | `ghcr.io/netherg-io/blockfield-launcher-backend` | [`server/README.md`](./server/README.md) |
| Filament CMS | [`server/admin/`](./server/admin/) | `8055` | `ghcr.io/netherg-io/blockfield-launcher-filament` | [`server/admin/README.md`](./server/admin/README.md) |

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
curl -H "Authorization: Bearer blockfield-dev-token" "http://localhost:8055/api/items/launcher_content?limit=1"
curl -f http://localhost:3000/api/launcher/v1/content.json
```

Default local Filament login: `http://localhost:8055/admin` / `admin@example.com` / `admin`

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
BLOCKFIELD_API_URL=https://play.blockfield.gg/api/launcher/v1 \
pnpm cms:publish-modpack ./server/files/modpack.zip 0.1.44 1.20.1
```

After manual CMS edits, reload the API:

```sh
curl -X POST https://play.blockfield.gg/api/launcher/v1/reload
```

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
