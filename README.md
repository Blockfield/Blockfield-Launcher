# Blockfield Launcher

Blockfield Launcher has three deployable parts:

- the Tauri desktop launcher;
- `blockfield-server`, the Rust launcher API on port `3000`;
- `filament`, the Laravel/Filament admin CMS on port `8055`.

The API reads launcher content, launcher updates, and modpack releases from Filament through a small CMS API:

- `GET /api/items/launcher_content`
- `GET /api/items/launcher_updates`
- `GET /api/items/modpack_releases`
- `POST /api/files`
- `GET /api/assets/{id}`

## Local Docker smoke test

Docker is expected to run inside WSL on Windows.

From PowerShell:

```powershell
wsl sh -lc "cd /mnt/d/External/Projects/Tauri/'Blockfield Launcher' && docker compose -f server/docker-compose.yml up -d --build"
```

From a Linux shell already inside the repository:

```sh
docker compose -f server/docker-compose.yml up -d --build
```

Check the running services:

```sh
curl -f http://localhost:3000/health
curl -I http://localhost:8055/admin
curl -H "Authorization: Bearer blockfield-dev-token" \
  "http://localhost:8055/api/items/launcher_content?limit=1"
curl -f http://localhost:3000/api/launcher/v1/content.json
```

Default local Filament login:

- URL: `http://localhost:8055/admin`
- Email: `admin@example.com`
- Password: `admin`

Stop the stack:

```sh
docker compose -f server/docker-compose.yml down
```

## Dokploy deployment

Use a Dokploy Docker Compose app from this Git repository.

Recommended app settings:

- Branch: `dev`
- Compose file: `server/docker-compose.yml`
- Public API service: `blockfield-api`, container port `3000`
- Public admin service: `filament`, container port `8055`
- API domain example: `https://play.blockfield.gg`
- Admin domain example: `https://admin.blockfield.gg`

The current compose stack builds both deployable server images from source:

- `filament` builds from `server/admin/Dockerfile` with context `server/admin`;
- `blockfield-api` builds from `server/Dockerfile` with repository-root context.

The compose stack creates:

- `filament-data:/data` for the Filament SQLite database and uploaded modpack ZIPs;
- `extracted:/data/extracted` for the API extracted-modpack cache;
- `server/files:/data/files` for fallback local ZIPs and the API CMS download cache.

Do not delete `filament-data` during redeploys unless you intentionally want to reset the CMS.

### Current compose caveat

`server/docker-compose.yml` currently starts `blockfield-api` with:

```yaml
depends_on:
  filament:
    condition: service_healthy
```

The current `filament` service does not define a healthcheck in `server/docker-compose.yml`, and `server/admin/Dockerfile` does not define a Docker `HEALTHCHECK` instruction.

If your Compose runtime refuses to start because of this, either add a `filament` healthcheck or change the dependency to:

```yaml
depends_on:
  filament:
    condition: service_started
```

### Required environment variables

Set these in Dokploy:

```dotenv
BASE_URL=https://play.blockfield.gg
FILAMENT_APP_URL=https://admin.blockfield.gg
FILAMENT_APP_KEY=base64:replace-with-stable-laravel-key
CMS_TOKEN=replace-with-long-random-token
FILAMENT_ADMIN_EMAIL=admin@example.com
FILAMENT_ADMIN_PASSWORD=replace-with-strong-password
```

Keep this internal value as-is for the API container:

```dotenv
CMS_URL=http://filament:8055/api
```

Optional but useful:

```dotenv
CMS_REQUIRED=true
MODPACK_VERSION=0.1.43
MINECRAFT_VERSION=1.20.1
FALLBACK_SERVER_IP=play.blockfield.gg:25565
FALLBACK_UPDATE_URL=https://play.blockfield.gg/downloads/blockfield-launcher_0.1.0_x64-setup.exe
```

Generate secrets on a Linux shell:

```sh
printf 'FILAMENT_APP_KEY=base64:%s\n' "$(openssl rand -base64 32)"
printf 'CMS_TOKEN=%s\n' "$(openssl rand -hex 32)"
```

`FILAMENT_APP_KEY` must stay stable between container restarts. If it is empty, the container generates a temporary key, which is fine for local testing but not for production.

### Deploy steps

