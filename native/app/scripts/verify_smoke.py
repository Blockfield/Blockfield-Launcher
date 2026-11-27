#!/usr/bin/env python3
"""Assertions for the real-data smoke: fails on fixture markers, wrong footer
version, startup crashes, slow window open, or blank screenshots.

Deviations from the first-draft marker list (both match live defaults, so
they cannot tell fixtures apart): "12:00:01" also ships in the real checking
log (update.rs checking_log) and bare "/ ПАТЧ" in the real default patch
("/ ПАТЧ ..."); the gate pins the fixture-exact "/ ПАТЧ 2.75" and the
update-log model is excluded from the --smoke-text dump.
"""

import subprocess
import sys
from pathlib import Path

MARKERS = ("v1.0.19", "CaptureOperator", "127.0.0.1:25565", "/ ПАТЧ 2.75")
WINDOW_OPEN_BUDGET_S = 60


def identify(path, expression):
    return subprocess.check_output(
        ["identify", "-format", expression, str(path)], text=True
    ).strip()


def main():
    directory = Path(sys.argv[1])
    scale = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
    version = sys.argv[3] if len(sys.argv) > 3 else None
    failures = []

    text_path = directory / "ui-text.txt"
    if not text_path.is_file():
        failures.append("missing ui-text.txt")
        text = ""
    else:
        text = text_path.read_text(encoding="utf-8")
    for marker in MARKERS:
        if marker in text:
            failures.append(f"fixture marker in UI text: {marker!r}")
    if version:
        expected = f"ЛАУНЧЕР v{version}"
        actual = next(
            (
                line.split("=", 1)[1]
                for line in text.splitlines()
                if line.startswith("launcher_label=")
            ),
            None,
        )
        if actual != expected:
            failures.append(f"footer version {actual!r} != {expected!r}")
    # Signed-out scope: login/firstrun/init-error, never the main shell.
    flags = {
        line.split("=", 1)[0]: line.split("=", 1)[1]
        for line in text.splitlines()
        if "=" in line and line.startswith("show_")
    }
    if flags and not (
        flags.get("show_login") == "true"
        or flags.get("show_firstrun") == "true"
        or flags.get("show_init_error") == "true"
    ):
        failures.append(f"no signed-out screen reached: {flags!r}")
    if flags.get("show_main") == "true":
        failures.append("signed-in main shell shown without credentials")

    log = (directory / "app.log").read_text(encoding="utf-8", errors="replace")
    if "READY pid=" not in log:
        failures.append("binary never reported READY")
    for token in ("panicked", "panic:", "RUST_BACKTRACE"):
        if token in log:
            failures.append(f"crash token in app.log: {token}")

    try:
        timing = (directory / "timing.txt").read_text(encoding="utf-8")
        opened = int(
            next(
                l.split("=")[1]
                for l in timing.splitlines()
                if l.startswith("window_open_s=")
            )
        )
    except (FileNotFoundError, StopIteration, ValueError):
        failures.append("missing or invalid timing.txt")
        opened = None
    if opened is not None and opened > WINDOW_OPEN_BUDGET_S:
        failures.append(
            f"window opened in {opened}s, budget is {WINDOW_OPEN_BUDGET_S}s"
        )

    shot = directory / f"screen-{sys.argv[2] if len(sys.argv) > 2 else '1'}.png"
    if not shot.is_file():
        failures.append(f"missing screenshot {shot.name}")
    else:
        size = identify(shot, "%wx%h")
        if size != f"{round(1280 * scale)}x{round(720 * scale)}":
            failures.append(f"screenshot size {size}, expected for scale {scale}")
        if identify(shot, "%k") in ("0", "1"):
            failures.append("screenshot is blank (<=1 unique color)")

    if failures:
        print("SMOKE FAIL:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print(
        f"SMOKE PASS scale={scale} open_s={opened} "
        f"markers=absent version={version} shots=non-blank"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
