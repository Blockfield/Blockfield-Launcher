#!/usr/bin/env bash
set -euo pipefail

sudo python3 - <<'PYTHON'
from pathlib import Path

sources = [Path("/etc/apt/sources.list")]
for pattern in ("*.list", "*.sources"):
    sources.extend(Path("/etc/apt/sources.list.d").glob(pattern))
for path in sources:
    if not path.is_file():
        continue
    original = path.read_text()
    updated = original
    for host in ("archive.ubuntu.com", "us.archive.ubuntu.com", "security.ubuntu.com", "mirrors.sonic.net"):
        updated = updated.replace(f"http://{host}/ubuntu", f"https://{host}/ubuntu")
    if updated != original:
        path.write_text(updated)
PYTHON

apt_options=(-o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)
sudo env DEBIAN_FRONTEND=noninteractive apt-get "${apt_options[@]}" update
sudo env DEBIAN_FRONTEND=noninteractive apt-get "${apt_options[@]}" install --yes "$@"
