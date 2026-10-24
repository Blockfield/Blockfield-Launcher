#!/usr/bin/env python3
"""Sample a whole Linux process tree (PSS/RSS/USS, CPU%, threads, GPU).

Usage:
  bench.py sample --pid PID [--interval 0.5 --duration 10 --json-out OUT]
  bench.py run --duration 10 [--interval 0.5 --repeats 3 --warmup 2]
      [--ready-regex REGEX | --ready-file PATH] [--out-dir DIR] -- CMD [ARGS...]

`run` launches CMD, waits for a readiness signal, sleeps `warmup`, then samples
the full descendant tree for `duration`, repeated `repeats` times. It writes
result.json (raw samples) and summary.md (median/p10/p90/min/max + table).
"""

import argparse
import glob
import json
import os
import platform
import re
import signal
import subprocess
import time
from pathlib import Path

HERTZ = os.sysconf(os.sysconf_names["SC_CLK_TCK"])


def read_stat_full(pid):
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8") as handle:
            data = handle.read()
    except OSError:
        return None
    end = data.rfind(")")
    if end < 0:
        return None
    fields = data[end + 2 :].split()
    if len(fields) < 13:
        return None
    try:
        return {
            "name": data[data.find("(") + 1 : end],
            "ppid": int(fields[1]),
            "utime": int(fields[11]),
            "stime": int(fields[12]),
        }
    except ValueError:
        return None


def all_processes():
    """Map pid -> {name, ppid, utime, stime} for every visible process."""
    procs = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        info = read_stat_full(int(entry))
        if info is not None:
            procs[int(entry)] = info
    return procs


def descendants(root, procs):
    """Root plus all transitive children; tolerates re-spawning children."""
    children = {}
    for pid, info in procs.items():
        children.setdefault(info["ppid"], []).append(pid)
    seen = {root}
    stack = [root]
    while stack:
        for child in children.get(stack.pop(), []):
            if child not in seen:
                seen.add(child)
                stack.append(child)
    return seen


def read_smaps_rollup(pid):
    """(rss_kb, pss_kb, uss_kb) from smaps_rollup; None if unreadable."""
    try:
        with open(f"/proc/{pid}/smaps_rollup", encoding="utf-8") as handle:
            lines = handle.readlines()
    except OSError:
        return None
    values = {}
    for line in lines:
        key, _, rest = line.partition(":")
        key = key.strip()
        if key in ("Rss", "Pss", "Private_Clean", "Private_Dirty"):
            try:
                values[key] = int(rest.split()[0])
            except (ValueError, IndexError):
                return None
    if set(values) != {"Rss", "Pss", "Private_Clean", "Private_Dirty"}:
        return None
    return (
        values["Rss"],
        values["Pss"],
        values["Private_Clean"] + values["Private_Dirty"],
    )


def thread_count(pid):
    try:
        with open(f"/proc/{pid}/status", encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("Threads:"):
                    return int(line.split()[1])
    except (OSError, ValueError):
        return None
    return None


def gpu_kb(pid):
    """Sum of drm-memory-* KiB over the process's fdinfo; None if not measurable."""
    total, found = 0, False
    for path in glob.glob(f"/proc/{pid}/fdinfo/*"):
        try:
            with open(path, encoding="utf-8", errors="replace") as handle:
                for line in handle:
                    if line.startswith("drm-memory-") and line.rstrip().endswith("KiB"):
                        total += int(line.split()[-2])
                        found = True
        except (OSError, ValueError):
            continue
    return total if found else None


def sample_tree(root, previous=None):
    """One snapshot of the tree under root. previous maps pid -> stat for CPU%."""
    now = time.monotonic()
    procs = all_processes()
    pids = descendants(root, procs) if root in procs else set()
    processes, totals = (
        [],
        {
            "rss_kb": 0,
            "pss_kb": 0,
            "uss_kb": 0,
            "cpu_pct": 0.0,
            "threads": 0,
            "gpu_kb": 0,
        },
    )
    gpu_measured = False
    current = {}
    for pid in sorted(pids):
        info = procs[pid]
        current[pid] = info
        mem = read_smaps_rollup(pid)
        entry = {"pid": pid, "name": info["name"], "threads": thread_count(pid)}
        if mem is not None:
            entry.update(zip(("rss_kb", "pss_kb", "uss_kb"), mem))
            totals["rss_kb"] += mem[0]
            totals["pss_kb"] += mem[1]
            totals["uss_kb"] += mem[2]
        entry["gpu_kb"] = gpu_kb(pid)
        if entry["gpu_kb"] is not None:
            totals["gpu_kb"] += entry["gpu_kb"]
            gpu_measured = True
        entry["cpu_pct"] = 0.0
        if previous is not None and pid in previous:
            prev, dt = previous[pid], now - previous[pid]["t"]
            if dt > 0:
                ticks = (info["utime"] + info["stime"]) - (
                    prev["utime"] + prev["stime"]
                )
                entry["cpu_pct"] = max(0.0, 100.0 * ticks / HERTZ / dt)
                totals["cpu_pct"] += entry["cpu_pct"]
        if entry["threads"] is not None:
            totals["threads"] += entry["threads"]
        processes.append(entry)
    totals["gpu_kb"] = totals["gpu_kb"] if gpu_measured else None
    snapshot = {"t": now, "processes": processes, "totals": totals}
    for pid, info in current.items():
        info["t"] = now
    snapshot["_stat"] = current
    return snapshot


def sample_for(root, interval, duration):
    """Sample every interval seconds for duration seconds (first sample: no CPU%)."""
    samples, previous, deadline = [], None, time.monotonic() + duration
    while True:
        snapshot = sample_tree(root, previous)
        previous = snapshot.pop("_stat")
        snapshot["t_offset"] = snapshot["t"] - (
            samples[0]["t"] if samples else snapshot["t"]
        )
        samples.append(snapshot)
        if time.monotonic() >= deadline:
            return samples
        time.sleep(min(interval, max(0.0, deadline - time.monotonic())))


def quantile(values, pct):
    """Linear-interpolation percentile over a non-empty sequence."""
    ordered = sorted(values)
    if len(ordered) == 1:
        return float(ordered[0])
    rank = (len(ordered) - 1) * pct / 100.0
    low, frac = int(rank), rank - int(rank)
    return ordered[low] + frac * (
        ordered[min(low + 1, len(ordered) - 1)] - ordered[low]
    )


def summarize(values):
    values = list(values)
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "min": min(values),
        "p10": quantile(values, 10),
        "median": quantile(values, 50),
        "p90": quantile(values, 90),
        "max": max(values),
    }


