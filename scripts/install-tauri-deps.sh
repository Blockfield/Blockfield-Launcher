#!/usr/bin/env bash
set -euo pipefail

# Prefer https (incl. the azure mirror that stalled CI); retry switches away from azure.
sudo python3 - <<'PYTHON'
from pathlib import Path

sources = [Path("/etc/apt/sources.list"), Path("/etc/apt/blacksmith-ubuntu-mirrors.txt")]
for pattern in ("*.list", "*.sources"):
    sources.extend(Path("/etc/apt/sources.list.d").glob(pattern))
for path in sources:
    if not path.is_file():
        continue
    original = path.read_text()
    updated = original
    for host in ("archive.ubuntu.com", "us.archive.ubuntu.com", "azure.archive.ubuntu.com", "security.ubuntu.com", "mirrors.sonic.net"):
        updated = updated.replace(f"http://{host}/ubuntu", f"https://{host}/ubuntu")
    if updated != original:
        path.write_text(updated)
PYTHON

shopt -s nullglob
CACHE_DIR="${APT_ARCHIVES_CACHE:-$HOME/.cache/apt-archives}"
cached=("$CACHE_DIR"/*.deb)
if [ "${#cached[@]}" -gt 0 ]; then
    sudo cp -n "$CACHE_DIR"/*.deb /var/cache/apt/archives/
fi

APT_TIMEOUT="${APT_TIMEOUT:-300}"
apt_options=(-o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)
apt_update() {
    sudo env DEBIAN_FRONTEND=noninteractive timeout "$APT_TIMEOUT" apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update
}
apt_install() {
    sudo env DEBIAN_FRONTEND=noninteractive timeout "$APT_TIMEOUT" apt-get "${apt_options[@]}" install --yes "$@"
}
switch_azure_mirror() {
    sudo python3 - <<'PYEOF_INNER'
from pathlib import Path
sources = [Path("/etc/apt/sources.list"), Path("/etc/apt/blacksmith-ubuntu-mirrors.txt")]
for pattern in ("*.list", "*.sources"):
    sources.extend(Path("/etc/apt/sources.list.d").glob(pattern))
for path in sources:
    if not path.is_file():
        continue
    original = path.read_text()
    updated = original.replace("http://azure.archive.ubuntu.com", "https://archive.ubuntu.com").replace("https://azure.archive.ubuntu.com", "https://archive.ubuntu.com")
    if updated != original:
        path.write_text(updated)
PYEOF_INNER
}

if ! apt_update; then
    switch_azure_mirror
    apt_update
fi
if ! apt_install "$@"; then
    switch_azure_mirror
    apt_install "$@"
fi

mkdir -p "$CACHE_DIR"
debs=(/var/cache/apt/archives/*.deb)
if [ "${#debs[@]}" -gt 0 ]; then
    sudo cp -n /var/cache/apt/archives/*.deb "$CACHE_DIR"/
    sudo chown "$(id -u):$(id -g)" "$CACHE_DIR"/*.deb
fi
