#!/usr/bin/env python3
"""Check rendered geometry, state changes, pulse motion and bundled font bytes."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys


def identify(path, expression):
    return subprocess.check_output(
        ["magick", str(path), "-format", expression, "info:"], text=True
    ).strip()


def main():
    directory = Path(sys.argv[1])
    scale = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
    names = (
        "shell", "nav-hover", "nav-updates", "keyboard-focus",
        "backdrop-dev-pulse-a", "backdrop-dev-pulse-b", "close-hover", "close-pressed",
    )
    expected_size = f"{round(1280 * scale)}x{round(720 * scale)}"
    for name in names:
        assert identify(directory / f"{name}.png", "%wx%h") == expected_size, name

    def pixel(name, x, y):
        return identify(
            directory / f"{name}.png",
            f"%[pixel:p{{{round(x * scale)},{round(y * scale)}}}]",
        )

    assert pixel("shell", 10, 60) == "srgb(11,9,6)", "header geometry"
    assert pixel("shell", 10, 100) == "srgb(7,6,4)", "empty content slot"
    assert pixel("shell", 10, 700) == "srgb(11,9,6)", "footer geometry"
    assert pixel("close-hover", 1240, 12) == "srgb(90,23,20)", "close hover"
    assert pixel("close-pressed", 1240, 12) == "srgb(90,23,20)", "close pressed"
    assert (directory / "shell.png").read_bytes() != (directory / "nav-updates.png").read_bytes()
    assert (directory / "nav-updates.png").read_bytes() != (directory / "keyboard-focus.png").read_bytes()
    pulse_values = []
    for name in ("backdrop-dev-pulse-a", "backdrop-dev-pulse-b"):
        pulse_values.append(subprocess.check_output(
            ["magick", str(directory / f"{name}.png"), "-crop",
             f"{round(20 * scale)}x{round(20 * scale)}+{round(18 * scale)}+{round(111 * scale)}",
             "-format", "%[fx:mean]", "info:"], text=True
        ).strip())
    assert float(pulse_values[0]) > float(pulse_values[1]), "pulse starts bright and fades before returning"
    fonts = Path(__file__).resolve().parents[1] / "assets/fonts"
    for filename, _family, _weight, _url, digest in json.loads((fonts / "sources.json").read_text()):
        assert hashlib.sha256((fonts / filename).read_bytes()).hexdigest() == digest, filename
    print(f"PASS: {len(names)} captures at {expected_size}; geometry, navigation/focus, close states, pulse, nine font hashes")


if __name__ == "__main__":
    main()
