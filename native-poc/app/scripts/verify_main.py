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
    # WebKit's physical-pixel rounding must not accumulate across flex panels.
    w = round(1280 * scale)
    pixels = subprocess.check_output(["magick", str(directory / "main.png"), "-depth", "8", "rgb:-"])
    play, details, footer, underline = {
        1.0: (247, 369, 687, 417),
        1.25: (307, 459, 859, 519),
        1.5: (368, 550, 1031, 622),
    }[scale]
    for x, y, color in [
        (600, play, (42, 33, 22)),
        (600, details, (42, 33, 22)),
        (800, footer, (24, 19, 13)),
        (50, underline, (245, 165, 36)),
    ]:
        column = round(x * scale)
        assert any(
            tuple(pixels[((row * w + column) * 3):((row * w + column) * 3 + 3)]) == color
            for row in range(y - 1, y + 2)
        ), ("border exceeds 1 physical pixel from baseline", x, y, color)
    dash_y = {1.0: 441, 1.25: 548, 1.5: 657}[scale]
    dash_colors = [
        tuple(pixels[(dash_y * w + x) * 3:(dash_y * w + x) * 3 + 3])
        for x in range(round(820 * scale), round(1000 * scale))
    ]
    starts = [i for i in range(1, len(dash_colors)) if dash_colors[i] == (24, 19, 13) and dash_colors[i - 1] != (24, 19, 13)]
    assert len(starts) > 10 and all(5 <= b - a <= 7 for a, b in zip(starts, starts[1:])), "package dashes must keep a 6-physical-pixel period"
    print(f"PASS: {len(names)} MainScreen captures at {size}; chrome/footer/panel geometry, briefing state")


if __name__ == "__main__":
    main()
