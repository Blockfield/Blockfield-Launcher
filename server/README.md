# Blockfield API Server

Rust API server for the Blockfield Launcher. Serves modpack manifests, files, and launcher content to Tauri clients.

Docker image: `ghcr.io/netherg-io/blockfield-launcher-backend`  
Port: `3000`

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/api/launcher/v1/content.json` | Launcher UI content |
| `GET` | `/api/launcher/v1/manifest.json` | Modpack file manifest |
| `GET` | `/api/launcher/v1/update.json` | Launcher self-update info |
| `GET` | `/api/launcher/v1/files/{path}` | Individual modpack files |
| `POST` | `/api/launcher/v1/reload` | Reload modpack from CMS |

## Environment variables

See [`.env.example`](./.env.example). Required vars in production:

- `BASE_URL` — public origin for generated file URLs
- `CMS_URL` — Filament CMS API URL
- `CMS_TOKEN` — bearer token for CMS authentication

Generate secrets:

```sh
printf 'CMS_TOKEN=%s\n' "$(openssl rand -hex 32)"
```

## Dokploy deployment

Deploy as a **separate Dokploy app** (not compose). Use the Docker image built by GitHub Actions.

1. Create a Dokploy app from the image `ghcr.io/netherg-io/blockfield-launcher-backend:develop`
2. Set port to `3000`
3. Add the [environment variables](#environment-variables)
4. Attach domain (e.g. `play.blockfield.gg`) to port `3000`
5. Mount volumes:
   - `/data/files` — modpack ZIPs and CMS download cache
   - `/data/extracted` — auto-populated modpack extraction cache
6. Deploy

Verify:

```sh
curl -f https://play.blockfield.gg/health
curl -f https://play.blockfield.gg/api/launcher/v1/content.json
curl -f https://play.blockfield.gg/api/launcher/v1/manifest.json
```

### Webhook

Set `DOKPLOY_BACKEND_WEBHOOK_URL` in GitHub Actions secrets to trigger Dokploy redeploy on push.

## Local Docker build

```sh
docker build -f server/Dockerfile -t blockfield-api .
docker run --rm -p 3000:3000 \
  -e CMS_URL=http://host.docker.internal:8055/api \
  -e CMS_TOKEN=blockfield-dev-token \
  blockfield-api
```

## Troubleshooting

- **API starts with fallback content** — check `CMS_URL` and `CMS_TOKEN`. Set `CMS_REQUIRED=true` to fail fast.
- **Modpack upload works but launcher sees old manifest** — `POST /api/launcher/v1/reload` or restart.
- **502 after deploy** — wait for modpack extraction to finish, then check logs.
