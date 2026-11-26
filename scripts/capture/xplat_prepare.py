#!/usr/bin/env python3
"""Stage a disposable capture source tree; never patch production source/config."""

import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def prepare(destination):
    destination = destination.resolve()
    if destination.exists():
        raise RuntimeError(f"Refusing to overwrite {destination}")
    for name in (
        subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT)
        .decode()
        .split("\0")
    ):
        if not name:
            continue
        source = ROOT / name
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    fonts = ROOT / ".cache/capture/fonts"
    shutil.copyfile(fonts / "fonts.css", destination / "src/styles/fonts.css")
    shutil.copytree(fonts, destination / "public/capture-fonts")
    config_path = destination / "src-tauri/tauri.conf.json"
    config = json.loads(config_path.read_text())
    # Separate app identity keeps native bootstrap writes away from installed profiles.
    config["identifier"] = "com.blockfield.capture"
    config["app"]["windows"][0]["center"] = True
    config["bundle"]["active"] = False
    config["bundle"]["createUpdaterArtifacts"] = False
    config_path.write_text(json.dumps(config, indent=2))
    capability = destination / "src-tauri/capabilities/default.json"
    config = json.loads(capability.read_text())
    config["permissions"].append("core:webview:allow-set-webview-zoom")
    capability.write_text(json.dumps(config, indent=2))


if __name__ == "__main__":
    prepare(Path(sys.argv[1]))
