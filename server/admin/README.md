# Filament CMS

Laravel/Filament admin panel for managing Blockfield Launcher content, updates, and modpack releases.

Docker image: `ghcr.io/netherg-io/blockfield-launcher-filament`  
Port: `8055`

## Collections

The API server reads only rows where `status` is `published`.

### `modpack_releases`

Required fields:
- `version`
- `minecraft_version`
- either `modpack_zip` (file upload) or `zip_url` (URL)

Optional:
- `prune` — JSON array of paths to delete, e.g. `["mods/old.jar", "config/*"]`
- `java` — JSON: `{ "version", "platform", "url", "sha256", "size" }`
- `forge` — JSON: `{ "version", "url", "sha256", "size" }`

### `launcher_content`

Visible launcher copy, translations, feature cards, and news feed.

JSON fields:
- `translations` — `{ "en": { "nav.deploy": "DEPLOY" }, "ru": { "nav.deploy": "БОЙ" } }`
- `features` — `{ "icon": "flag|swords|truck|crosshair", "title": "...", "desc": "..." }`
- `feed` — `{ "tag": "PATCH", "tone": "amber|green|sand", "date": "06.07", "title": "...", "body": "..." }`

## Environment variables

See [`.env.example`](./.env.example). Required vars in production:

- `FILAMENT_APP_KEY` — **must be stable between deploys**
- `CMS_TOKEN` — shared secret with the API server
- `FILAMENT_ADMIN_EMAIL` / `FILAMENT_ADMIN_PASSWORD` — admin login

Generate secrets:

```sh
printf 'FILAMENT_APP_KEY=base64:%s\n' "$(openssl rand -base64 32)"
printf 'CMS_TOKEN=%s\n' "$(openssl rand -hex 32)"
```

## Dokploy deployment

Deploy as a **separate Dokploy app** (not compose). Use the Docker image built by GitHub Actions.

1. Create a Dokploy app from the image `ghcr.io/netherg-io/blockfield-launcher-filament:develop`
2. Set port to `8055`
3. Add the [environment variables](#environment-variables)
4. Attach domain (e.g. `admin.blockfield.gg`) to port `8055`
5. Mount volume `/data` — contains SQLite database and uploaded modpack ZIPs
   - **Do not delete this volume** on redeploy unless you want to reset the CMS
6. Deploy
7. Open `https://admin.blockfield.gg/admin` and sign in

### Webhook

Set `DOKPLOY_FILAMENT_WEBHOOK_URL` in GitHub Actions secrets to trigger Dokploy redeploy on push.

## Local Docker build

```sh
docker build -f server/admin/Dockerfile -t blockfield-filament server/admin
docker run --rm -p 8055:8055 \
  -e CMS_TOKEN=blockfield-dev-token \
  -v filament-data:/data \
  blockfield-filament
```

Admin: `http://localhost:8055/admin`  
Default dev login: `admin@example.com` / `admin`

## Troubleshooting

- **Admin login resets after deploy** — set a stable `FILAMENT_APP_KEY` and keep the `/data` volume.
- **401 from `/api/items/*`** — bearer token does not match `CMS_TOKEN`. Check both the Filament env and the API server env.