1. Create a Dokploy project.
2. Add a Docker Compose app from this repository.
3. Set the branch to `dev`.
4. Set the compose path to `server/docker-compose.yml`.
5. Add the environment variables above.
6. Attach the API domain to `blockfield-api:3000`.
7. Attach the admin domain to `filament:8055`.
8. Deploy the app.
9. Open `https://admin.blockfield.gg/admin` and sign in with `FILAMENT_ADMIN_EMAIL` / `FILAMENT_ADMIN_PASSWORD`.
10. Check the API:

```sh
curl -f https://play.blockfield.gg/health
curl -f https://play.blockfield.gg/api/launcher/v1/content.json
curl -f https://play.blockfield.gg/api/launcher/v1/manifest.json
```

## Publishing a modpack

You can publish through the Filament admin or with the CLI helper.

CLI example:

```sh
CMS_URL=https://admin.blockfield.gg/api \
CMS_TOKEN=replace-with-long-random-token \
BLOCKFIELD_API_URL=https://play.blockfield.gg/api/launcher/v1 \
pnpm cms:publish-modpack ./server/files/modpack.zip 0.1.44 1.20.1
```

The helper uploads the ZIP to Filament, creates a published `modpack_releases` row, and asks the API to reload. If you edit the CMS manually, reload the API after publishing:

```sh
curl -X POST https://play.blockfield.gg/api/launcher/v1/reload
```

## GitHub Actions deployment

`.github/workflows/dokploy-dev.yml` runs on pushes to `dev` and on manual `workflow_dispatch`.

The workflow has three jobs:

- `tauri-release`: builds the Windows Tauri release on `windows-latest`;
- `backend`: builds and pushes the Rust API Docker image;
- `filament`: builds and pushes the Filament Docker image.

The Windows Tauri dev build is published to the GitHub release/tag named `develop`, not `dev`.

The backend image is pushed to GHCR with these tags:

```text
ghcr.io/netherg-io/blockfield-launcher-backend:develop
ghcr.io/netherg-io/blockfield-launcher-backend:develop-<short-sha>
```

The Filament image is pushed to GHCR with these tags:

```text
ghcr.io/netherg-io/blockfield-launcher-filament:develop
ghcr.io/netherg-io/blockfield-launcher-filament:develop-<short-sha>
```

Required GitHub environment/secrets for webhook deployment:

```dotenv
DOKPLOY_BACKEND_WEBHOOK_URL=https://dokploy.example.com/api/deploy/...
DOKPLOY_FILAMENT_WEBHOOK_URL=https://dokploy.example.com/api/deploy/...
```

If either secret is missing, the corresponding job prints a skip message and does not trigger Dokploy for that part.

The compose deployment can still build directly from the repository. The pushed GHCR images are useful if Dokploy is configured to deploy prebuilt images or if you want GitHub Actions to fail before Dokploy pulls the latest code.

## Runtime facts

The API exposes:

- `GET /health`
- `GET /api/launcher/v1/content.json`
- `GET /api/launcher/v1/manifest.json`
- `GET /api/launcher/v1/update.json`
- `GET /api/launcher/v1/files/{path}`
- `POST /api/launcher/v1/reload`

The API reads `BASE_URL` to generate file URLs in `manifest.json`. Keep it equal to the public API origin, for example `https://play.blockfield.gg`.

The API uses `CMS_TOKEN` as a bearer token when reading from Filament. Filament returns `401` for `/api/items/*`, `/api/files`, and `/api/assets/*` when the bearer token does not match.

## Troubleshooting

- `502` or unavailable API just after deploy: wait for the API to extract the current modpack ZIP, then check `blockfield-api` logs.
- Compose fails around `service_healthy`: add a Filament healthcheck or change the dependency to `condition: service_started`.
- Admin login resets after each deploy: set a stable `FILAMENT_APP_KEY` and keep the `filament-data` volume.
- API starts with fallback content: check `CMS_URL`, `CMS_TOKEN`, and Filament logs. In production, set `CMS_REQUIRED=true` to fail fast instead.
- `401` from `/api/items/*`: the bearer token does not match `CMS_TOKEN`.
- Modpack upload works but launcher still serves the old manifest: call `POST /api/launcher/v1/reload` or restart `blockfield-api`.
