#!/usr/bin/env python3
"""Build public updater manifests from completed, signed native release assets."""

import argparse
import datetime
import json
import sys
from pathlib import Path
from urllib.parse import quote

# Native launcher release artifacts. Linux + Windows only: the manifest
# requires every asset with a non-empty minisign signature; a partial set
# fails the publish job loudly.
RELEASE_ASSETS = {
    "linux-x86_64": ("blockfield-launcher_{version}_amd64.AppImage", "appimage"),
    "linux-x86_64-deb": ("blockfield-launcher_{version}_amd64.deb", None),
    "windows-x86_64": ("blockfield-launcher_{version}_x64_en-US.msi", "msi"),
    "windows-x86_64-nsis": ("blockfield-launcher_{version}_x64-setup.exe", None),
}


def _collect_platforms(directory, version):
    base = f"https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{version}/"
    missing = []
    for _platform, (template, _bundle) in RELEASE_ASSETS.items():
        name = template.format(version=version)
        if not (directory / name).is_file():
            missing.append(name)
            continue
        sig = directory / (name + ".sig")
        if not sig.is_file() or not sig.read_text().strip():
            missing.append(name + ".sig")
    if missing:
        raise ValueError(
            "Incomplete release asset set, refusing to publish: " + ", ".join(missing)
        )
    platforms = {}
    for platform, (template, bundle) in RELEASE_ASSETS.items():
        name = template.format(version=version)
        signature = (directory / (name + ".sig")).read_text().strip()
        entry = {"signature": signature, "url": base + quote(name)}
        platforms[platform] = entry
        if bundle:
            platforms[f"{platform}-{bundle}"] = entry
    return platforms


def prepare(directory, version, *, notes=None, pub_date=None):
    path = directory / "latest.json"
    existing = {}
    if path.is_file():
        existing = json.loads(path.read_text())
        if existing.get("version") != version:
            raise ValueError("Updater version does not match release")
    platforms = _collect_platforms(directory, version)
    data = dict(existing)
    data["version"] = version
    if notes is not None:
        data["notes"] = notes
    elif "notes" not in data:
        data["notes"] = ""
    if pub_date is not None:
        data["pub_date"] = pub_date
    elif "pub_date" not in data:
        data["pub_date"] = (
            datetime.datetime.now(datetime.timezone.utc)
            .replace(microsecond=0)
            .isoformat()
            .replace("+00:00", "Z")
        )
    data["platforms"] = platforms
    path.write_text(json.dumps(data, indent=2) + "\n")


def prepare_native(directory, version):
    # Backward-compatible manifest for existing native-channel installs (v2.0.0):
    # points at the same signed assets on the versioned release so clients on
    # native-channel upgrade seamlessly onto latest.json.
    platforms = _collect_platforms(directory, version)
    path = directory / "latest-native.json"
    path.write_text(
        json.dumps({"version": version, "platforms": platforms}, indent=2) + "\n"
    )


def _parse_semver(version):
    # Semver §11 precedence: numeric MAJOR.MINOR.PATCH, then prerelease
    # (a version without prerelease outranks one with it); build metadata
    # after "+" is ignored.
    text = str(version).strip()
    if text[:1] == "v":
        text = text[1:]
    text = text.split("+", 1)[0]
    core_text, sep, pre_text = text.partition("-")
    core = []
    for part in core_text.split("."):
        core.append(int(part) if part.isdigit() else 0)
    while len(core) < 3:
        core.append(0)
    prerelease = None
    if sep and pre_text:
        prerelease = []
        for identifier in pre_text.split("."):
            if identifier.isdigit():
                prerelease.append((0, int(identifier), ""))
            else:
                prerelease.append((1, 0, identifier))
    return tuple(core), prerelease


def _version_key(version):
    # Release (no prerelease) sorts above its prereleases: 1.0.20 > 1.0.20-rc.1.
    core, prerelease = _parse_semver(version)
    if prerelease is None:
        return (core, 1, [])
    return (core, 0, prerelease)


def download_error_is_missing(log):
    # Only a missing channel release/asset means "nothing published yet".
    # Any other download failure must fail the job, never upload blind.
    lowered = str(log).lower()
    return (
        "404" in lowered
        or "release not found" in lowered
        or "no assets match" in lowered
    )


def native_manifest_newer(current_path, version):
    # Concurrent tag releases share the rolling channel: only a strictly
    # newer version may replace the published manifest, so an older release
    # that finishes last cannot hide a newer update. Missing or unreadable
    # manifest = nothing published yet, so publish.
    try:
        current = json.loads(Path(current_path).read_text()).get("version", "")
    except (OSError, ValueError):
        return True
    # Equal republishes: a re-run of the same release (e.g. re-signed
    # assets) must be able to replace its own manifest.
    return _version_key(version) >= _version_key(current)


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    if len(args) == 4 and args[0:2] == ["native", "newer-than"]:
        return 0 if native_manifest_newer(Path(args[2]), args[3]) else 1
    if len(args) == 3 and args[0:2] == ["native", "download-is-missing"]:
        return 0 if download_error_is_missing(args[2]) else 1
    if len(args) == 3 and args[0] == "native":
        prepare_native(Path(args[1]), args[2])
        return 0

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    parser.add_argument("--notes-file", type=Path, default=None)
    parsed = parser.parse_args(args)
    notes = (
        parsed.notes_file.read_text(encoding="utf-8").strip()
        if parsed.notes_file is not None
        else None
    )
    prepare(parsed.directory, parsed.version, notes=notes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
