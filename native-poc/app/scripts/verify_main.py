#!/usr/bin/env python3
"""Check MainScreen capture geometry and visible interactive state changes."""
from pathlib import Path
import subprocess
import sys


def identify(path, expression):
    return subprocess.check_output(["magick", str(path), "-format", expression, "info:"], text=True).strip()


def main():
    directory = Path(sys.argv[1])
    scale = float(sys.argv[2])
    names = ["main", "deploy-hover", "room-next", "briefing", "room-select", "verifying", "keyboard-select", "launch-after-click"]
    size = f"{round(1280 * scale)}x{round(720 * scale)}"
    for name in names:
        assert identify(directory / f"{name}.png", "%wx%h") == size, name
    assert (directory / "main.png").read_bytes() != (directory / "briefing.png").read_bytes(), "briefing content changes"
    for x, y, expected in [(10, 60, "srgb(11,9,6)"), (10, 700, "srgb(11,9,6)"), (790, 500, "srgb(11,9,6)")]:
        pixel = identify(directory / "main.png", f"%[pixel:p{{{round(x*scale)},{round(y*scale)}}}]")
        assert pixel == expected, (x, y, pixel)
    print(f"PASS: {len(names)} MainScreen captures at {size}; chrome/footer/panel geometry, briefing state")


if __name__ == "__main__":
    main()
