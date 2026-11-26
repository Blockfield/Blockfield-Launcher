#!/usr/bin/env python3
"""Runs only inside the agent-owned, networkless Xvfb container."""

import json
import os
from pathlib import Path
import subprocess
import time

OUT = Path("/out")
APP = "/app/target/release/examples/slint-preview"


def wait(check, seconds, label):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.05)
    raise RuntimeError(f"Timed out: {label}")


def run(*args):
    return subprocess.check_output(args, text=True, timeout=30).strip()


def main():
    assert os.environ.get("DISPLAY") == ":99"
    os.environ.pop("WAYLAND_DISPLAY", None)
    Path("/tmp/runtime").mkdir(mode=0o700, exist_ok=True)
    children = []
    handles = []

    def launch(name, args):
        log = (OUT / f"{name}.log").open("w")
        handles.append(log)
        child = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT)
        children.append(child)
        return child

    try:
        launch(
            "xvfb",
            ["Xvfb", ":99", "-screen", "0", "640x520x24", "-nolisten", "tcp", "-ac"],
        )
        wait(lambda: Path("/tmp/.X11-unix/X99").exists(), 10, "Xvfb socket")
        launch("wm", ["openbox", "--sm-disable"])
        wait(
            lambda: (
                "_NET_SUPPORTING_WM_CHECK(WINDOW)"
                in run("xprop", "-root", "_NET_SUPPORTING_WM_CHECK")
            ),
            10,
            "window manager",
        )
        mode = os.environ["PREVIEW_MODE"]
        if mode == "self-check":
            app = launch("self-check", [APP, "--self-check", "--idle-seconds", "15"])
            assert app.wait(timeout=20) == 0
            assert "SELF_CHECK passed" in (OUT / "self-check.log").read_text()
            return
        args = [APP, "--idle-seconds", "100"]
        if mode == "capture":
            args += ["--start-time", os.environ["PREVIEW_PHASE"]]
        app = launch(mode, args)
        log = OUT / f"{mode}.log"
        wait(
            lambda: "READY " in log.read_text() or app.poll() is not None,
            20,
            "rendered first frame",
        )
        assert app.poll() is None, log.read_text()
        window = run(
            "xdotool", "search", "--pid", str(app.pid), "--name", "Blockfield"
        ).splitlines()[0]
        run("xdotool", "windowmove", window, "0", "0")
        run("xdotool", "windowsize", window, "320", "420")
        geometry = run("xdotool", "getwindowgeometry", "--shell", window)
        (OUT / "geometry.txt").write_text(geometry)
        coordinates = dict(line.split("=", 1) for line in geometry.splitlines())
        grab = f":99.0+{coordinates['X']},{coordinates['Y']}"
        if mode == "capture":
            recorder = launch(
                "recording",
                [
                    "ffmpeg",
                    "-y",
                    "-loglevel",
                    "warning",
                    "-f",
                    "x11grab",
                    "-framerate",
                    "60",
                    "-video_size",
                    "320x420",
                    "-i",
                    grab,
                    "-t",
                    "3",
                    "-r",
                    "60",
                    "-fps_mode",
                    "cfr",
                    "-enc_time_base",
                    "1:60",
                    "-c:v",
                    "mpeg4",
                    "-q:v",
                    "2",
                    str(OUT / "slint-motion.mp4"),
                ],
            )
            assert recorder.wait(timeout=15) == 0
            run("magick", "import", "-window", window, str(OUT / "visible.png"))
            run(
                "xdotool",
                "mousemove",
                "--window",
                window,
                "160",
                "140",
                "mousedown",
                "1",
                "mousemove",
                "--window",
                window,
                "210",
                "180",
                "mouseup",
                "1",
            )
            time.sleep(0.2)
            run("magick", "import", "-window", window, str(OUT / "dragged.png"))
            run("xdotool", "mousemove", "--window", window, "160", "386", "click", "1")
            wait(
                lambda: "running=false" in log.read_text(), 3, "toggle stops rendering"
            )
            time.sleep(0.3)
            run("magick", "import", "-window", window, str(OUT / "hidden.png"))
            run("xdotool", "mousemove", "--window", window, "160", "386", "click", "1")
            wait(
                lambda: log.read_text().count("running=true") >= 2, 3, "toggle resumes"
            )
            run("xdotool", "windowminimize", window)
            wait(
                lambda: log.read_text().count("running=false") >= 2,
                3,
                "minimize stops rendering",
            )
            time.sleep(0.3)
            run("xdotool", "windowmap", window)
            wait(
                lambda: log.read_text().count("running=true") >= 3, 3, "restore resumes"
            )
        else:
            time.sleep(2)
            for state in ("visible", "hidden", "minimized"):
                if state == "hidden":
                    run(
                        "xdotool",
                        "mousemove",
                        "--window",
                        window,
                        "160",
                        "386",
                        "click",
                        "1",
                    )
                    wait(lambda: "running=false" in log.read_text(), 3, "hidden pacing")
                    time.sleep(2)
                elif state == "minimized":
                    run(
                        "xdotool",
                        "mousemove",
                        "--window",
                        window,
                        "160",
                        "386",
                        "click",
                        "1",
                    )
                    time.sleep(0.3)
                    run("xdotool", "windowminimize", window)
                    wait(
                        lambda: log.read_text().count("running=false") >= 2,
                        3,
                        "minimized pacing",
                    )
                    time.sleep(2)
                subprocess.run(
                    [
                        "python3",
                        "/bench.py",
                        "sample",
                        "--pid",
                        str(app.pid),
                        "--duration",
                        "20",
                        "--interval",
                        "0.5",
                        "--json-out",
                        str(OUT / f"{state}.json"),
                    ],
                    check=True,
                    timeout=30,
                )
            (OUT / "summary.json").write_text(
                json.dumps(
                    {
                        state: json.loads((OUT / f"{state}.json").read_text())["pooled"]
                        for state in ("visible", "hidden", "minimized")
                    },
                    indent=2,
                )
            )
        run("xdotool", "windowmap", window)
        run("xdotool", "windowactivate", "--sync", window)
        run("xdotool", "key", "alt+F4")
        assert app.wait(timeout=10) == 0
        if mode == "capture":
            deterministic = launch(
                "poses",
                [
                    APP,
                    "--capture-dir",
                    str(OUT / "poses"),
                    "--start-time",
                    os.environ["PREVIEW_PHASE"],
                    "--idle-seconds",
                    "15",
                ],
            )
            assert deterministic.wait(timeout=20) == 0
            assert len(list((OUT / "poses").glob("frame-*.png"))) == 4
    finally:
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=3)
        for handle in handles:
            handle.close()


if __name__ == "__main__":
    main()
