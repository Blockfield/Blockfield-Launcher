# Windows MVP E2E smoke test

Use this checklist for the release candidate produced from `dev`. Do not replace a result with unit-test evidence: each scenario must run on Windows against the deployed API, Filament, Minecraft server and signed updater channel.

## Evidence header

| Field                            | Value |
| -------------------------------- | ----- |
| Commit SHA                       |       |
| Launcher version                 |       |
| Windows version / VM snapshot    |       |
| API deployment/image digest      |       |
| Filament deployment/image digest |       |
| Minecraft endpoint/version       |       |
| Tester and UTC time              |       |

Attach screenshots or logs with secrets and tokens redacted. Never attach launcher config/session storage or full Java launch arguments.

## Prerequisites

- A clean Windows VM or unused Windows user profile.
- A valid active test user and a second admin account for Filament.
- Deployed API/CMS configured with strong `CMS_TOKEN`, `RELOAD_TOKEN` and `AUTH_SIGNING_KEY`.
- Reachable Minecraft server and one validated active release.
- A signed newer Windows launcher build in the configured updater channel.
- A small delta release that changes one file and prunes one known test file.
- A corrupt ZIP fixture that cannot be published.

## Scenarios

Record `PASS` only after every step in the row has been observed.

The local process integration recorded in `docs/MVP_AUDIT_CLOSURE.md` proves the API/auth/reload contracts, including rollback, restart, rate limit, offline telemetry and health behavior. It does not replace the clean-profile launcher/game observations below.

| Scenario           | Required observations                                                                                                                                                                  | Result  | Evidence |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | -------- |
| Clean install      | Login succeeds; actual username/status appear; Java size/hash verifies; Forge, libraries, assets and modpack install; game starts; in-game identity equals authenticated account.      | NOT RUN |          |
| No-op relaunch     | Remembered session restores; version is current; zero unnecessary downloads; game starts.                                                                                              | NOT RUN |          |
| Delta update       | Publish from Filament; launcher sees version; only changed requirements transfer; denominator is stable; prune stays in game root; game starts.                                        | NOT RUN |          |
| Interrupted update | Cancel or interrupt network; old valid files remain; `.part` is not installed; retry resumes/completes; game starts.                                                                   | NOT RUN |          |
| Failed publication | Corrupt ZIP is rejected; previous release remains active; an existing client can still fetch its manifest/files.                                                                       | NOT RUN |          |
| User enforcement   | Login, then disable/ban and revoke sessions in Filament; refresh/protected launch returns user to login; launch remains denied.                                                        | NOT RUN |          |
| Server offline     | Stop the test Minecraft server; launcher shows offline/unavailable with no player/ping fallback; restore server and observe fresh real status.                                         | NOT RUN |          |
| Self-update        | Older launcher detects signed newer build; signature validates; update installs and relaunches; displayed/runtime version matches. Also verify unsigned/tampered metadata is rejected. | NOT RUN |          |

## Failure capture

For a failed row record:

1. exact step and UTC time;
2. launcher/API/CMS version or image digest;
3. sanitized launcher and backend logs around the request ID;
4. whether the previous installed/release state remained usable;
5. a reproducible follow-up issue.

## Release gate

The release candidate passes this manual gate only when all eight scenario rows are `PASS`, the CI quality/Filament/secret-scan/Windows-release jobs are green, and every evidence link is populated.
