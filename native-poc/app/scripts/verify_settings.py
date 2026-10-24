#!/usr/bin/env python3
"""Check settings-skin capture geometry, skin-tab state and live 3D preview."""
from pathlib import Path
import subprocess
import sys


def identify(path, expression):
    return subprocess.check_output(["magick", str(path), "-format", expression, "info:"], text=True).strip()


def main():
    directory = Path(sys.argv[1])
    scale = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
    size = f"{round(1280 * scale)}x{round(720 * scale)}"
    assert identify(directory / "settings-skin.png", "%wx%h") == size, "size"

    def pixel(name, x, y):
        return identify(
            directory / name,
            f"%[pixel:p{{{round(x * scale)},{round(y * scale)}}}]",
        )

    # Window chrome / shell header / footer framing matches the main captures.
    assert pixel("settings-skin.png", 10, 60) == "srgb(11,9,6)", "header geometry"
    assert pixel("settings-skin.png", 10, 700) == "srgb(11,9,6)", "footer geometry"
    # Settings panel body and the dark preview panel differ from bare chrome.
    assert pixel("settings-skin.png", 640, 400) == "srgb(17,16,13)", "panel body"
    # Skin tab controls: file row, browse button.
    assert pixel("settings-skin.png", 330, 232) == "srgb(11,9,6)", "skin path field"
    assert pixel("settings-skin.png", 905, 235) == "srgb(17,16,13)", "browse button"
    # The native preview renders a real frame: its crop is never flat panel color.
    colors = subprocess.check_output(
        [
            "magick",
            str(directory / "settings-skin.png"),
            "-crop",
            f"{round(220 * scale)}x{round(300 * scale)}+{round(1011 * scale)}+{round(216 * scale)}",
            "+repage",
            "-format",
            "%k",
            "info:",
        ],
        text=True,
    ).strip()
    assert int(colors) > 50, f"preview frame rendered, got {colors} colors"
    print("PASS settings-skin capture geometry; shell chrome, panel, skin controls, live preview")


if __name__ == "__main__":
    main()
