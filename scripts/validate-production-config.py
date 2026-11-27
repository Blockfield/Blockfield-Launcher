#!/usr/bin/env python3
"""Validate production pack URLs and the native updater release endpoint."""

import ipaddress
import os
import re
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
RELEASES_PREFIX = "https://github.com/Blockfield/Blockfield-Launcher/releases/"
_ENDPOINT_RE = re.compile(
    r'pub\s+const\s+UPDATE_ENDPOINT\s*:\s*&str\s*=\s*"([^"]+)"\s*;'
)


def validate_production_url(value, name="VITE_BLOCKFIELD_PACK_URL"):
    raw = (value or "").strip()
    if not raw:
        raise ValueError(f"{name} is required")
    parsed = urlparse(raw)
    if parsed.scheme != "https":
        raise ValueError("Production pack must use HTTPS")
    if parsed.username or parsed.password:
        raise ValueError("Production pack URL must not contain credentials")
    host = (parsed.hostname or "").lower()
    if not host:
        raise ValueError(f"{name} must include a host")
    if host == "localhost" or host.endswith(".localhost") or host.endswith(".local"):
        raise ValueError("Production pack host is local or private")
    try:
        ip = ipaddress.ip_address(host)
    except ValueError:
        ip = None
    if ip is not None and (
        ip.is_private
        or ip.is_loopback
        or ip.is_link_local
        or ip.is_reserved
        or ip.is_unspecified
    ):
        raise ValueError("Production pack host is local or private")
    return f"{parsed.scheme}://{parsed.netloc}"


def validate_updater_endpoint(root=ROOT):
    update_rs = Path(root) / "native" / "app" / "src" / "update.rs"
    text = update_rs.read_text(encoding="utf-8")
    match = _ENDPOINT_RE.search(text)
    if not match:
        raise ValueError("UPDATE_ENDPOINT constant missing in native/app/src/update.rs")
    endpoint = match.group(1)
    if not endpoint.startswith(RELEASES_PREFIX):
        raise ValueError(
            f"Production updater endpoint must live in the public releases repo: {RELEASES_PREFIX}"
        )
    return endpoint


def main():
    origin = validate_production_url(os.environ.get("VITE_BLOCKFIELD_PACK_URL"))
    workshop = os.environ.get("VITE_BLOCKFIELD_WORKSHOP_PACK_URL", "").strip()
    if workshop:
        validate_production_url(workshop, "VITE_BLOCKFIELD_WORKSHOP_PACK_URL")
    validate_updater_endpoint()
    print(f"Production pack configuration valid: {origin}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ValueError as error:
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)
