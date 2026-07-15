# DEV MVP audit closure

Date: 2026-07-15
Source: `BLOCKFIELD_DEV_MVP_AUDIT.md`
Branch: `dev`

This report maps every backlog item to its implementation and automated proof. It distinguishes code closure from the release-candidate gate: the latter still requires execution of the clean Windows scenarios in `docs/MVP_E2E_SMOKE.md` against deployed services and a signed build.

## P0

| Item      | Status | Implementation and proof                                                                                                                                                                                                                                                                                         |
| --------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| BF-P0-001 | Closed | Laravel login/refresh/logout/me API, hashed rotating sessions, expiry/revocation/status checks and throttling; persistent launcher auth in `src/lib/auth.ts`. `LauncherAuthTest` and frontend auth tests cover the normal and failure paths.                                                                     |
| BF-P0-002 | Closed | Dedicated `LauncherUser` and `LauncherSession` models/migration with normalized unique identity, UUID, status/role and token hashes. Existing Filament administrators remain separate.                                                                                                                           |
| BF-P0-003 | Closed | `LauncherUserResource` supports search/filter/create/edit/password reset/status/ban/session revocation and audited actions. Resource authorization and transition tests pass.                                                                                                                                    |
| BF-P0-004 | Closed | Login/main screens render authenticated identity only after the auth API succeeds and distinguish invalid credentials, expiry and service failure. No verified fallback remains.                                                                                                                                 |
| BF-P0-005 | Closed | Launch revalidates `/auth/me`, passes the authenticated username, stable UUID and short-lived signed project ticket, and rejects placeholder identity. Launch arguments and tokens are not logged.                                                                                                               |
| BF-P0-006 | Closed | Rust API implements Minecraft Server List Ping with timeout and 10-second cache, configured region/location, freshness time and unavailable state. Frontend renders nullable real values. Rust tests cover success, malformed data, offline, timeout and cache.                                                  |
| BF-P0-007 | Closed | Reload requires a dedicated strong bearer secret, compares it in constant time, returns structured 401/409/503 responses and is single-flight. Publication logs request/release/version/phase.                                                                                                                   |
| BF-P0-008 | Closed | CMS middleware fails closed for missing/short tokens and always checks the protected bearer token. `CmsTokenTest` covers missing, wrong and valid configuration.                                                                                                                                                 |
| BF-P0-009 | Closed | Release-specific staging, safe extraction, manifest verification, atomic active/previous rename, persistent active manifest and rollback are implemented. Rust tests cover unsafe/corrupt archives, empty manifests, rollback, retention and restart.                                                            |
| BF-P0-010 | Closed | Filament provides draft, validate, publish, rollback and archive actions. Publish invokes backend activation, marks exactly one DB row active and compensates backend activation if the DB transaction fails.                                                                                                    |
| BF-P0-011 | Closed | Client prune rules reject absolute paths, parent components, drive/UNC prefixes, empty paths, unsupported globs and escapes; deletion remains contained in the game root. Rust contract tests pass.                                                                                                              |
| BF-P0-012 | Closed | Java extraction rejects unsafe/symlink entries, stages into a temporary directory, verifies `bin/java`, and atomically swaps while retaining the previous runtime on failure. Tests cover malicious, missing and valid archives.                                                                                 |
| BF-P0-013 | Closed | Java metadata requires immutable HTTPS URL, exact positive size and SHA-256 in CMS, server manifest and launcher. Download is size/hash verified before staged extraction.                                                                                                                                       |
| BF-P0-014 | Closed | Tauri updater public key remains configured; CI requires signing secrets and a non-empty `.sig`, validates `update.json`, and will not publish unsigned metadata. Signed artifacts are packaged into the final backend image; a server-only authenticated GitHub proxy remains as a private-repository fallback. |
| BF-P0-015 | Closed | First Filament admin requires explicit email and a non-default password of at least 12 characters; startup never overwrites an existing admin password. Seeder tests pass.                                                                                                                                       |
| BF-P0-016 | Closed | Filament Docker/Compose/entrypoint and examples use the same `FILAMENT_*`, CMS, reload and signing environment contract.                                                                                                                                                                                         |

## P1

| Item      | Status | Implementation and proof                                                                                                                                                                        |
| --------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| BF-P1-001 | Closed | Release form uses structured archive/Java/Forge/version fields, exactly-one-source validation and operator actions rather than raw JSON. Validation runs in both UI and controller/model paths. |
| BF-P1-002 | Closed | Editable CMS content no longer owns ping, player count, capacity, region or auth/network health. Migration clears old fake operational fields.                                                  |
| BF-P1-003 | Closed | Launcher computes Java/Forge/Minecraft/runtime/modpack work before freezing the byte denominator; skipped files do not count as transferred and bootstrap phases are explicitly indeterminate.  |
| BF-P1-004 | Closed | Downloads use `.part`, flush/sync, verify size/hash, then atomically replace the target while preserving the previous valid file. Startup/cancel cleanup removes stale partials safely.         |
| BF-P1-005 | Closed | Explicit connect/read/request timeouts, bounded three-attempt retry/backoff, HTTP Range resume and visible retry status are implemented. Checksum failures restart cleanly.                     |
| BF-P1-006 | Closed | Server files are streamed from disk with `Content-Length`, immutable cache headers, single-range 206 support and no compression for binary data.                                                |
| BF-P1-007 | Closed | Server and client SHA-256 helpers use buffered streaming reads.                                                                                                                                 |
| BF-P1-008 | Closed | CORS is an explicit origin/method/header allowlist; privileged reload still requires service authentication.                                                                                    |
| BF-P1-009 | Closed | Tauri has restrictive default/script/style/img/connect CSP plus `object-src 'none'` and `frame-src 'none'`.                                                                                     |
| BF-P1-010 | Closed | Frontend file and arbitrary shell permissions were removed. Only dialog, updater and process-restart permissions required by the UI remain; sensitive file work stays in Rust commands.         |
| BF-P1-011 | Closed | Package, workspace, Tauri and Cargo versions are `0.1.0`; UI reads the runtime package version and CI runs `scripts/check-version.mjs`.                                                         |
| BF-P1-012 | Closed | `/health/live` checks process liveness; `/health/ready` verifies active manifest/files, writable storage and required CMS availability.                                                         |
| BF-P1-013 | Closed | Backend content cache has a 60-second TTL, ETag/304 behavior and protected invalidation invoked after Filament content writes. Launcher refreshes content on startup.                           |
| BF-P1-014 | Closed | Filament users have explicit admin/editor roles and panel/resource/action authorization. Launcher users cannot enter Filament.                                                                  |
| BF-P1-015 | Closed | Settings require a safe absolute writable game directory, validate selected Java by running `-version` for Java 17/21, and clamp RAM to a safe minimum and physical-memory fraction.            |

