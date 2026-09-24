# Discord mode assets

Upload these 512×512 PNGs in Discord Developer Portal → application `1548300877125779456` → Rich Presence → Art Assets. Use the filename without `.png` as the asset key.

| Key | Server mode |
| --- | --- |
| `mode-room` | Захват точек |
| `mode-campaign` | Кампания |
| `mode-tdm` | Командный бой |
| `mode-deathmatch` | Каждый сам за себя |
| `mode-snipers` | Перелётные снайперы |

The launcher keeps `blockfield` as its large image and omits the small image for unknown/missing modes and outside the running game. Discord silently omits an asset until it is uploaded for the configured application. Custom application IDs need the same keys uploaded separately.

Icons reuse the project's Lucide flag, map, users, swords and crosshair geometry (`blockfield-mod/dev/ui/icons`), with a transparent background and the launcher’s amber gradient (`#8A571C → #F5A524 → #FFC861`). SVG sources are included; PNGs are rendered at 512×512. See LICENSE for ISC/MIT attribution.
