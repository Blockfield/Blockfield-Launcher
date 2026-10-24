#!/usr/bin/env python3
"""One app run inside the shared, networkless Xvfb/Openbox container."""

import json
import os
import re
import subprocess
import time
from pathlib import Path
from urllib.parse import urlencode

from ab_session import wait
from bench import sample_for

WORK = Path("/capture")


def command(*args, env):
    return subprocess.check_output(args, env=env, text=True, stderr=subprocess.STDOUT)


def main():
    request = json.loads((WORK / "request.json").read_text())
    env = dict(os.environ)
    for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "LD_PRELOAD"):
        env.pop(key, None)
    env.update(
        {
            "DISPLAY": ":97",
            "HOME": "/capture/home",
            "XDG_DATA_HOME": "/capture/data",
            "XDG_CONFIG_HOME": "/capture/config",
            "XDG_CACHE_HOME": "/capture/cache",
            "XDG_RUNTIME_DIR": "/capture/runtime",
            "LIBGL_ALWAYS_SOFTWARE": "1",
            "GALLIUM_DRIVER": "llvmpipe",
            "LP_NUM_THREADS": "4",
            "SLINT_SCALE_FACTOR": "1",
            "WINIT_UNIX_BACKEND": "x11",
            "GDK_BACKEND": "x11",
            "GDK_SCALE": "1",
            "GDK_DPI_SCALE": "1",
            "TZ": "UTC",
            "LANG": "C.UTF-8",
            "WEBKIT_DISABLE_DMABUF_RENDERER": "1",
            "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS": "1",
            "SLINT_BACKEND": request["backend"],
            "SLINT_SKIA_PARTIAL_RENDERING": "1",
            "VK_DRIVER_FILES": "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
        }
    )
    for name in ("home", "data", "config", "cache", "runtime", "game"):
        (WORK / name).mkdir(mode=0o700)
    cfg_path = WORK / "data/com.blockfield.launcher/launcher-config.json"
    cfg_path.parent.mkdir(parents=True)
    cfg_path.write_text(
        json.dumps(
            {
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
        )
    )
    children = []
    try:
        with (WORK / "display.log").open("w") as log:
            children.append(
                subprocess.Popen(
                    [
                        "Xvfb",
                        ":97",
                        "-screen",
                        "0",
                        "1280x720x24",
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
            )
            wait(
                lambda: (
                    subprocess.run(
                        ["xprop", "-root"], env=env, capture_output=True
                    ).returncode
                    == 0
                ),
                10,
                "private Xvfb",
            )
            children.append(
                subprocess.Popen(
                    ["openbox", "--sm-disable"],
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )
            )
            wait(
                lambda: (
                    "WINDOW"
                    in command("xprop", "-root", "_NET_SUPPORTING_WM_CHECK", env=env)
                ),
                10,
                "Openbox WM",
            )
            graphics = command("glxinfo", "-B", env=env)
            if "llvmpipe" not in graphics:
                raise RuntimeError(f"Expected llvmpipe: {graphics}")
            app_env = dict(env)
            if request["app"] == "tauri":
                app_env.update(
                    {
                        "LD_PRELOAD": "/opt/console.so",
                        "BLOCKFIELD_CAPTURE_QUERY": urlencode(
                            {
                                "state": request["state"],
                                "lang": "ru",
                                "scale": 1,
                                "motion": 1,
                            }
                        ),
                    }
                )
                argv = ["/opt/tauri"]
            else:
                argv = ["/opt/slint", "--state", request["state"]]
            with (WORK / "app.log").open("w") as app_log:
                started = time.monotonic()
                app = subprocess.Popen(
                    ["dbus-run-session", "--", *argv],
                    env=app_env,
                    stdout=app_log,
                    stderr=subprocess.STDOUT,
                )
                children.append(app)

                def ready():
                    if app.poll() is not None:
                        raise RuntimeError(
                            f"App exited {app.returncode}: {(WORK / 'app.log').read_text()}"
                        )
                    if request["app"] == "slint":
                        log = (WORK / "app.log").read_text()
                        return (
                            bool(re.search(r"^READY pid=\d+ time=", log, re.MULTILINE))
                            if request["state"] == "settings-skin"
                            else "READY pid=" in log
                        )
                    lang = json.loads(cfg_path.read_text())["lang"]
                    if lang.startswith("capture-failed:"):
                        raise RuntimeError(lang)
                    if lang.startswith("capture-ready:"):
                        proof = json.loads(lang.removeprefix("capture-ready:"))
                        if proof["errors"] or not proof["fonts"]["inter"]:
                            raise RuntimeError(f"Invalid fixture: {proof}")
                        return proof
                    return None

                evidence = wait(ready, 40, "app readiness")
                startup = time.monotonic() - started
                windows = wait(
                    lambda: subprocess.run(
                        [
                            "xdotool",
                            "search",
                            "--onlyvisible",
                            "--name",
                            "^Blockfield Launcher$"
                            if request["app"] == "tauri"
                            else "BLOCKFIELD",
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                    ).stdout.strip(),
                    10,
                    "app window",
                )
                window = windows.splitlines()[0]
                command("xdotool", "windowsize", window, "1280", "720", env=env)
                command("xdotool", "windowmove", window, "0", "0", env=env)
                geometry = command(
                    "xdotool", "getwindowgeometry", "--shell", window, env=env
                )
                if "WIDTH=1280\n" not in geometry or "HEIGHT=720\n" not in geometry:
                    raise RuntimeError(f"Invalid dimensions: {geometry}")
                if request["minimized"]:
                    command("xdotool", "windowminimize", window, env=env)
                    wait(
                        lambda: (
                            "Iconic"
                            in command("xprop", "-id", window, "WM_STATE", env=env)
                        ),
                        5,
                        "Openbox IconicState",
                    )
                time.sleep(request["warmup"])
                if (
                    request["app"] == "slint"
                    and request["state"] == "settings-skin"
                    and request["minimized"]
                ):
                    if "PACING running=false" not in (WORK / "app.log").read_text():
                        raise RuntimeError("Minimized skin preview did not stop pacing")
                samples = (
                    sample_for(app.pid, 0.5, request["duration"])
                    if request["duration"]
                    else []
                )
                if app.poll() is not None or any(
                    not sample["processes"] for sample in samples
                ):
                    raise RuntimeError("App exited during measurement")
                # Only smoke runs capture pixels; readback must not perturb resource samples.
                if request.get("screenshot"):
                    command("import", "-window", "root", "/capture/smoke.png", env=env)
                return {
                    "request": request,
                    "startup_s": startup,
                    "ready": evidence,
                    "geometry": geometry,
                    "wm_state": command("xprop", "-id", window, "WM_STATE", env=env),
                    "graphics": graphics,
                    "cpu_count": os.cpu_count(),
                    "samples": samples,
                }
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
    (WORK / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    raise SystemExit(1 if "error" in result else 0)
