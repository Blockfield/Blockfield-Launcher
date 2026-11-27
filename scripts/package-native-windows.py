#!/usr/bin/env python3
"""Package the native Windows launcher binary into MSI (WiX v3) and NSIS setup installers."""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGING_DIR = REPO_ROOT / "native" / "packaging" / "windows"
ASSETS_DIR = REPO_ROOT / "native" / "packaging" / "assets"
DEFAULT_ICON = ASSETS_DIR / "icon.ico"
DEFAULT_HEADER = ASSETS_DIR / "header.bmp"
DEFAULT_SIDEBAR = ASSETS_DIR / "sidebar.bmp"
DEFAULT_DISCORD_APP_ID = "1548300877125779456"

OLE_MSI_MAGIC = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"
_SEMVER_CORE_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$")


def normalize_msi_version(version):
    match = _SEMVER_CORE_RE.match(version.strip())
    if not match:
        raise ValueError(f"Version is not semver X.Y.Z: {version!r}")
    major, minor, patch = (int(part) for part in match.groups())
    if major > 255 or minor > 255 or patch > 65535:
        raise ValueError(f"Version out of MSI bounds (255.255.65535): {version!r}")
    return f"{major}.{minor}.{patch}"


def normalize_nsis_quad(version):
    return f"{normalize_msi_version(version)}.0"


def find_wix():
    candle = shutil.which("candle") or shutil.which("candle.exe")
    light = shutil.which("light") or shutil.which("light.exe")
    if candle and light:
        return Path(candle), Path(light)

    candidates = []
    if os.environ.get("WIX"):
        candidates.append(Path(os.environ["WIX"]) / "bin")
    pf86 = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
    if pf86.is_dir():
        candidates.extend(sorted(pf86.glob("WiX Toolset v3*/bin"), reverse=True))
    for directory in candidates:
        c = directory / "candle.exe"
        l_exe = directory / "light.exe"
        if c.is_file() and l_exe.is_file():
            return c, l_exe
    raise FileNotFoundError(
        "WiX Toolset v3 (candle.exe/light.exe) not found on PATH or Program Files (x86)"
    )


def find_makensis():
    found = shutil.which("makensis") or shutil.which("makensis.exe")
    if found:
        return Path(found)
    pf86 = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
    candidate = pf86 / "NSIS" / "makensis.exe"
    if candidate.is_file():
        return candidate
    raise FileNotFoundError(
        "NSIS (makensis.exe) not found on PATH or Program Files (x86)\\NSIS"
    )


def build_windows_installers(
    binary_path,
    version,
    output_dir,
    *,
    icon_path=DEFAULT_ICON,
    header_bmp=DEFAULT_HEADER,
    sidebar_bmp=DEFAULT_SIDEBAR,
    discord_app_id=DEFAULT_DISCORD_APP_ID,
    dev=False,
):
    binary = Path(binary_path).resolve()
    if not binary.is_file():
        raise FileNotFoundError(f"Missing Windows launcher binary: {binary}")
    for asset in (icon_path, header_bmp, sidebar_bmp):
        if not Path(asset).is_file():
            raise FileNotFoundError(f"Missing installer asset: {asset}")

    msi_version = normalize_msi_version(version)
    nsis_quad = normalize_nsis_quad(version)
    out = Path(output_dir).resolve()
    out.mkdir(parents=True, exist_ok=True)

    pkg = "blockfield-launcher-dev" if dev else "blockfield-launcher"
    msi_out = out / f"{pkg}_{version}_x64_en-US.msi"
    exe_out = out / f"{pkg}_{version}_x64-setup.exe"

    candle, light = find_wix()
    makensis = find_makensis()

    candle_args = [
        str(candle),
        "-nologo",
        "-arch",
        "x64",
        f"-dMsiVersion={msi_version}",
        f"-dBinaryPath={binary}",
        f"-dIconPath={Path(icon_path).resolve()}",
        f"-dDiscordAppId={discord_app_id}",
        f"-dDevBuild={'1' if dev else '0'}",
    ]
    with tempfile.TemporaryDirectory() as tmp:
        wixobj = Path(tmp) / "installer.wixobj"
        subprocess.run(
            [
                *candle_args,
                "-out",
                str(wixobj),
                str((PACKAGING_DIR / "installer.wxs").resolve()),
            ],
            check=True,
        )
        subprocess.run(
            [
                str(light),
                "-nologo",
                "-spdb",
                "-out",
                str(msi_out),
                str(wixobj),
            ],
            check=True,
        )

    if not msi_out.read_bytes().startswith(OLE_MSI_MAGIC):
        raise RuntimeError(f"Built MSI lacks OLE compound header: {msi_out}")

    nsis_args = [
        str(makensis),
        "/V2",
        f"/DVERSION={version}",
        f"/DVERSION_QUAD={nsis_quad}",
        f"/DBINARY_PATH={binary}",
        f"/DICON_PATH={Path(icon_path).resolve()}",
        f"/DHEADER_BMP={Path(header_bmp).resolve()}",
        f"/DSIDEBAR_BMP={Path(sidebar_bmp).resolve()}",
        f"/DDISCORD_APP_ID={discord_app_id}",
        f"/DOUT_FILE={exe_out}",
    ]
    if dev:
        nsis_args.append("/DDEV_BUILD=1")
    nsis_args.append(str((PACKAGING_DIR / "installer.nsi").resolve()))
    subprocess.run(nsis_args, check=True)

    if not exe_out.read_bytes().startswith(b"MZ"):
        raise RuntimeError(f"Built NSIS setup lacks PE MZ header: {exe_out}")

    return msi_out, exe_out


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        required=True,
        help="Path to the built blockfield-native.exe release binary",
    )
    parser.add_argument("--version", required=True, help="Package version (e.g. 2.0.1)")
    parser.add_argument(
        "--output",
        required=True,
        help="Output directory for .msi and -setup.exe installers",
    )
    parser.add_argument(
        "--discord-app-id",
        default=os.environ.get("BLOCKFIELD_DISCORD_APPLICATION_ID")
        or DEFAULT_DISCORD_APP_ID,
        help="Discord application id for the discord-<id> URI scheme",
    )
    parser.add_argument(
        "--dev",
        action="store_true",
        help="Build side-by-side DEV installers (no production deep-link registration)",
    )
    args = parser.parse_args(argv)
    msi_out, exe_out = build_windows_installers(
        args.binary,
        args.version,
        args.output,
        discord_app_id=args.discord_app_id,
        dev=args.dev,
    )
    print(msi_out)
    print(exe_out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
