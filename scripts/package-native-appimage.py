#!/usr/bin/env python3
"""Package the native Linux launcher binary into a Type-2 AppImage.

Concatenates the pinned official AppImage/type2-runtime ELF stub with a
SquashFS image of a minimal AppDir (no bundled GTK/Mesa/Wayland libraries).
"""

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ICON = REPO_ROOT / "native" / "packaging" / "assets" / "256x256.png"
DEFAULT_DISCORD_APP_ID = "1548300877125779456"
CACHE_DIR = REPO_ROOT / ".cache" / "quality"

# Immutable AppImage/type2-runtime release 20251108.
TYPE2_RUNTIME = {
    "amd64": (
        "appimage-runtime-20251108-x86_64",
        "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64",
        "2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d",
    ),
}

_VERSION_RE = re.compile(r"^[0-9][A-Za-z0-9.+~-]*$")
_ARCH_RE = re.compile(r"^[a-z0-9-]+$")


def desktop_entry(
    version,
    *,
    dev=False,
    discord_app_id=DEFAULT_DISCORD_APP_ID,
):
    pkg = "blockfield-launcher-dev" if dev else "blockfield-launcher"
    name = "Blockfield Launcher DEV" if dev else "Blockfield Launcher"
    exec_line = f"{pkg}" if dev else f"{pkg} %U"
    lines = [
        "[Desktop Entry]",
        "Type=Application",
        f"Name={name}",
        "Comment=Blockfield game launcher",
        f"Exec={exec_line}",
        f"Icon={pkg}",
        "Terminal=false",
        "Categories=Game;",
        "StartupWMClass=blockfield-native",
        f"X-AppImage-Version={version}",
    ]
    if not dev:
        lines.append(
            f"MimeType=x-scheme-handler/blockfield;x-scheme-handler/discord-{discord_app_id};"
        )
    return "\n".join(lines) + "\n"


def apprun_script(*, dev=False):
    pkg = "blockfield-launcher-dev" if dev else "blockfield-launcher"
    return (
        "#!/bin/sh\n"
        "set -eu\n"
        'HERE="$(dirname "$(readlink -f "$0")")"\n'
        f'exec "$HERE/usr/bin/{pkg}" "$@"\n'
    )


def ensure_runtime(arch="amd64", cache_dir=CACHE_DIR):
    if arch not in TYPE2_RUNTIME:
        raise ValueError(f"Unsupported AppImage architecture: {arch!r}")
    name, url, checksum = TYPE2_RUNTIME[arch]
    target = Path(cache_dir) / name
    if target.is_file():
        data = target.read_bytes()
    else:
        with urllib.request.urlopen(url, timeout=60) as response:
            data = response.read()
    if hashlib.sha256(data).hexdigest() != checksum:
        raise RuntimeError(f"AppImage runtime checksum mismatch: {name}")
    if not target.is_file():
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return target


def validate_runtime_bytes(data):
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[8:11] != b"AI\x02":
        raise ValueError("Invalid AppImage type-2 runtime (missing ELF/AI\\x02 header)")