def machine_info():
    info = {
        "kernel": platform.release(),
        "cpu": platform.processor() or "unknown",
        "display": os.environ.get("WAYLAND_DISPLAY")
        or os.environ.get("DISPLAY")
        or "headless",
        "session_type": os.environ.get("XDG_SESSION_TYPE", "unknown"),
        "gpu": "not detected",
    }
    try:
        with open("/proc/meminfo", encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("MemTotal:"):
                    info["ram_kb"] = int(line.split()[1])
                    break
    except (FileNotFoundError, ValueError):
        info["ram_kb"] = None
    try:
        with open("/proc/cpuinfo", encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("model name"):
                    info["cpu"] = line.split(":", 1)[1].strip()
                    break
    except FileNotFoundError:
        pass
    cards = sorted(glob.glob("/sys/class/drm/card?/device/uevent"))
    names = []
    for card in cards:
        vendor = device = None
        try:
            with open(card, encoding="utf-8") as handle:
                for line in handle:
                    if line.startswith("PCI_ID="):
                        vendor, _, device = line.strip().split("=", 1)[1].partition(":")
        except FileNotFoundError:
            continue
        names.append(f"{Path(card).parts[4]} pci={vendor}:{device}")
    if names:
        info["gpu"] = ", ".join(names)
    info["cpu_count"] = os.cpu_count()
    return info


def git_sha(start):
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            cwd=start,
            timeout=10,
        )
        if out.returncode == 0:
            return out.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        pass
    return "unknown"


def wait_ready(log_path, ready_regex, ready_file, timeout):
    """Poll log file for regex or for a path appearing; returns seconds or None."""
    start = time.monotonic()
    pattern = re.compile(ready_regex) if ready_regex else None
    target = Path(ready_file) if ready_file else None
    while time.monotonic() - start < timeout:
        if target is not None and target.exists():
            return time.monotonic() - start
        if pattern is not None and log_path.exists():
            with open(log_path, encoding="utf-8", errors="replace") as handle:
                if pattern.search(handle.read()):
                    return time.monotonic() - start
        time.sleep(0.05)
    return None


def run_scenario(
    command,
    *,
    interval,
    duration,
    repeats,
    warmup,
    ready_regex,
    ready_file,
    ready_timeout,
    out_dir,
    name,
):
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    result = {
        "name": name,
        "command": command,
        "config": {
            "interval": interval,
            "duration": duration,
            "repeats": repeats,
            "warmup": warmup,
            "ready_regex": ready_regex,
            "ready_file": ready_file,
            "ready_timeout": ready_timeout,
        },
        "machine": machine_info(),
        "git_sha": git_sha(Path(__file__).resolve().parents[2]),
        "repeats": [],
    }
    for index in range(repeats):
        log_path = out_dir / f"repeat{index}.log"
        with open(log_path, "wb") as log:
            proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        try:
            startup = wait_ready(log_path, ready_regex, ready_file, ready_timeout)
            time.sleep(warmup)
            samples = sample_for(proc.pid, interval, duration)
            alive = proc.poll() is None
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
        for sample in samples:
            sample.pop("_stat", None)
        result["repeats"].append(
            {
                "startup_s": startup,
                "exited_mid_run": not alive,
                "samples": samples,
            }
        )
    return result


def pooled_totals(result):
    pooled = {}
    for repeat in result["repeats"]:
        for sample in repeat["samples"]:
            for key, value in sample["totals"].items():
                if value is not None:
                    pooled.setdefault(key, []).append(value)
    return pooled


def render_markdown(result):
    lines = [
        f"# {result['name']}",
        "",
        f"command: `{' '.join(result['command'])}` · "
        f"git: `{result['git_sha'][:12]}` · "
        f"cpu: {result['machine']['cpu']} · gpu: {result['machine']['gpu']}",
        "",
        "## repeats",
        "| repeat | startup s | samples | pss med KiB | pss p90 KiB | "
        "rss med KiB | uss med KiB | cpu avg % |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for index, repeat in enumerate(result["repeats"]):
        totals = [s["totals"] for s in repeat["samples"]]
        pss = [t["pss_kb"] for t in totals]
        startup = (
            f"{repeat['startup_s']:.2f}"
            if repeat["startup_s"] is not None
            else "timeout"
        )
        cpu = sum(t["cpu_pct"] for t in totals) / len(totals) if totals else 0.0
        lines.append(
            f"| {index} | {startup} | {len(totals)} | {quantile(pss, 50):.0f} | "
            f"{quantile(pss, 90):.0f} | "
            f"{quantile([t['rss_kb'] for t in totals], 50):.0f} | "
            f"{quantile([t['uss_kb'] for t in totals], 50):.0f} | {cpu:.1f} |"
        )
    lines += [
        "",
        "## pooled totals (all samples, all repeats)",
        "| metric | min | p10 | median | p90 | max |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for key, values in sorted(pooled_totals(result).items()):
        stats = summarize(values)
        lines.append(
            f"| {key} | {stats['min']:.1f} | {stats['p10']:.1f} | "
            f"{stats['median']:.1f} | {stats['p90']:.1f} | {stats['max']:.1f} |"
        )
    return "\n".join(lines) + "\n"


def command_sample(args):
    samples = sample_for(args.pid, args.interval, args.duration)
    for sample in samples:
        sample.pop("_stat", None)
    payload = {
        "pid": args.pid,
        "machine": machine_info(),
        "pooled": {
            k: summarize(v)
            for k, v in pooled_totals({"repeats": [{"samples": samples}]}).items()
        },
        "samples": samples,
    }
    text = json.dumps(payload, indent=2)
    if args.json_out:
        Path(args.json_out).write_text(text, encoding="utf-8")
    else:
        print(text)


def command_run(args):
    if not args.command:
        raise SystemExit("run needs a command: bench.py run -- CMD [ARGS...]")
    result = run_scenario(
        args.command,
        interval=args.interval,
        duration=args.duration,
        repeats=args.repeats,
        warmup=args.warmup,
        ready_regex=args.ready_regex,
        ready_file=args.ready_file,
        ready_timeout=args.ready_timeout,
        out_dir=args.out_dir,
        name=args.name,
    )
    out_dir = Path(args.out_dir)
    (out_dir / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    markdown = render_markdown(result)
    (out_dir / "summary.md").write_text(markdown, encoding="utf-8")
    print(markdown)


def build_parser():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    sample = sub.add_parser("sample", help="sample the tree of a running PID")
    sample.add_argument("--pid", type=int, required=True)
    sample.add_argument("--interval", type=float, default=0.5)
    sample.add_argument("--duration", type=float, default=10.0)
    sample.add_argument("--json-out", default=None)
    sample.set_defaults(func=command_sample)
    run = sub.add_parser("run", help="launch a command and measure its tree")
    run.add_argument("--interval", type=float, default=0.5)
    run.add_argument("--duration", type=float, default=10.0)
    run.add_argument("--repeats", type=int, default=3)
    run.add_argument("--warmup", type=float, default=2.0)
    run.add_argument("--ready-regex", default=None)
    run.add_argument("--ready-file", default=None)
    run.add_argument("--ready-timeout", type=float, default=30.0)
    run.add_argument("--out-dir", default="bench-out")
    run.add_argument("--name", default="scenario")
    run.add_argument("command", nargs=argparse.REMAINDER)
    run.set_defaults(func=command_run)
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.cmd == "run" and args.command and args.command[0] == "--":
        args.command = args.command[1:]
    # argparse.REMAINDER keeps a leading "--"; drop it so Popen sees the real argv.
    signal.signal(signal.SIGPIPE, signal.SIG_DFL)
    args.func(args)


if __name__ == "__main__":
    main()
