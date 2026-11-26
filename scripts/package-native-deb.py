#!/usr/bin/env python3
"""Package the native Linux launcher binary into a Debian .deb archive.

Produces an ar archive with `debian-binary`, `control.tar.gz` and `data.tar.gz`
matching the exact layout `native/app/src/update.rs` extracts (`data.tar.gz`
with `usr/bin/blockfield-native`, gzip-compressed GNU tar).
"""

import argparse
import gzip
import io
import math
import os
import re
import sys
import tarfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ICON = REPO_ROOT / "src-tauri" / "icons" / "256x256.png"
PACKAGE_NAME = "blockfield-native"
DEFAULT_DISCORD_APP_ID = "1548300877125779456"

_VERSION_RE = re.compile(r"^[0-9][A-Za-z0-9.+~-]*$")
_ARCH_RE = re.compile(r"^[a-z0-9-]+$")


def desktop_entry(discord_app_id=DEFAULT_DISCORD_APP_ID):
    return (
        "[Desktop Entry]\n"
        "Type=Application\n"
        "Name=Blockfield Launcher\n"
        "Comment=Blockfield game launcher\n"
        "Exec=/usr/bin/blockfield-native %U\n"
        "Icon=blockfield-native\n"
        "Terminal=false\n"
        "Categories=Game;\n"
        "StartupWMClass=blockfield-native\n"
        f"MimeType=x-scheme-handler/blockfield;x-scheme-handler/discord-{discord_app_id};\n"
    )


def control_text(version, arch, installed_bytes):
    installed_kb = max(1, math.ceil(installed_bytes / 1024))
    return (
        f"Package: {PACKAGE_NAME}\n"
        f"Version: {version}\n"
        f"Architecture: {arch}\n"
        "Maintainer: Blockfield <support@blockfield.pro>\n"
        "Section: games\n"
        "Priority: optional\n"
        f"Installed-Size: {installed_kb}\n"
        "Depends: libc6, libfontconfig1, libgl1, libxkbcommon-x11-0\n"
        "Homepage: https://blockfield.pro\n"
        "Description: Blockfield Native Launcher\n"
        " Native Slint launcher for Blockfield.\n"
    )


def _tar_gz(entries, mtime):
    buf = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buf, mtime=mtime) as gz:
        with tarfile.open(fileobj=gz, mode="w", format=tarfile.GNU_FORMAT) as tar:
            for name, mode, payload in entries:
                info = tarfile.TarInfo(name=name)
                info.uid = 0
                info.gid = 0
                info.uname = "root"
                info.gname = "root"
                info.mtime = mtime
                info.mode = mode
                if payload is None:
                    info.type = tarfile.DIRTYPE
                    info.size = 0
                    tar.addfile(info)
                else:
                    info.type = tarfile.REGTYPE
                    info.size = len(payload)
                    tar.addfile(info, io.BytesIO(payload))
    return buf.getvalue()


def _ar_archive(members, mtime):
    out = bytearray(b"!<arch>\n")
    for name, payload in members:
        if len(name) > 16:
            raise ValueError(f"ar member name too long: {name!r}")
        header = (
            f"{name:<16}{mtime:<12}0     0     100644  {len(payload):<10}`\n"
        ).encode("ascii")
        if len(header) != 60:
            raise ValueError(f"Invalid ar header length {len(header)} for {name!r}")
        out.extend(header)
        out.extend(payload)
        if len(payload) % 2:
            out.extend(b"\n")
    return bytes(out)


def build_deb(
    binary_path,
    version,
    output_path,
    *,
    arch="amd64",
    icon_path=DEFAULT_ICON,
    discord_app_id=DEFAULT_DISCORD_APP_ID,
    mtime=None,
):
    if not _VERSION_RE.match(version):
        raise ValueError(f"Invalid Debian package version: {version!r}")
    if not _ARCH_RE.match(arch):
        raise ValueError(f"Invalid Debian architecture: {arch!r}")

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

    if mtime is None:
        mtime = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))

    desktop_bytes = desktop_entry(discord_app_id).encode("utf-8")
    control_bytes = control_text(
        version, arch, len(binary_bytes) + len(desktop_bytes) + len(icon_bytes)
    ).encode("utf-8")

    control_tar = _tar_gz([("./control", 0o644, control_bytes)], mtime)
    data_tar = _tar_gz(
        [
            ("./usr/", 0o755, None),
            ("./usr/bin/", 0o755, None),
            ("./usr/bin/blockfield-native", 0o755, binary_bytes),
            ("./usr/share/", 0o755, None),
            ("./usr/share/applications/", 0o755, None),
            (
                "./usr/share/applications/blockfield-native.desktop",
                0o644,
                desktop_bytes,
            ),
            ("./usr/share/icons/", 0o755, None),
            ("./usr/share/icons/hicolor/", 0o755, None),
            ("./usr/share/icons/hicolor/256x256/", 0o755, None),
            ("./usr/share/icons/hicolor/256x256/apps/", 0o755, None),
            (
                "./usr/share/icons/hicolor/256x256/apps/blockfield-native.png",
                0o644,
                icon_bytes,
            ),
        ],
        mtime,
    )
    deb_bytes = _ar_archive(
        [
            ("debian-binary", b"2.0\n"),
            ("control.tar.gz", control_tar),
            ("data.tar.gz", data_tar),
        ],
        mtime,
    )

    destination = Path(output_path)
    if destination.is_dir() or destination.suffix != ".deb":
        destination.mkdir(parents=True, exist_ok=True)
        destination = destination / f"{PACKAGE_NAME}_{version}_{arch}.deb"
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(deb_bytes)
    return destination


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        required=True,
        help="Path to the built blockfield-native release binary",
    )
    parser.add_argument(
        "--version", required=True, help="Package version (e.g. 1.0.31)"
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Output .deb file path or directory (writes blockfield-native_<version>_<arch>.deb)",
    )
    parser.add_argument(
        "--arch", default="amd64", help="Debian architecture (default: amd64)"
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
    args = parser.parse_args(argv)
    out = build_deb(
        args.binary,
        args.version,
        args.output,
        arch=args.arch,
        icon_path=args.icon,
        discord_app_id=args.discord_app_id,
    )
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
