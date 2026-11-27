#!/usr/bin/env python3
"""Read and validate the launcher version from native/app/Cargo.toml."""

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "native" / "app" / "Cargo.toml"
_SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def read_version(root=ROOT):
    data = tomllib.loads((Path(root) / "native" / "app" / "Cargo.toml").read_text())
    version = str(data.get("package", {}).get("version", "")).strip()
    if not _SEMVER_RE.match(version):
        raise ValueError(
            f"Invalid launcher version in native/app/Cargo.toml: {version!r}"
        )
    return version


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--print",
        action="store_true",
        dest="print_only",
        help="Print only the version string",
    )
    parser.add_argument(
        "--tag",
        default=None,
        help="Assert tag equals launcher-v<version>",
    )
    args = parser.parse_args(argv)
    version = read_version()
    if args.tag is not None:
        expected = f"launcher-v{version}"
        if args.tag != expected:
            print(
                f"Tag {args.tag!r} does not match native/app/Cargo.toml version {version!r} (expected {expected!r})",
                file=sys.stderr,
            )
            return 1
    if args.print_only:
        print(version)
    else:
        print(f"Launcher version {version} is valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
