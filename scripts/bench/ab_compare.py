#!/usr/bin/env python3
"""One-command release-vs-release A/B on Linux (stdlib only).

Runs two commands with identical sampling config via bench.py, then renders a
matched-pair median/spread table with % savings. Savings are computed only on
matched pairs (same repeats/duration/interval/warmup, sequential on one host);
empty sample windows are invalid and fail the run.

Usage:
  python3 scripts/bench/ab_compare.py --out OUTDIR \\
    --tauri-cmd "cmd for Tauri release" --slint-cmd "cmd for Slint release" \\
    [--repeats 3 --duration 10 --warmup 2 --interval 0.5] \\
    [--ready-regex REGEX | --tauri-ready-regex R --slint-ready-regex R]

Writes OUTDIR/tauri/{result.json,summary.md}, OUTDIR/slint/{...},
OUTDIR/ab.json and OUTDIR/ab.md.
"""

import argparse
import importlib.util
import json
import shlex
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("bench", HERE / "bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


def parse_cmd(text):
    argv = shlex.split(text)
    if not argv:
        raise SystemExit("empty command")
    return argv


def compare_results(tauri, slint):
    """Matched-pair stats + savings. Raises on empty windows or mismatched config."""
    for label, result in (("tauri", tauri), ("slint", slint)):
        if not result["repeats"]:
            raise ValueError(f"{label}: no repeats")
        for repeat in result["repeats"]:
            if not repeat["samples"]:
                raise ValueError(f"{label}: empty sample window (invalid)")
    if tauri["config"] != slint["config"]:
        raise ValueError("config mismatch: runs are not matched pairs")
    if len(tauri["repeats"]) != len(slint["repeats"]):
        raise ValueError("repeat count mismatch: runs are not matched pairs")
    pooled_tauri = bench.pooled_totals(tauri)
    pooled_slint = bench.pooled_totals(slint)
    comparison = {}
    for metric in sorted(set(pooled_tauri) | set(pooled_slint)):
        series_a = pooled_tauri.get(metric, [])
        series_b = pooled_slint.get(metric, [])
        if not series_a or not series_b:
            raise ValueError(f"{metric}: missing samples on one side (invalid)")
        stats_a = bench.summarize(series_a)
        stats_b = bench.summarize(series_b)
        base = stats_a["median"]
        comparison[metric] = {
            "tauri": stats_a,
            "slint": stats_b,
            "savings_pct": (100.0 * (stats_b["median"] / base - 1) if base else None),
        }
    startups = {}
    for label, result in (("tauri", tauri), ("slint", slint)):
        waits = [r["startup_s"] for r in result["repeats"]]
        if any(w is None for w in waits):
            raise ValueError(f"{label}: readiness timeout (invalid)")
        startups[label] = bench.summarize(waits)
    base = startups["tauri"]["median"]
    comparison["startup_s"] = {
        "tauri": startups["tauri"],
        "slint": startups["slint"],
        "savings_pct": 100.0 * (startups["slint"]["median"] / base - 1)
        if base
        else None,
    }
    return comparison


def render_ab(name, tauri, slint, comparison):
    lines = [
        f"# {name}: tauri vs slint (Linux, matched pairs)",
        "",
        f"tauri: `{' '.join(tauri['command'])}` · git `{tauri['git_sha'][:12]}`",
        f"slint: `{' '.join(slint['command'])}` · git `{slint['git_sha'][:12]}`",
        f"machine: {tauri['machine']['cpu']} · gpu: {tauri['machine']['gpu']} "
        f"· {tauri['machine']['cpu_count']} cpus",
        "",
        "Matched config: "
        + ", ".join(f"{k}={v}" for k, v in sorted(tauri["config"].items())),
        "Savings compare pooled medians on matched pairs only; "
        "negative = slint uses less. Spread = max-min.",
        "",
        "| metric | tauri median | tauri p10/p90 | tauri spread | "
        "slint median | slint p10/p90 | slint spread | savings |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for metric, stats in sorted(comparison.items()):
        row = stats
        savings = (
            f"{row['savings_pct']:+.1f}%"
            if row["savings_pct"] is not None
            else "n/a (zero baseline)"
        )
        lines.append(
            f"| {metric} | {row['tauri']['median']:.1f} | "
            f"{row['tauri']['p10']:.1f}/{row['tauri']['p90']:.1f} | "
            f"{row['tauri']['max'] - row['tauri']['min']:.1f} | "
            f"{row['slint']['median']:.1f} | "
            f"{row['slint']['p10']:.1f}/{row['slint']['p90']:.1f} | "
            f"{row['slint']['max'] - row['slint']['min']:.1f} | {savings} |"
        )
    return "\n".join(lines) + "\n"


def run_side(
    label,
    cmd,
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
    result = bench.run_scenario(
        cmd,
        interval=interval,
        duration=duration,
        repeats=repeats,
        warmup=warmup,
        ready_regex=ready_regex,
        ready_file=ready_file,
        ready_timeout=ready_timeout,
        out_dir=out_dir,
        name=name,
    )
    (out_dir / "result.json").write_text(json.dumps(result, indent=2))
    (out_dir / "summary.md").write_text(bench.render_markdown(result))
    return result


def build_parser():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--name", default="ab")
    parser.add_argument("--interval", type=float, default=0.5)
    parser.add_argument("--duration", type=float, default=10.0)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmup", type=float, default=2.0)
    parser.add_argument("--ready-regex", default=None)
    parser.add_argument("--tauri-ready-regex", default=None)
    parser.add_argument("--slint-ready-regex", default=None)
    parser.add_argument("--ready-file", default=None)
    parser.add_argument("--ready-timeout", type=float, default=30.0)
    parser.add_argument("--tauri-cmd", required=True)
    parser.add_argument("--slint-cmd", required=True)
    parser.add_argument("--tauri-name", default="tauri-idle")
    parser.add_argument("--slint-name", default="slint-idle")
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    out = args.out.resolve()
    tauri_ready = args.tauri_ready_regex or args.ready_regex
    slint_ready = args.slint_ready_regex or args.ready_regex
    tauri = run_side(
        "tauri",
        parse_cmd(args.tauri_cmd),
        interval=args.interval,
        duration=args.duration,
        repeats=args.repeats,
        warmup=args.warmup,
        ready_regex=tauri_ready,
        ready_file=args.ready_file,
        ready_timeout=args.ready_timeout,
        out_dir=out / "tauri",
        name=args.tauri_name,
    )
    slint = run_side(
        "slint",
        parse_cmd(args.slint_cmd),
        interval=args.interval,
        duration=args.duration,
        repeats=args.repeats,
        warmup=args.warmup,
        ready_regex=slint_ready,
        ready_file=args.ready_file,
        ready_timeout=args.ready_timeout,
        out_dir=out / "slint",
        name=args.slint_name,
    )
    comparison = compare_results(tauri, slint)
    payload = {
        "name": args.name,
        "machine": tauri["machine"],
        "git_sha": tauri["git_sha"],
        "comparison": comparison,
        "tauri": tauri,
        "slint": slint,
    }
    # shortcut: pooled samples stay in ab.json for audit; drop per-sample
    # process lists only if the file grows past review size (keep now).
    (out / "ab.json").write_text(json.dumps(payload, indent=2))
    markdown = render_ab(args.name, tauri, slint, comparison)
    (out / "ab.md").write_text(markdown)
    print(markdown)


if __name__ == "__main__":
    sys.dont_write_bytecode = True
    main()
