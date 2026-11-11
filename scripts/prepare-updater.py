#!/usr/bin/env python3
"""Build the public updater manifest from completed, signed release assets."""

import json
import sys
from pathlib import Path
from urllib.parse import quote


def prepare(directory, version):
    path = directory / "latest.json"
    data = json.loads(path.read_text())
    if data["version"] != version:
        raise ValueError("Updater version does not match release")
    base = f"https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{version}/"
    assets = {
        "linux-x86_64": (f"blockfield-launcher_{version}_amd64.AppImage", "appimage"),
        "windows-x86_64": (f"blockfield-launcher_{version}_x64_en-US.msi", "msi"),
        "darwin-x86_64": (f"blockfield-launcher_{version}_x64.app.tar.gz", "app"),
        "darwin-aarch64": (f"blockfield-launcher_{version}_aarch64.app.tar.gz", "app"),
        "windows-x86_64-nsis": (f"blockfield-launcher_{version}_x64-setup.exe", None),
        "linux-x86_64-deb": (f"blockfield-launcher_{version}_amd64.deb", None),
        "linux-x86_64-rpm": (f"blockfield-launcher-{version}-1.x86_64.rpm", None),
    }
    platforms = {}
    for platform, (name, bundle) in assets.items():
        if not (directory / name).is_file():
            raise ValueError(f"Missing updater asset: {name}")
        signature = (directory / (name + ".sig")).read_text().strip()
        if not signature:
            raise ValueError(f"Empty updater signature: {name}")
        entry = {"signature": signature, "url": base + quote(name)}
        platforms[platform] = entry
        if bundle:
            platforms[f"{platform}-{bundle}"] = entry
    data["platforms"] = platforms
    path.write_text(json.dumps(data, indent=2) + "\n")


# Native launcher artifacts mirror the Tauri naming scheme with the native
# binary name. Linux + Windows only (no macOS): the manifest requires exactly
# this set with signatures; a partial set fails the publish job loudly.
NATIVE_ASSETS = {
    "linux-x86_64-deb": "blockfield-native_{version}_amd64.deb",
    "windows-x86_64": "blockfield-native_{version}_x64_en-US.msi",
    "windows-x86_64-nsis": "blockfield-native_{version}_x64-setup.exe",
}


def prepare_native(directory, version):
    # Asset bytes stay on the versioned release; only the manifest rides the
    # rolling native-channel tag (see release.yml).
    base = f"https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{version}/"
    missing = []
    for platform, template in NATIVE_ASSETS.items():
        name = template.format(version=version)
        if not (directory / name).is_file():
            missing.append(name)
            continue
        sig = directory / (name + ".sig")
        if not sig.is_file() or not sig.read_text().strip():
            missing.append(name + ".sig")
    if missing:
        raise ValueError(
            "Incomplete native asset set, refusing to publish: " + ", ".join(missing)
        )
    platforms = {}
    for platform, template in NATIVE_ASSETS.items():
        name = template.format(version=version)
        signature = (directory / (name + ".sig")).read_text().strip()
        platforms[platform] = {"signature": signature, "url": base + quote(name)}
    path = directory / "latest-native.json"
    path.write_text(
        json.dumps({"version": version, "platforms": platforms}, indent=2) + "\n"
    )


def _version_key(version):
    # Numeric per dotted part, so 1.0.20 > 1.0.9 (plain string compare is wrong).
    key = []
    for part in str(version).split("."):
        digits = ""
        for char in part:
            if not char.isdigit():
                break
            digits += char
        key.append((int(digits) if digits else 0, part[len(digits) :]))
    return key


def native_manifest_newer(current_path, version):
    # Concurrent tag releases share the rolling channel: only a strictly
    # newer version may replace the published manifest, so an older release
    # that finishes last cannot hide a newer update. Missing or unreadable
    # manifest = nothing published yet, so publish.
    try:
        current = json.loads(Path(current_path).read_text()).get("version", "")
    except (OSError, ValueError):
        return True
    return _version_key(version) > _version_key(current)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "native":
        prepare_native(Path(sys.argv[2]), sys.argv[3])
    elif len(sys.argv) == 5 and sys.argv[1:3] == ["native", "newer-than"]:
        sys.exit(0 if native_manifest_newer(Path(sys.argv[3]), sys.argv[4]) else 1)
    else:
        prepare(Path(sys.argv[1]), sys.argv[2])
