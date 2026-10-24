#!/usr/bin/env python3
"""Quiet-host final A/B matrix. Run this supervisor through agent-process."""

import argparse
import hashlib
import json
import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VARIANTS = {
    "tauri": "winit-skia-opengl",
    "slint-gl": "winit-skia-opengl",
    "slint-sw": "winit-skia-software",
}
SCENARIOS = {
    "main": ("main", False),
    "skin": ("settings-skin", False),
    "skin-minimized": ("settings-skin", True),
    "main-minimized": ("main", True),
    "startup": ("main", False),
}


def quiet_gate(out, limit=1200):
    deadline = time.monotonic() + limit
    history = []
    while True:
        uptime = subprocess.check_output(["uptime"], text=True).strip()
        load = os.getloadavg()
        history.append({"time": time.time(), "load": load, "uptime": uptime})
        with (out / "gate.jsonl").open("a") as log:
            log.write(json.dumps(history[-1]) + "\n")
        if load[0] < 2 or time.monotonic() >= deadline:
            return {"noisy": load[0] >= 2, "history": history}
        time.sleep(min(10, max(0, deadline - time.monotonic())))


def run_one(args, variant, scenario, repeat):
    state, minimized = SCENARIOS[scenario]
    gate = quiet_gate(args.out)
    request = {
        "app": "tauri" if variant == "tauri" else "slint",
        "backend": VARIANTS[variant],
        "state": state,
        "minimized": minimized,
        "duration": 0 if scenario == "startup" else args.duration,
        "warmup": 0 if scenario == "startup" else args.warmup,
        "screenshot": args.smoke and not minimized,
    }
    name = f"{scenario}-{variant}-{repeat}"
    with tempfile.TemporaryDirectory(prefix="session-", dir=args.out) as directory:
        work = Path(directory)
        (work / "request.json").write_text(json.dumps(request))
        cid = work / "container.cid"
        argv = [
            "podman",
            "run",
            "--rm",
            "--cidfile",
            str(cid),
            "--network=none",
            "--security-opt",
            "label=disable",
            "--read-only",
            "--shm-size",
            "512m",
            "--tmpfs",
            "/tmp:rw,size=256m",
            "--tmpfs",
            "/run:rw,size=16m",
            "-v",
            f"{args.tauri.resolve()}:/opt/tauri:ro",
            "-v",
            f"{args.slint.resolve()}:/opt/slint:ro",
            "-v",
            f"{ROOT / 'scripts/bench'}:/bench:ro",
            "-v",
            f"{ROOT / 'scripts/capture'}:/harness:ro",
            "-v",
            f"{work}:/capture:rw",
            args.image,
            "python3",
            "/bench/final_session.py",
        ]
        env = dict(os.environ)
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        try:
            completed = subprocess.run(
                argv, env=env, capture_output=True, text=True, timeout=120
            )
            result = (
                json.loads((work / "result.json").read_text())
                if (work / "result.json").exists()
                else {}
            )
            result.update(
                {
                    "gate": gate,
                    "load_after": os.getloadavg(),
                    "command": argv,
                    "returncode": completed.returncode,
                    "stderr": completed.stderr,
                    "stdout": completed.stdout,
                }
            )
            for filename in ("app.log", "display.log", "smoke.png"):
                source = work / filename
                if source.exists():
                    source.replace(args.out / f"{name}.{filename}")
            (args.out / f"{name}.json").write_text(json.dumps(result, indent=2) + "\n")
            if completed.returncode or "error" in result or "startup_s" not in result:
                raise RuntimeError(f"{name} failed: {result}")
            print(
                f"{name}: ready {result['startup_s']:.3f}s; load={gate['history'][-1]['load'][0]:.2f}",
                flush=True,
            )
        finally:
            if cid.exists():
                subprocess.run(
                    ["podman", "rm", "--force", cid.read_text().strip()],
                    capture_output=True,
                    timeout=15,
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--tauri", type=Path, required=True)
    parser.add_argument("--slint", type=Path, required=True)
    parser.add_argument("--image", default="localhost/blockfield-final-ab:latest")
    parser.add_argument("--duration", type=float, default=30)
    parser.add_argument("--warmup", type=float, default=5)
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--scenario", choices=SCENARIOS, action="append")
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    metadata = {
        "git": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "image": json.loads(
            subprocess.check_output(
                ["podman", "image", "inspect", args.image], text=True
            )
        ),
        "binaries": {
            name: {
                "path": str(path.resolve()),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
            for name, path in [("tauri", args.tauri), ("slint", args.slint)]
        },
        "argv": vars(args)
        | {"out": str(args.out), "tauri": str(args.tauri), "slint": str(args.slint)},
    }
    (args.out / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    for scenario in args.scenario or SCENARIOS:
        repeats = 1 if args.smoke else 5 if scenario == "startup" else 3
        for repeat in range(repeats):
            # Interleave apps to avoid putting one renderer entirely at the end of a drift.
            for variant in VARIANTS:
                run_one(args, variant, scenario, repeat)
        with (args.out.parent / "progress.md").open("a") as progress:
            progress.write(
                f"- {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}: "
                f"{'smoke' if args.smoke else 'measurement'} {scenario} complete; "
                f"{repeats} repeats × {len(VARIANTS)} variants; evidence {args.out}.\n"
            )


def stop(_signum, _frame):
    raise SystemExit(143)


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, stop)
    main()
