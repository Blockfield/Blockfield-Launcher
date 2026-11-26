#!/usr/bin/env python3
"""Quick headless Tauri-vs-native A/B for launcher#59 (brief u-bench).

Measures on the §0 inventory main screen, Linux, under an owned Xvfb
(never the owner's display): cold start to readiness, whole-process-tree
RSS idle median/peak, CPU idle mean, binary size (+ PSS median for the
issue's ≥30% gate). Repeats are sequential; results go to --out as
ab.json + report.md.

Both apps run fixture-only content with no production network:
  Tauri: capture frontend (VITE_CAPTURE_FIXTURES=1 pnpm build) + release
    src-tauri build, launched with BLOCKFIELD_CAPTURE_QUERY=state=main...,
    LD_PRELOAD=console.so, readiness = `capture-ready:` lang in the seeded
    config file (session.py protocol).
  Slint: release native/app build, launched with `--state main`,
    readiness = `READY pid=` on stdout.

console.so is built from scripts/capture/console.c when --console-so is
omitted (needs gcc + webkit2gtk-4.1 headers, both on the build host).

Example:
  python3 scripts/bench/quick_ab.py --out /tmp/u-bench/ab \\
    --tauri /path/to/blockfield-launcher --slint /path/to/blockfield-native \\
    --repeats 3 --duration 15 --warmup 5
"""

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bench  # noqa: E402  (bench.py: whole-tree sampler, stdlib only)


def free_display(number):
    path = Path(f"/tmp/.X11-unix/X{number}")
    if path.exists():
        return False
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        return sock.connect_ex(("127.0.0.1", 6000 + number)) != 0


