#!/usr/bin/env python3
"""Parallel watchdog runner for the native Slint capture and E2E CI walks.

Unifies the inline Python previously embedded in `.github/workflows/native.yml`
so no `run:` block pipes multi-KB heredocs through actionlint's shellcheck
(which deadlocks on Windows once a block exceeds the 4096-byte pipe buffer).

Each task runs its command with stdout+stderr captured to `app.log` under a
150 s watchdog: on timeout the log gets `\nTIMEOUT after 150s\n` and the task
exits 124. On Linux the command is wrapped in `xvfb-run -n <display>` (one
display per task, `100 + idx`, never `-a`) in its own process group, so a
timeout SIGKILLs Xvfb and the app together instead of leaking them; on Windows
the child is killed and reaped.
"""

import argparse
import concurrent.futures
import os
import signal
import struct
import subprocess
import sys
import threading
import zlib
from pathlib import Path

TIMEOUT_SECS = 150
MAX_WORKERS = 4
XVFB_SCREEN = "-screen 0 1920x1080x24"
CAPTURE_STATES = ("main", "settings-skin")
E2E_STATES = (
    "main",
    "main-launching",
    "offline",
    "error",
    "settings-skin",
    "login",
    "firstrun",
    "update",
)


def run_guarded(cmd, log_path, env):
    """Run cmd with output to log_path; 124 plus a TIMEOUT marker on timeout."""
    with open(log_path, "wb") as log:
        kwargs = {"start_new_session": True} if os.name == "posix" else {}
        proc = subprocess.Popen(
            cmd, stdout=log, stderr=subprocess.STDOUT, env=env, **kwargs
        )
        try:
            return proc.wait(timeout=TIMEOUT_SECS)
        except subprocess.TimeoutExpired:
            if os.name == "posix":
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
            else:
                proc.kill()
            proc.wait()
            log.write(b"\nTIMEOUT after 150s\n")
            log.flush()
            return 124


def _xvfb(cmd, display):
    if os.environ.get("RUNNER_OS") == "Linux":
        return ["xvfb-run", "-n", str(display), "-s", XVFB_SCREEN, *cmd]
    return cmd


def _run_all(tasks):
    """Run (label, cmd, env, log) tasks in parallel; True if any task failed."""
    lock = threading.Lock()
    failed = False

    def one(task):
        label, cmd, env, log = task
        with lock:
            print(f"=== {label} ===", flush=True)
        code = run_guarded(cmd, str(log), env)
        with lock:
            print(f"exit={code} [{label}]", flush=True)
            if code != 0:
                print(f"--- {log} (last 3k) ---", flush=True)
                try:
                    tail = Path(log).read_bytes()[-3000:]
                    print(tail.decode("utf-8", "replace"), flush=True)
                except OSError as e:
                    print(f"(no log: {e})", flush=True)
        return code

    with concurrent.futures.ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
        for code in pool.map(one, tasks):
            if code != 0:
                failed = True
    return failed


def _chunk(chunk_type, data):
    return (
        struct.pack(">I", len(data))
        + chunk_type
        + data
        + struct.pack(">I", zlib.crc32(chunk_type + data) & 0xFFFFFFFF)
    )


def convert_ppms(out):
    """Convert every P6 PPM under out to PNG (stdlib only), deleting originals."""
    for ppm in sorted(Path(out).rglob("*.ppm")):
        data = ppm.read_bytes()
        assert data.startswith(b"P6\n"), ppm
        header_end = data.index(b"255\n") + 4
        w, h, maxval = (int(x) for x in data[:header_end].split()[1:4])
        assert maxval == 255, ppm
        raw = data[header_end:]
        assert len(raw) == w * h * 3, (ppm, len(raw), w, h)
        rows = b"".join(b"\x00" + raw[y * w * 3 : (y + 1) * w * 3] for y in range(h))
        png = (
            b"\x89PNG\r\n\x1a\n"
            + _chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + _chunk(b"IDAT", zlib.compress(rows))
            + _chunk(b"IEND", b"")
        )
        target = ppm.with_suffix(".png")
        target.write_bytes(png)
        print(f"{target} {w}x{h}")
        ppm.unlink()


def list_files(out):
    for path in sorted(str(p) for p in Path(out).rglob("*") if p.is_file()):
        print(path)


def cmd_capture(args):
    out = Path(args.out)
    tasks = []
    for renderer in ("default", "software"):
        for scale in ("1", "1.5"):
            for state in CAPTURE_STATES:
                d = out / renderer / f"scale-{scale}" / state
                d.mkdir(parents=True, exist_ok=True)
                env = dict(os.environ, SLINT_SCALE_FACTOR=scale)
                if renderer == "software":
                    env["SLINT_BACKEND"] = "winit-software"
                else:
                    env.pop("SLINT_BACKEND", None)
                log = d / "app.log"
                cmd = _xvfb(
                    [
                        args.exe,
                        "--state",
                        state,
                        "--freeze-motion",
                        "--capture-dir",
                        str(d),
                    ],
                    100 + len(tasks),
                )
                tasks.append((f"{renderer} scale={scale} state={state}", cmd, env, log))
    failed = _run_all(tasks)
    convert_ppms(out)
    list_files(out)
    return 1 if failed else 0


def cmd_e2e(args):
    out = Path(args.out)
    tasks = []
    for idx, state in enumerate(E2E_STATES):
        d = out / state
        d.mkdir(parents=True, exist_ok=True)
        env = dict(os.environ, BLOCKFIELD_E2E_STATE=state)
        log = d / "app.log"
        cmd = [
            args.exe,
            "--state",
            state,
            "--freeze-motion",
            "--e2e",
            str(d),
            "--e2e-golden",
            args.golden,
        ]
        if args.update:
            cmd.append("--e2e-update-goldens")
        tasks.append((f"e2e state={state}", _xvfb(cmd, 100 + idx), env, log))
    failed = _run_all(tasks)
    list_files(out)
    return 1 if failed else 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Parallel watchdog runner for native captures and E2E walks."
    )
    sub = parser.add_subparsers(required=True)
    capture = sub.add_parser("capture", help="run renderer x scale x state captures")
    capture.add_argument("--exe", required=True)
    capture.add_argument("--out", required=True)
    capture.set_defaults(func=cmd_capture)
    e2e = sub.add_parser("e2e", help="run the per-state E2E acceptance walk")
    e2e.add_argument("--exe", required=True)
    e2e.add_argument("--out", required=True)
    e2e.add_argument("--golden", required=True)
    e2e.add_argument("--update", action="store_true")
    e2e.set_defaults(func=cmd_e2e)
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
