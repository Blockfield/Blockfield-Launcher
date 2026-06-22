# Filament CMS

Run:

```bash
cd server
docker compose up -d --build
```

Admin: `http://localhost:8055/admin`

Default dev login:

- Email: `admin@example.com`
- Password: `admin`

Set `FILAMENT_ADMIN_EMAIL`, `FILAMENT_ADMIN_PASSWORD`, `CMS_TOKEN`, and `FILAMENT_APP_KEY` in production.

## Collections

The launcher API reads only rows where `status` is `published`.

### `modpack_releases`

Required:

- `version`
- `minecraft_version`
- either `modpack_zip` or `zip_url`

Optional:

- `prune` JSON array, for example `["mods/old.jar", "config/*"]`
- `java` JSON object matching `{ "version", "platform", "url", "sha256", "size" }`
- `forge` JSON object matching `{ "version", "url", "sha256", "size" }`

Publish from the command line:

```bash
pnpm cms:publish-modpack ./server/files/modpack.zip 0.1.44 1.20.1
```

### `launcher_updates`

- `version`
- `notes`
- `pub_date`
- `windows_url`
- `windows_signature`
- `platforms` JSON to fully control the Tauri updater response

### `launcher_content`

Visible launcher copy, localized overrides, feature cards, and news feed.

Useful JSON fields:

- `translations`: `{ "en": { "nav.deploy": "DEPLOY" }, "ru": { "nav.deploy": "БОЙ" } }`
- `features`: `{ "icon": "flag|swords|truck|crosshair", "title": "...", "desc": "..." }`
- `feed`: `{ "tag": "PATCH", "tone": "amber|green|sand", "date": "06.07", "title": "...", "body": "..." }`
