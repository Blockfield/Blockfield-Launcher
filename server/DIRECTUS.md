# Directus CMS model

Run:

```bash
cd server
docker compose up -d
cd ..
pnpm cms:bootstrap
```

Studio: `http://localhost:8055`

Default dev login:

- Email: `admin@example.com`
- Password: `d1r3ctu5`
- API token for `blockfield-api`: `blockfield-dev-token`

`pnpm cms:bootstrap` creates the collections and starter content below. You can run it more than once.

## Collections

Every collection has a `status` field. The launcher API only reads rows where `status` is `published`.

### `modpack_releases`

Required:

- `version` string
- `minecraft_version` string
- either `modpack_zip` file or `zip_url` string

Optional:

- `zip_url` string for an externally hosted ZIP
- `prune` JSON array, for example `["mods/old.jar", "config/*"]`
- `java` JSON object matching `{ "version", "platform", "url", "sha256", "size" }`
- `forge` JSON object matching `{ "version", "url", "sha256", "size" }`
- Flat alternatives: `java_version`, `java_platform`, `java_url`, `java_sha256`, `java_size`, `forge_version`, `forge_url`, `forge_sha256`, `forge_size`

Publish a new Minecraft build by uploading the ZIP to `modpack_zip`, bumping `version`, then calling:

```bash
curl -X POST http://localhost:3000/api/launcher/v1/reload
```

Or publish from the command line:

```bash
pnpm cms:publish-modpack ./server/files/modpack.zip 0.1.44 1.20.1
```

### `launcher_updates`

- `version` string
- `notes` text
- `pub_date` string, ISO date
- `windows_url` string
- `windows_signature` string

Advanced alternative: use `platforms` JSON to fully control the Tauri updater response.

### `launcher_content`

- `brand` string
- `brand_subtitle` string
- `chrome_title` string
- `operation_name` string
- `season` string
- `description` text
- `server_name` string
- `server_ip` string
- `server_region` string
- `operators` string
- `ping` string
- `region` string
- `launcher_version` string
- `coordinates` string
- `copyright` string
- `login_sector` string
- `login_slogan` string
- `operator_handle` string
- `operator_initials` string
- `operator_rank` string
- `support_label` string
- `network_status` string
- `update_description` text
- `settings_preferences` string
- `translations` JSON object for UI text overrides, for example:

```json
{
  "en": { "nav.deploy": "DEPLOY" },
  "ru": { "nav.deploy": "БОЙ" },
  "uk": { "nav.deploy": "БІЙ" }
}
```

- `features` JSON array: `{ "icon": "flag|swords|truck|crosshair", "title": "...", "desc": "..." }`
- `feed` JSON array: `{ "tag": "PATCH", "tone": "amber|green|sand", "date": "06.07", "title": "...", "body": "..." }`
