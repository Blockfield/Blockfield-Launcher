#!/usr/bin/env python3
"""Cache the same Google Fonts faces as fonts.css; never change production CSS."""

import hashlib
import json
from pathlib import Path
import re
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / ".cache/capture/fonts"
URL = (
    "https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600;700"
    "&family=Inter:wght@400;500;600;700;800&subset=latin,cyrillic&display=swap"
)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    request = Request(URL, headers={"User-Agent": "Mozilla/5.0"})
    with urlopen(request, timeout=60) as response:
        css = response.read().decode()
    sources = []
    for url in sorted(
        set(re.findall(r"url\((https://fonts.gstatic.com/[^)]+)\)", css))
    ):
        name = hashlib.sha256(url.encode()).hexdigest()[:20] + ".woff2"
        with urlopen(url, timeout=60) as response:
            data = response.read()
        (OUT / name).write_bytes(data)
        css = css.replace(url, f"/capture-fonts/{name}")
        sources.append(
            {"url": url, "file": name, "sha256": hashlib.sha256(data).hexdigest()}
        )
    assert "Inter" in css and "IBM Plex Mono" in css and sources
    (OUT / "fonts.css").write_text(css)
    (OUT / "sources.json").write_text(
        json.dumps({"stylesheet": URL, "files": sources}, indent=2)
    )
    print(f"Cached {len(sources)} exact font resources in {OUT}")


if __name__ == "__main__":
    main()