def start_xvfb(xvfb_bin, display, out_dir):
    if not free_display(display):
        raise RuntimeError(f"display :{display} is already in use; refusing")
    log = (out_dir / "xvfb.log").open("w")
    proc = subprocess.Popen(
        [str(xvfb_bin), f":{display}", "-screen", "0", "1280x720x24"],
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    time.sleep(2)
    if proc.poll() is not None:
        raise RuntimeError(f"Xvfb exited immediately, see {out_dir / 'xvfb.log'}")
    return proc


def build_console_so(out_dir):
    root = Path(__file__).resolve().parents[2]
    src = root / "scripts/capture/console.c"
    out = out_dir / "console.so"
    subprocess.run(
        [
            "gcc",
            "-shared",
            "-fPIC",
            "-o",
            str(out),
            str(src),
            *subprocess.check_output(
                ["pkg-config", "--cflags", "webkit2gtk-4.1"], text=True
            ).split(),
        ],
        check=True,
    )
    return out


TAURI_ENV = {
    "GDK_BACKEND": "x11",
    "GDK_SCALE": "1",
    "GDK_DPI_SCALE": "1",
    "WEBKIT_DISABLE_DMABUF_RENDERER": "1",
    "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS": "1",
}

SEED_CONFIG = {
    "gameDir": "game",
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


def base_env(display):
    env = dict(os.environ)
    for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
        env.pop(key, None)
    env.update(
        {
            "DISPLAY": f":{display}",
            "LIBGL_ALWAYS_SOFTWARE": "1",
            "GALLIUM_DRIVER": "llvmpipe",
            "SLINT_SKIA_PARTIAL_RENDERING": "1",
            "TZ": "UTC",
            "LANG": "C.UTF-8",
        }
    )
    return env


def wait_config_ready(cfg_path, log_path, timeout=120):
    """Poll the seeded launcher config for capture-ready (session.py protocol)."""
    start = time.monotonic()
    while time.monotonic() - start < timeout:
        if log_path.exists():
            log = log_path.read_text(errors="replace")
            if "Unhandled Promise Rejection" in log:
                raise RuntimeError(f"tauri JS failure:\n{log[-3000:]}")
        if cfg_path.exists():
            try:
                lang = json.loads(cfg_path.read_text()).get("lang", "")
            except (json.JSONDecodeError, UnicodeDecodeError):
                lang = ""
            if lang.startswith("capture-failed:"):
                raise RuntimeError(f"tauri fixture failed: {lang}")
            if lang.startswith("capture-ready:"):
                return time.monotonic() - start, json.loads(
                    lang.removeprefix("capture-ready:")
                )
        time.sleep(0.1)
    return None, None


def sample_after_ready(proc, warmup, duration):
    time.sleep(warmup)
    samples = bench.sample_for(proc.pid, 0.5, duration)
    alive = proc.poll() is None
    for sample in samples:
        sample.pop("_stat", None)
    if not alive:
        raise RuntimeError("app exited mid-run")
    if not samples:
        raise RuntimeError("empty sample window")
    return samples


def stop(proc):
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


def run_tauri(binary, console_so, display, repeats, warmup, duration, out_dir):
    """Tauri side: disposable HOME, seeded config, config-file readiness."""
    prefix = ["dbus-run-session", "--"] if shutil.which("dbus-run-session") else []
    result = {"label": "tauri", "argv": [str(binary)], "repeats": []}
    for index in range(repeats):
        work = out_dir / f"tauri-home{index}"
        for name in ("home", "data", "config", "cache", "runtime"):
            (work / name).mkdir(mode=0o700, parents=True, exist_ok=True)
        cfg_path = work / "data/com.blockfield.launcher/launcher-config.json"
        cfg_path.parent.mkdir(parents=True, exist_ok=True)
        seed = dict(SEED_CONFIG)
        seed["gameDir"] = str(work / "game")
        cfg_path.write_text(json.dumps(seed))
        env = base_env(display)
        env.update(TAURI_ENV)
        env.update(
            {
                "HOME": str(work / "home"),
                "XDG_DATA_HOME": str(work / "data"),
                "XDG_CONFIG_HOME": str(work / "config"),
                "XDG_CACHE_HOME": str(work / "cache"),
                "XDG_RUNTIME_DIR": str(work / "runtime"),
                "LD_PRELOAD": str(console_so),
                "BLOCKFIELD_CAPTURE_QUERY": urllib.parse.urlencode(
                    {"state": "main", "lang": "ru", "scale": 1, "motion": 1}
                ),
            }
        )
        log_path = out_dir / f"tauri-repeat{index}.log"
        with open(log_path, "wb") as log:
            proc = subprocess.Popen(
                prefix + [str(binary)], stdout=log, stderr=subprocess.STDOUT, env=env
            )
        try:
            startup, _proof = wait_config_ready(cfg_path, log_path)
            if startup is None or proc.poll() is not None:
                tail = log_path.read_text(errors="replace")[-3000:]
                raise RuntimeError(
                    f"tauri repeat {index}: no readiness within 120s.\n{tail}"
                )
            samples = sample_after_ready(proc, warmup, duration)
        finally:
            stop(proc)
        totals = [s["totals"] for s in samples]
        result["repeats"].append(
            {
                "startup_s": startup,
                "rss_kb": [t["rss_kb"] for t in totals],
                "pss_kb": [t["pss_kb"] for t in totals if t["pss_kb"] is not None],
                "cpu_pct": [t["cpu_pct"] for t in totals],
            }
        )
    return result


def run_slint(binary, display, repeats, warmup, duration, out_dir):
    """Slint side: fixture main screen, stdout readiness, disposable HOME."""
    prefix = ["dbus-run-session", "--"] if shutil.which("dbus-run-session") else []
    result = {"label": "slint", "argv": [str(binary), "--state", "main"], "repeats": []}
    for index in range(repeats):
        home = out_dir / f"slint-home{index}"
        home.mkdir(mode=0o700, parents=True, exist_ok=True)
        env = base_env(display)
        env["HOME"] = str(home)
        log_path = out_dir / f"slint-repeat{index}.log"
        with open(log_path, "wb") as log:
            proc = subprocess.Popen(
                prefix + [str(binary), "--state", "main"],
                stdout=log,
                stderr=subprocess.STDOUT,
                env=env,
            )
        try:
            startup = bench.wait_ready(log_path, r"READY pid=", None, 90)
            if startup is None or proc.poll() is not None:
                tail = log_path.read_text(errors="replace")[-3000:]
                raise RuntimeError(
                    f"slint repeat {index}: no readiness within 90s.\n{tail}"
                )
            samples = sample_after_ready(proc, warmup, duration)
        finally:
            stop(proc)
        totals = [s["totals"] for s in samples]
        result["repeats"].append(
            {
                "startup_s": startup,
                "rss_kb": [t["rss_kb"] for t in totals],
                "pss_kb": [t["pss_kb"] for t in totals if t["pss_kb"] is not None],
                "cpu_pct": [t["cpu_pct"] for t in totals],
            }
        )
    return result


def stats(values):
    s = bench.summarize(values)
    return {
        "n": s["n"],
        "median": s["median"],
        "p10": s["p10"],
        "p90": s["p90"],
        "max": s["max"],
        "mean": sum(values) / len(values),
    }


def pool(app, key):
    values = [v for rep in app["repeats"] for v in rep[key]]
    if not values:
        return {"n": 0}
    return stats(values)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--tauri", type=Path, required=True)
    parser.add_argument("--slint", type=Path, required=True)
    parser.add_argument("--console-so", type=Path, default=None)
    parser.add_argument("--xvfb", type=Path, default=Path("/usr/bin/Xvfb"))
    parser.add_argument("--display", type=int, default=99)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--duration", type=float, default=15)
    parser.add_argument("--warmup", type=float, default=5)
    args = parser.parse_args(argv)

    for path in (args.tauri, args.slint):
        if not (path.exists() and os.access(path, os.X_OK)):
            raise SystemExit(f"not executable: {path}")
    args.out.mkdir(parents=True, exist_ok=True)
    console_so = args.console_so or build_console_so(args.out)

    xvfb = start_xvfb(args.xvfb, args.display, args.out)
    try:
        tauri = run_tauri(
            args.tauri,
            console_so,
            args.display,
            args.repeats,
            args.warmup,
            args.duration,
            args.out,
        )
        slint = run_slint(
            args.slint,
            args.display,
            args.repeats,
            args.warmup,
            args.duration,
            args.out,
        )
    finally:
        xvfb.terminate()
        xvfb.wait(timeout=10)

    report = {
        "machine": bench.machine_info(),
        "binaries": {
            "tauri": {"path": str(args.tauri), "bytes": args.tauri.stat().st_size},
            "slint": {"path": str(args.slint), "bytes": args.slint.stat().st_size},
        },
        "tauri": {
            "startup_s": stats([r["startup_s"] for r in tauri["repeats"]]),
            "rss_kb": pool(tauri, "rss_kb"),
            "pss_kb": pool(tauri, "pss_kb"),
            "cpu_pct": pool(tauri, "cpu_pct"),
        },
        "slint": {
            "startup_s": stats([r["startup_s"] for r in slint["repeats"]]),
            "rss_kb": pool(slint, "rss_kb"),
            "pss_kb": pool(slint, "pss_kb"),
            "cpu_pct": pool(slint, "cpu_pct"),
        },
    }
    for key in ("rss_kb", "pss_kb", "cpu_pct", "startup_s"):
        t, s = report["tauri"][key].get("median"), report["slint"][key].get("median")
        report.setdefault("savings_pct", {})[key] = 100.0 * (t - s) / t if t else None
    (args.out / "ab.json").write_text(json.dumps(report, indent=2) + "\n")

    mib = lambda kb: kb / 1024
    lines = [
        "# launcher#59 quick A/B: Tauri vs native main screen (headless, llvmpipe)",
        "",
        f"repeats={args.repeats} duration={args.duration}s warmup={args.warmup}s; "
        f"machine: {report['machine']['cpu']}; kernel {report['machine']['kernel']}",
        "",
        "| metric | tauri | native | savings |",
        "| --- | --- | --- | --- |",
    ]
    rows = [
        ("cold start to ready (median s)", "startup_s", lambda v: f"{v:.2f}"),
        ("RSS idle (median MiB)", "rss_kb", lambda v: f"{mib(v):.1f}"),
        ("RSS peak (max MiB)", "rss_kb", lambda v: f"{mib(v):.1f}", "max"),
        ("CPU idle (mean % of 1 core)", "cpu_pct", lambda v: f"{v:.2f}", "mean"),
        ("PSS (median MiB, gate metric)", "pss_kb", lambda v: f"{mib(v):.1f}"),
        ("binary size (MiB)", None, None),
    ]
    for name, key, fmt, *which in rows:
        agg = (which or ["median"])[0]
        if key is None:
            t = report["binaries"]["tauri"]["bytes"] / 2**20
            s = report["binaries"]["slint"]["bytes"] / 2**20
            lines.append(f"| {name} | {t:.1f} | {s:.1f} | {100 * (t - s) / t:.1f}% |")
            continue
        t, s = report["tauri"][key][agg], report["slint"][key][agg]
        base = (
            report["savings_pct"][key]
            if agg == "median"
            else (100 * (t - s) / t if t else None)
        )
        sav = f"{base:.1f}%" if base is not None else "n/a"
        lines.append(f"| {name} | {fmt(t)} | {fmt(s)} | {sav} |")
    pss_sav = report["savings_pct"]["pss_kb"]
    gate = "GO" if (pss_sav is not None and pss_sav >= 30) else "NO-GO"
    pss_txt = f"{pss_sav:.1f}%" if pss_sav is not None else "n/a (no PSS samples)"
    lines += [
        "",
        f"Gate reading (memory leg only): {gate} — PSS saving {pss_txt} "
        "(threshold ≥30%; visual/behavioral parity is reviewed separately).",
        "Headless llvmpipe result, not real-GPU data; fixture content, no network.",
    ]
    (args.out / "report.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