## P2

| Item      | Status | Implementation and proof                                                                                                                                                       |
| --------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| BF-P2-001 | Closed | Human labels/groups/help, length limits, notifications, structured forms, release state indicators and deployment/server dashboard widget were added.                          |
| BF-P2-002 | Closed | `AdminAuditLog` records actor/action/target/release/previous/result/failure/time without secrets; Filament exposes read-only history.                                          |
| BF-P2-003 | Closed | Backend release operations emit structured JSON fields for request ID, release, version, phase and result; launch argument lists are not logged.                               |
| BF-P2-004 | Closed | Cancellation propagates to parallel jobs, removes staging partials and never writes the installed manifest after partial failure. UI distinguishes cancellation/failure/retry. |
| BF-P2-005 | Closed | Successful Tauri updater installation calls the process plugin relaunch flow; capability is limited to restart.                                                                |
| BF-P2-006 | Closed | Operational fallback numbers/status/location were removed. Unavailable live data is null/offline and rendered as unavailable.                                                  |
| BF-P2-007 | Closed | Retention covers expired sessions/audits, failed staging, archives, current+one previous release and launcher partials while preserving rollback.                              |
| BF-P2-008 | Closed | Rust, FrankenPHP and Composer container inputs are digest-pinned; lockfiles are authoritative; CI runs pnpm/composer/cargo audits, Trivy and gitleaks.                         |

## Automated verification

Executed on 2026-07-15 from the dirty `dev` working tree:

- `pnpm install --frozen-lockfile`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test`: 3 files, 7 tests passed
- `pnpm version:check`
- `pnpm stylelint`
- `pnpm audit --audit-level high`: no known vulnerabilities
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 33 tests passed
- `cargo audit`: no blocking vulnerability; 18 informational transitive warnings remain, primarily Tauri Linux GTK3 maintenance warnings plus upstream unsoundness notices
- `composer validate --strict`: valid
- `composer audit --locked`: no security advisories
- `php artisan test`: 17 tests, 60 assertions passed
- PHP syntax check for every override file
- Prettier and `git diff --check`

### Local process integration

An isolated SQLite Laravel/Filament application and the current `target/debug/blockfield-server.exe` were run together on Windows. The HTTP-level results were:

- live/ready/manifest `200`; a single byte range returned `206` and the exact `Content-Range`;
- offline Minecraft status returned `online=false` with null player and latency values;
- anonymous and incorrect reload tokens returned `401`;
- login `200`, `/me` `200`, refresh rotation `200`, reused refresh token `401`, logout `200`, post-logout `/me` `401`;
- disabling the launcher account invalidated its protected request with `401`;
- five invalid logins returned `401` and the sixth returned `429`;
- content returned an ETag and matching `If-None-Match` returned `304`;
- simultaneous reloads returned one `200` and one `409`;
- a corrupt archive returned `500` while the previous manifest and file content remained available and unchanged;
- restart without CMS or source ZIP restored the persisted active manifest and file;
- required-CMS outage produced live `200`, ready `503`; missing active release produced ready/manifest `503`.

This run found and fixed an empty-body logout proxy bug; `auth_proxy_accepts_empty_logout_body` now guards the path.

CI additionally owns the frontend production build, Filament Docker build/test, image scan, secret scan and signed Windows Tauri build. These are not represented as locally passed until the workflow runs on the resulting revision.

### Dev deployment verification

The deployed CMS applied all four MVP migrations and is healthy. Release `0.1.0` is active with a server-computed 458,300,386-byte archive SHA-256, immutable Temurin 17.0.19+10 and Forge 47.4.10 metadata, and an audited bootstrap repair. A second administrator account was created without logging its password.

The deployed API returned live/ready `200`, a 4,772-file manifest, exact single-byte `206` range delivery, empty-body logout `200`, invalid login `401`, and live status for the configured `minecraft.play.nether.pp.ua:25565` endpoint. At the time of verification the Minecraft endpoint itself timed out: its DNS resolved to Cloudflare proxy addresses and no Minecraft SRV record existed. The previous private GitHub release produced updater `204`; the final CI now packages signed updater files into the backend image and also rejects incomplete or unsigned metadata.

## Release-candidate boundary

The code backlog is closed. The product must not yet be labelled externally MVP-ready until the Windows matrix in `docs/MVP_E2E_SMOKE.md` is executed against a deployed CMS/API/Minecraft environment with test accounts, a real modpack release and updater signing credentials. This is an environment/signing evidence gate, not an omitted code fix.
