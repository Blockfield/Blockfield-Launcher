#!/usr/bin/env python3
"""Render raw final_ab results, keeping run loads and distributions auditable."""

import argparse
import json
from pathlib import Path

from bench import summarize
from final_ab import SCENARIOS, VARIANTS

METRICS = ("pss_kb", "rss_kb", "uss_kb", "cpu_pct", "cpu_normalized", "threads")


def distributions(runs):
    values = {metric: [] for metric in METRICS}
    for run in runs:
        for index, sample in enumerate(run["samples"]):
            for metric in METRICS:
                # First snapshot has no preceding CPU ticks, so its zero is not an observation.
                if metric.startswith("cpu") and index == 0:
                    continue
                value = (
                    sample["totals"]["cpu_pct"] / run["cpu_count"]
                    if metric == "cpu_normalized"
                    else sample["totals"][metric]
                )
                values[metric].append(value / 1024 if metric.endswith("_kb") else value)
    return {metric: summarize(series) for metric, series in values.items()}


def change(value, baseline):
    return (
        f"{100 * (value / baseline - 1):+.1f}%" if baseline else "n/a (zero baseline)"
    )


def render(directory):
    meta = json.loads((directory / "metadata.json").read_text())
    lines = [
        "# Final Tauri / Slint resource A/B",
        "",
        "Scope: Linux headless **llvmpipe, no real GPU**. Both binaries run in the same "
        "read-only, networkless Fedora image with private Xvfb 1280×720@1x and Openbox. "
        "Fixture data only. No owner display or real account/game data.",
        "",
        "## Provenance",
        "",
        f"- Source revision: `{meta['git']}`.",
        f"- Shared image ID: `{meta['image'][0]['Id']}`.",
    ]
    for name, binary in meta["binaries"].items():
        lines.append(f"- {name}: `{binary['path']}`; SHA-256 `{binary['sha256']}`.")
    lines += [
        "",
        "## Method",
        "",
        "Each resource scenario: 5 s warmup after ready/minimize, 3 independent "
        "30 s runs, samples every 0.5 s. Apps interleaved by repeat. Whole app "
        "process tree includes the identical dbus-run-session wrapper and its "
        "children; excludes Xvfb/Openbox and the Python sampler. PSS/RSS/USS are "
        "summed over the tree. CPU% sums per-core usage (100% = one core); "
        "normalized CPU divides by the container CPU count. First CPU snapshot "
        "is omitted because it has no preceding tick baseline.",
        "",
        "Skia GL uses `SLINT_SKIA_PARTIAL_RENDERING=1`; software uses "
        "`SLINT_BACKEND=winit-skia-software`. Both use `LP_NUM_THREADS=4`, "
        "`LIBGL_ALWAYS_SOFTWARE=1`, `GALLIUM_DRIVER=llvmpipe`. Tauri retains "
        "its shipped rendering behavior with capture-only fixtures/local fonts. "
        "The capture preload enables console/readiness instrumentation and hides "
        "the caret; it does not pause animations. Minimize uses Openbox and checks "
        "`WM_STATE=Iconic`, rather than merely unmapping the window.",
        "",
        "Startup: five fresh process/container/home/cache runs per variant, "
        "main fixture, timed from process spawn to the existing ready signal. "
        "Xvfb/Openbox setup is excluded. Host page cache is not flushed. "
        "**This is process-cold readiness, not disk-cold first-frame latency.** "
        "Tauri waits for fixture/font proof; Slint prints READY after show(), "
        "before entering its painting event loop. Their readiness definitions differ.",
        "",
        "## Absolute resource medians and changes",
        "",
        "Negative changes mean lower resource use. Changes compare pooled medians "
        "against Tauri in the same scenario. Memory in MiB.",
        "",
        "| Scenario | Variant | PSS | RSS | USS | CPU% | CPU% / nproc | Threads | ΔPSS | ΔCPU |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    all_runs = {}
    stats = {}
    for scenario in SCENARIOS:
        for variant in VARIANTS:
            paths = sorted(directory.glob(f"{scenario}-{variant}-[0-9]*.json"))
            expected = 5 if scenario == "startup" else 3
            if len(paths) != expected:
                raise ValueError(
                    f"{scenario}/{variant}: expected {expected} runs, got {len(paths)}"
                )
            runs = [json.loads(path.read_text()) for path in paths]
            if any(run.get("returncode") or "error" in run for run in runs):
                raise ValueError(f"Failed runs: {scenario}/{variant}")
            all_runs[scenario, variant] = runs
            if scenario != "startup":
                if any(len(run["samples"]) < 60 for run in runs):
                    raise ValueError(f"Incomplete sampling: {scenario}/{variant}")
                stats[scenario, variant] = distributions(runs)
                med = {
                    metric: dist["median"]
                    for metric, dist in stats[scenario, variant].items()
                }
                base = stats[scenario, "tauri"]
                noisy = (
                    " **NOISY**" if any(run["gate"]["noisy"] for run in runs) else ""
                )
                lines.append(
                    f"| {scenario} | {variant}{noisy} | {med['pss_kb']:.2f} | "
                    f"{med['rss_kb']:.2f} | {med['uss_kb']:.2f} | {med['cpu_pct']:.2f} | "
                    f"{med['cpu_normalized']:.2f} | {med['threads']:.0f} | "
                    f"{change(med['pss_kb'], base['pss_kb']['median'])} | "
                    f"{change(med['cpu_pct'], base['cpu_pct']['median'])} |"
                )
    lines += [
        "",
        "## Startup ready (seconds)",
        "",
        "| Variant | p10 | Median | p90 | Min–max | Δmedian vs Tauri |",
        "|---|---:|---:|---:|---|---:|",
    ]
    startup_base = summarize(run["startup_s"] for run in all_runs["startup", "tauri"])[
        "median"
    ]
    for variant in VARIANTS:
        dist = summarize(run["startup_s"] for run in all_runs["startup", variant])
        lines.append(
            f"| {variant} | {dist['p10']:.3f} | {dist['median']:.3f} | "
            f"{dist['p90']:.3f} | {dist['min']:.3f}–{dist['max']:.3f} | "
            f"{change(dist['median'], startup_base)} |"
        )
    lines += [
        "",
        "## Distributions",
        "",
        "Pooled sample distributions; spread is max−min. Memory in MiB.",
        "",
        "| Scenario | Variant | Metric | p10 | Median | p90 | Spread |",
        "|---|---|---|---:|---:|---:|---:|",
    ]
    for (scenario, variant), metrics in stats.items():
        for metric, dist in metrics.items():
            label = metric.replace("_kb", "_MiB")
            lines.append(
                f"| {scenario} | {variant} | {label} | {dist['p10']:.2f} | "
                f"{dist['median']:.2f} | {dist['p90']:.2f} | {dist['max'] - dist['min']:.2f} |"
            )
    lines += [
        "",
        "## Run loads and repeat medians",
        "",
        "Load is host 1/5/15-minute average immediately before launch and after sampling. "
        "Each run gates 1-minute load <2.0, waiting up to 1200 s; otherwise labeled NOISY. "
        "Full gate histories and exact container argv are retained in each JSON.",
        "",
        "| Scenario | Variant | Repeat | Load before | Load after | Gate | PSS MiB | CPU% | Ready s |",
        "|---|---|---:|---|---|---|---:|---:|---:|",
    ]
    for (scenario, variant), runs in all_runs.items():
        for index, run in enumerate(runs):
            before = "/".join(
                f"{value:.2f}" for value in run["gate"]["history"][-1]["load"]
            )
            after = "/".join(f"{value:.2f}" for value in run["load_after"])
            med = distributions([run]) if run["samples"] else None
            pss = f"{med['pss_kb']['median']:.2f}" if med else "—"
            cpu = f"{med['cpu_pct']['median']:.2f}" if med else "—"
            lines.append(
                f"| {scenario} | {variant} | {index} | {before} | {after} | "
                f"{'NOISY' if run['gate']['noisy'] else 'quiet'} | {pss} | {cpu} | {run['startup_s']:.3f} |"
            )
    (directory / "summary.json").write_text(
        json.dumps(
            {
                f"{scenario}/{variant}": metrics
                for (scenario, variant), metrics in stats.items()
            },
            indent=2,
        )
        + "\n"
    )
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("report", type=Path)
    args = parser.parse_args()
    args.report.write_text(render(args.directory))
