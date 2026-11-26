#!/usr/bin/env python3
"""A/B sampler for the Tauri launcher: same isolated container conditions as
scripts/capture/session.py (Xvfb 1280x720, llvmpipe env, capture query with
motion=1 so CSS animations run), but instead of a screenshot it samples the
whole process tree for --duration seconds after --warmup, via /bench.py."""

import json
import os
import subprocess
import time
from pathlib import Path
from urllib.parse import urlencode

WORK = Path("/capture")
DUR = float(os.environ.get("AB_DURATION", "30"))
WARMUP = float(os.environ.get("AB_WARMUP", "5"))


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
        "username": "CaptureOperator",
        "discordPresence": False,
        "preLaunchCommand": "",
        "postExitCommand": "",
        "hideWhilePlaying": False,
    }
    cfg_path.write_text(json.dumps(cfg))
    flags = subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "webkit2gtk-4.1"], text=True
    ).split()
    subprocess.run(
        [
            "gcc",
            "-shared",
            "-fPIC",
            "/harness/console.c",
            "-o",
            "/capture/console.so",
            *flags,
            "-ldl",
        ],
        check=True,
    )
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
                app_env = {
                    **env,
                    "LD_PRELOAD": "/capture/console.so",
                    "BLOCKFIELD_CAPTURE_QUERY": urlencode(
                        {key: request[key] for key in ("state", "lang", "scale")}
                        | {"motion": 1}
                    ),
                }
                app = subprocess.Popen(
                    ["dbus-run-session", "--", "/opt/blockfield-launcher"],
                    env=app_env,
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
                if os.environ.get("AB_HIDDEN") == "1":
                    subprocess.run(
                        ["xdotool", "windowunmap", window],
                        env=env,
                        check=True,
                    )

                def ready():
                    if "Unhandled Promise Rejection" in (WORK / "app.log").read_text():
                        raise RuntimeError((WORK / "app.log").read_text())
                    if (
                        "VITE_BLOCKFIELD_PACK_URL is required for production"
                        in (WORK / "app.log").read_text()
                    ):
                        raise RuntimeError(
                            "Capture build omitted VITE_BLOCKFIELD_PACK_URL; frontend cannot initialize"
                        )
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
                if evidence["errors"]:
                    raise RuntimeError(
                        f"Frontend errors invalidate capture: {evidence['errors']}"
                    )
                if not evidence["fonts"]["inter"]:
                    raise RuntimeError("Inter check failed")
                time.sleep(WARMUP)
                bench = subprocess.run(
                    [
                        "python3",
                        "/bench.py",
                        "sample",
                        "--pid",
                        str(app.pid),
                        "--duration",
                        str(DUR),
                        "--interval",
                        "0.5",
                        "--json-out",
                        "/capture/bench.json",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=DUR + 30,
                )
                if bench.returncode != 0:
                    raise RuntimeError(f"bench sample failed: {bench.stderr[-2000:]}")
                evidence["physicalSize"] = {"width": width, "height": height}
                evidence["glxinfo"] = subprocess.run(
                    ["glxinfo", "-B"],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                ).stdout
                try:
                    with open("/proc/cpuinfo", encoding="utf-8") as handle:
                        evidence["containerCpus"] = sum(
                            1 for line in handle if line.startswith("processor")
                        )
                except OSError:
                    evidence["containerCpus"] = None
                evidence["bench"] = json.loads(Path("/capture/bench.json").read_text())
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
