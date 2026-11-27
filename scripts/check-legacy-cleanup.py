#!/usr/bin/env python3
"""Guard the static native launcher architecture; requires Git and Python 3.11+."""

import subprocess
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
errors = []
workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
members = workspace["members"]

for retired in ("server", "src-tauri"):
    try:
        candidates = (
            subprocess.check_output(
                [
                    "git",
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "--",
                    retired,
                ],
                cwd=root,
            )
            .decode()
            .split("\0")
        )
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"Unable to inspect legacy source with Git: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    legacy_source = [path for path in candidates if path and (root / path).exists()]
    if retired in members or legacy_source:
        errors.append(f"Retired {retired} source/workspace member has returned.")

if not {
    "shared",
    "core/launcher-core",
    "core/launcher-net",
    "core/launcher-app",
}.issubset(members):
    errors.append("The active core/shared workspace members are missing.")

lock = tomllib.loads((root / "Cargo.lock").read_text())
if any(package["name"] == "blockfield-server" for package in lock["package"]):
    errors.append("Cargo.lock still contains the retired workspace package.")

native_toml = root / "native/app/Cargo.toml"
app_toml = root / "core/launcher-app/Cargo.toml"
if not native_toml.is_file():
    errors.append("Missing native/app/Cargo.toml.")
else:
    native_deps = tomllib.loads(native_toml.read_text())["dependencies"]
    via_core_app = (
        native_deps.get("launcher-app", {}).get("path") == "../../core/launcher-app"
        and app_toml.is_file()
        and tomllib.loads(app_toml.read_text())["dependencies"]
        .get("blockfield-shared", {})
        .get("path")
        == "../../shared"
    )
    if not via_core_app:
        errors.append("The active shared contract dependency must be retained.")

readme = (root / "README.md").read_text()
for stale in (
    "Three independent deployable parts:",
    "VITE_BLOCKFIELD_API_URL",
    "pnpm cms:publish-modpack",
    "## Local server smoke test",
):
    if stale in readme:
        errors.append(
            f"Active README still contains retired setup instructions: {stale}"
        )

for relative in (
    "docs/LAUNCHER_SMOKE.md",
    "docs/ROOMS-AND-DISCORD.md",
    "docs/legacy/README.md",
):
    if not (root / relative).is_file():
        errors.append(f"Missing current/archive documentation: {relative}")

if errors:
    print("\n".join(errors), file=sys.stderr)
    raise SystemExit(1)
print("Static native launcher boundary verified; active shared contracts preserved.")