def build_appimage(
    binary_path,
    version,
    output_path,
    *,
    arch="amd64",
    icon_path=DEFAULT_ICON,
    discord_app_id=DEFAULT_DISCORD_APP_ID,
    dev=False,
    runtime_path=None,
    mksquashfs_bin=None,
):
    if not _VERSION_RE.match(version):
        raise ValueError(f"Invalid AppImage version: {version!r}")
    if not _ARCH_RE.match(arch):
        raise ValueError(f"Invalid AppImage architecture: {arch!r}")

    binary = Path(binary_path)
    if not binary.is_file():
        raise FileNotFoundError(f"Missing native launcher binary: {binary}")
    binary_bytes = binary.read_bytes()
    if not binary_bytes:
        raise ValueError(f"Empty native launcher binary: {binary}")

    icon = Path(icon_path)
    if not icon.is_file():
        raise FileNotFoundError(f"Missing launcher icon: {icon}")
    icon_bytes = icon.read_bytes()
    if not icon_bytes:
        raise ValueError(f"Empty launcher icon: {icon}")

    mksquashfs = mksquashfs_bin or shutil.which("mksquashfs")
    if not mksquashfs:
        raise FileNotFoundError("mksquashfs not found on PATH (install squashfs-tools)")

    runtime = (
        Path(runtime_path) if runtime_path is not None else ensure_runtime(arch=arch)
    )
    runtime_bytes = runtime.read_bytes()
    validate_runtime_bytes(runtime_bytes)

    pkg = "blockfield-launcher-dev" if dev else "blockfield-launcher"
    desktop = desktop_entry(version, dev=dev, discord_app_id=discord_app_id).encode(
        "utf-8"
    )
    apprun = apprun_script(dev=dev).encode("utf-8")

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        appdir = tmp_path / "AppDir"
        (appdir / "usr" / "bin").mkdir(parents=True)
        (appdir / "usr" / "share" / "applications").mkdir(parents=True)
        (appdir / "usr" / "share" / "icons" / "hicolor" / "256x256" / "apps").mkdir(
            parents=True
        )

        apprun_file = appdir / "AppRun"
        apprun_file.write_bytes(apprun)
        apprun_file.chmod(0o755)

        bin_file = appdir / "usr" / "bin" / pkg
        bin_file.write_bytes(binary_bytes)
        bin_file.chmod(0o755)

        for desktop_dest in (
            appdir / f"{pkg}.desktop",
            appdir / "usr" / "share" / "applications" / f"{pkg}.desktop",
        ):
            desktop_dest.write_bytes(desktop)
            desktop_dest.chmod(0o644)

        for icon_dest in (
            appdir / f"{pkg}.png",
            appdir / ".DirIcon",
            appdir
            / "usr"
            / "share"
            / "icons"
            / "hicolor"
            / "256x256"
            / "apps"
            / f"{pkg}.png",
        ):
            icon_dest.write_bytes(icon_bytes)
            icon_dest.chmod(0o644)

        squashfs = tmp_path / "payload.squashfs"
        subprocess.run(
            [
                str(mksquashfs),
                str(appdir),
                str(squashfs),
                "-root-owned",
                "-noappend",
                "-no-progress",
                "-comp",
                "gzip",
                "-quiet",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        squashfs_bytes = squashfs.read_bytes()
        if not squashfs_bytes:
            raise RuntimeError("mksquashfs produced an empty payload")

    destination = Path(output_path)
    if destination.is_dir() or destination.suffix != ".AppImage":
        destination.mkdir(parents=True, exist_ok=True)
        destination = destination / f"{pkg}_{version}_{arch}.AppImage"
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(runtime_bytes + squashfs_bytes)
    destination.chmod(0o755)
    return destination


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        required=True,
        help="Path to the built blockfield-native release binary",
    )
    parser.add_argument("--version", required=True, help="Package version (e.g. 2.0.1)")
    parser.add_argument(
        "--output",
        required=True,
        help="Output .AppImage file path or directory",
    )
    parser.add_argument(
        "--arch", default="amd64", help="AppImage architecture (default: amd64)"
    )
    parser.add_argument(
        "--icon",
        default=str(DEFAULT_ICON),
        help="Path to 256x256 PNG application icon",
    )
    parser.add_argument(
        "--discord-app-id",
        default=os.environ.get("BLOCKFIELD_DISCORD_APPLICATION_ID")
        or DEFAULT_DISCORD_APP_ID,
        help="Discord application id for the discord-<id> URI scheme",
    )
    parser.add_argument(
        "--runtime",
        default=None,
        help="Optional local AppImage type-2 runtime path (skips download)",
    )
    parser.add_argument(
        "--dev",
        action="store_true",
        help="Build side-by-side DEV AppImage (no production deep-link registration)",
    )
    args = parser.parse_args(argv)
    out = build_appimage(
        args.binary,
        args.version,
        args.output,
        arch=args.arch,
        icon_path=args.icon,
        discord_app_id=args.discord_app_id,
        dev=args.dev,
        runtime_path=args.runtime,
    )
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
