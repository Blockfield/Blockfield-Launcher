# Retired API/CMS documentation

These July 2026 audit and smoke-test documents describe the retired Rust API,
Laravel/Filament CMS, server-managed accounts, and older release pipeline.
They are historical evidence, not setup instructions or claims about the
current static packwiz launcher. Do not run their deployment commands as
part of current launcher setup.

The retired source is available in Git history at
`7a342dc2935665779534d2709081c6458e20d77a`:

```sh
git show 7a342dc2935665779534d2709081c6458e20d77a:server/README.md
git worktree add --detach ../blockfield-launcher-legacy 7a342dc2935665779534d2709081c6458e20d77a
```

Use a separate worktree only for inspection or an explicitly planned recovery.
Do not redeploy these services, overwrite installations, or delete data volumes.
Cleanup removes tracked legacy source; it does not touch running services,
`/data`, user settings, worlds, packs, releases, or Git history.

Current documentation: [launcher guide](../../README.md),
[launcher smoke checklist](../LAUNCHER_SMOKE.md), and
[rooms and Discord](../ROOMS-AND-DISCORD.md).
