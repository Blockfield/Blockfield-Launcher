#!/usr/bin/env python3
"""Run one isolated release capture and append its evidence to manifest.json."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
STATES = (
    "first-run",
    "login",
    "main",
    "main-launching",
    "update-42",
    "stats",
    "settings-general",
    "settings-runtime",
    "settings-commands",
    "settings-skin",
    "settings-launcher",
    "offline",
    "error",
)


def arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("state", choices=STATES)
    parser.add_argument("--size", default="1280x720", help="logical viewport size")
    parser.add_argument("--scale", type=float, choices=(1, 1.25, 1.5), default=1)
    parser.add_argument("--lang", choices=("ru",), default="ru")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--record", type=float, default=0, metavar="SECONDS")
    parser.add_argument("--fps", type=int, default=60)
    parser.add_argument(
        "--validate-build",
        action="store_true",
        help="validate a new build with a main/RU/1x smoke capture",
    )
    args = parser.parse_args(argv)
    try:
        width, height = map(int, args.size.split("x"))
        assert 1280 <= width <= 2560 and 720 <= height <= 1440
        assert 0 <= args.record <= 60 and args.fps == 60
    except (ValueError, AssertionError):
        parser.error("Use 1280x720..2560x1440, 0..60 seconds, and --fps 60")
    if args.validate_build and (
        args.state != "main"
        or args.scale != 1
        or args.record
        or args.size != "1280x720"
    ):
        parser.error("--validate-build requires main at 1280x720@1x without recording")
    args.width, args.height = width, height
    return args


def run(args):
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    cache = ROOT / ".cache/capture"
    build = json.loads((cache / "build.json").read_text())
    if build.get("valid") is not True and not args.validate_build:
        raise SystemExit(
            f"Capture binary is not validated: {build.get('failure', 'rebuild with build.sh')}"
        )
    stem = f"{args.state}-{args.size}@{args.scale:g}x-{args.lang}" + (
        "-motion" if args.record else ""
    )
    if (args.out / f"{stem}.json").exists():
        raise SystemExit(
            f"Refusing to overwrite capture: {stem}; choose a different --out"
        )
    with tempfile.TemporaryDirectory(prefix="blockfield-capture-") as temporary:
        scratch = Path(temporary)
        request = {
            "state": args.state,
            "lang": args.lang,
            "scale": args.scale,
            "motion": bool(args.record),
            "width": args.width,
            "height": args.height,
            "seconds": args.record,
            "fps": args.fps,
        }
        (scratch / "request.json").write_text(json.dumps(request))
        command = [
            "podman",
            "run",
            "--rm",
            "--name",
            f"blockfield-capture-{uuid.uuid4().hex[:12]}",
            "--userns",
            "keep-id",
            "--network",
            "none",
            "--shm-size",
            "512m",
            "-v",
            f"{cache / 'cargo-target/release/blockfield-launcher'}:/opt/blockfield-launcher:ro,Z",
            "-v",
            f"{ROOT / 'scripts/capture'}:/harness:ro,Z",
            "-v",
            f"{scratch}:/capture:Z",
            "localhost/blockfield-capture:latest",
            "timeout",
            "85",
            "python3",
            "/harness/session.py",
        ]
        registered = json.loads(
            subprocess.check_output(
                [
                    "agent-process",
                    "start",
                    "--name",
                    "blockfield-capture-session",
                    "--cwd",
                    str(ROOT),
                    "--",
                    *command,
                ],
                text=True,
            )
        )
        try:
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                if (scratch / "result.json").exists():
                    break
                time.sleep(0.25)
            else:
                raise RuntimeError(
                    f"Capture deadline exceeded; process log: {registered['log']}"
                )
            result = json.loads((scratch / "result.json").read_text())
            for name in ("app.log", "display.log", "failure.png"):
                if (scratch / name).exists():
                    (args.out / f"{stem}-{name}").write_bytes(
                        (scratch / name).read_bytes()
                    )
            if "error" in result:
                (args.out / f"{stem}-failure.json").write_text(
                    json.dumps(result, indent=2)
                )
                raise RuntimeError(result["error"])
            import shutil

            for name in ("capture.png", "capture.mp4"):
                if (scratch / name).exists():
                    shutil.copyfile(
                        scratch / name, args.out / f"{stem}{Path(name).suffix}"
                    )
            if args.validate_build:
                build["valid"] = True
                build.pop("failure", None)
                build["smokeValidated"] = True
                (cache / "build.json").write_text(json.dumps(build, indent=2) + "\n")
            artifacts = {
                f"{stem}{Path(name).suffix}": hashlib.sha256(
                    (scratch / name).read_bytes()
                ).hexdigest()
                for name in ("capture.png", "capture.mp4")
                if (scratch / name).exists()
            }
            result.update(
                {
                    "build": build,
                    "productLanguage": "product is RU-only",
                    "artifactsSha256": artifacts,
                    "state": args.state,
                    "size": args.size,
                    "scale": args.scale,
                    "requestedLanguage": args.lang,
                    "png": f"{stem}.png",
                    "recording": f"{stem}.mp4" if args.record else None,
                    "recordSeconds": args.record,
                    "fps": args.fps if args.record else None,
                    "renderer": "WebKitGTK / Xvfb / Mesa llvmpipe (software)",
                    "platform": "Linux x86_64, Debian bookworm container",
                    "scaling": "WebKit webview zoom; physical output = logical size × scale",
                }
            )
            (args.out / f"{stem}.json").write_text(json.dumps(result, indent=2) + "\n")
            manifest = args.out / "manifest.json"
            entries = json.loads(manifest.read_text()) if manifest.exists() else []
            entries.append(result)
            manifest.write_text(json.dumps(entries, indent=2) + "\n")
            print(args.out / f"{stem}.png")
        finally:
            stopped = subprocess.run(
                ["agent-process", "stop", registered["id"]],
                capture_output=True,
                text=True,
            )
            if stopped.returncode:
                status = json.loads(
                    subprocess.check_output(
                        ["agent-process", "status", registered["id"]], text=True
                    )
                )
                if status["live"]:
                    raise RuntimeError(
                        f"Could not stop capture process {registered['id']}: {stopped.stderr}"
                    )


if __name__ == "__main__":
    run(arguments())
