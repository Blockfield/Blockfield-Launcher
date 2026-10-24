#!/usr/bin/env python3
"""Container-only supervisor. The owning agent-process supervises this whole tree."""

import json
import os
from pathlib import Path
import subprocess
import time

WORK = Path("/capture")


def wait(check, seconds, label):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.05)
    raise RuntimeError(f"Timed out waiting for {label}")


def main():
    request = json.loads((WORK / "request.json").read_text())
    width = round(request["width"] * request["scale"])
    height = round(request["height"] * request["scale"])
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("DBUS_SESSION_BUS_ADDRESS", None)
    env.update(
        {
            "DISPLAY": ":97",
            "HOME": "/capture/home",
            "XDG_DATA_HOME": "/capture/data",
            "XDG_CONFIG_HOME": "/capture/config",
            "XDG_CACHE_HOME": "/capture/cache",
            "XDG_RUNTIME_DIR": "/capture/runtime",
            "GDK_BACKEND": "x11",
            "GDK_SCALE": "1",
            "GDK_DPI_SCALE": "1",
            "LIBGL_ALWAYS_SOFTWARE": "1",
            "GALLIUM_DRIVER": "llvmpipe",
            "WEBKIT_DISABLE_DMABUF_RENDERER": "1",
            "TZ": "UTC",
            "LANG": "C.UTF-8",
            "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS": "1",
        }
    )
    for name in ("home", "data", "config", "cache", "runtime", "game"):
        (WORK / name).mkdir(mode=0o700, exist_ok=True)
    cfg_path = WORK / "data/com.blockfield.launcher/launcher-config.json"
    cfg_path.parent.mkdir(parents=True)
    cfg = {
        "gameDir": "/capture/game",
        "javaPath": "",
        "ramMb": 4096,
        "autoUpdate": False,
        "lang": "ru",
        "username": json.dumps(request),
        "discordPresence": False,
        "preLaunchCommand": "",
        "postExitCommand": "",
        "hideWhilePlaying": False,
    }
    cfg_path.write_text(json.dumps(cfg))
    children = []
    try:
        with (WORK / "display.log").open("w") as log:
            display = subprocess.Popen(
                [
                    "Xvfb",
                    ":97",
                    "-screen",
                    "0",
                    f"{width}x{height}x24",
                    "-dpi",
                    "96",
                    "-nolisten",
                    "tcp",
                    "-ac",
                ],
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            children.append(display)
            wait(
                lambda: (
                    subprocess.run(
                        ["xdpyinfo"], env=env, capture_output=True
                    ).returncode
                    == 0
                ),
                10,
                "isolated Xvfb :97",
            )
            with (WORK / "app.log").open("w") as app_log:
                app = subprocess.Popen(
                    ["dbus-run-session", "--", "/opt/blockfield-launcher"],
                    env=env,
                    stdout=app_log,
                    stderr=subprocess.STDOUT,
                )
                children.append(app)

                def window_id():
                    if app.poll() is not None:
                        raise RuntimeError(
                            f"Launcher exited {app.returncode}: {(WORK / 'app.log').read_text()}"
                        )
                    result = subprocess.run(
                        [
                            "xdotool",
                            "search",
                            "--onlyvisible",
                            "--name",
                            "^Blockfield Launcher$",
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    return (
                        result.stdout.strip().splitlines()[0]
                        if result.stdout.strip()
                        else None
                    )

                window = wait(window_id, 20, "launcher window")
                subprocess.run(
                    ["xdotool", "windowsize", window, str(width), str(height)],
                    env=env,
                    check=True,
                )
                subprocess.run(
                    ["xdotool", "windowmove", window, "0", "0"], env=env, check=True
                )

                def ready():
                    lang = json.loads(cfg_path.read_text()).get("lang", "")
                    if lang.startswith("capture-failed:"):
                        raise RuntimeError(lang)
                    if app.poll() is not None:
                        raise RuntimeError(f"Launcher exited {app.returncode}")
                    return (
                        json.loads(lang.removeprefix("capture-ready:"))
                        if lang.startswith("capture-ready:")
                        else None
                    )

                evidence = wait(ready, 35, "fixture state and font proof")
                if not evidence["fonts"]["inter"]:
                    raise RuntimeError("Inter check failed")
                time.sleep(0.2)
                grab = [
                    "ffmpeg",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "x11grab",
                    "-draw_mouse",
                    "0",
                    "-video_size",
                    f"{width}x{height}",
                    "-framerate",
                    "60",
                    "-i",
                    ":97+0,0",
                ]
                subprocess.run(
                    [
                        *grab,
                        "-frames:v",
                        "1",
                        "-threads",
                        "1",
                        str(WORK / "capture.png"),
                    ],
                    env=env,
                    check=True,
                    timeout=15,
                )
                if request["seconds"]:
                    subprocess.run(
                        [
                            *grab,
                            "-t",
                            str(request["seconds"]),
                            "-c:v",
                            "libx264",
                            "-preset",
                            "ultrafast",
                            "-crf",
                            "0",
                            "-pix_fmt",
                            "yuv444p",
                            "-threads",
                            "2",
                            str(WORK / "capture.mp4"),
                        ],
                        env=env,
                        check=True,
                        timeout=request["seconds"] + 15,
                    )
                evidence["glxinfo"] = subprocess.run(
                    ["glxinfo", "-B"],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                ).stdout
                evidence["physicalSize"] = {"width": width, "height": height}
                evidence["webKitSandbox"] = (
                    "disabled only inside isolated rootless networkless capture container"
                )
                return evidence
    finally:
        for process in reversed(children):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


if __name__ == "__main__":
    try:
        result = main()
    except Exception as error:
        result = {"error": str(error)}
    temporary = WORK / "result.tmp"
    temporary.write_text(json.dumps(result, indent=2))
    temporary.rename(WORK / "result.json")
