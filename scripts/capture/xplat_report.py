#!/usr/bin/env python3
"""Diff actual Tauri captures against the committed native goldens and summarize matched runs."""
import argparse
import json
from pathlib import Path
import re
import statistics
import subprocess

GOLDENS = {'settings-general': 'main-tab-0', 'settings-runtime': 'main-tab-1',
           'settings-commands': 'main-tab-2', 'settings-launcher': 'main-tab-4'}


def compare(actual, golden, out):
    metrics = {}
    for metric in ('RMSE', 'AE'):
        command = ['magick', 'compare', '-metric', metric]
        if metric == 'AE':
            command += ['-fuzz', '2%']
        process = subprocess.run([*command, str(actual), str(golden),
                                  str(out) if metric == 'RMSE' else 'null:'],
                                 capture_output=True, text=True, timeout=30)
        if process.returncode not in (0, 1):
            raise RuntimeError(process.stderr)
        metrics[metric] = process.stderr.strip()
    metrics['rmse_normalized'] = float(re.search(r'\(([^)]+)\)', metrics['RMSE'])[1])
    metrics['different_pixels_2pct'] = int(float(metrics['AE']))
    return metrics


def summary(runs, key):
    values = [r[key] for r in runs]
    return statistics.mean(values), statistics.stdev(values) if len(values) > 1 else 0


def report(out, goldens):
    lines = ['# Tauri vs native: ' + goldens.name, '',
             'CI VM evidence, 1280×720 at scale 1. Visual differences are measurements, not parity approval.', '',
             '| State | RMSE normalized | Pixels ≠ (fuzz 2%) |', '|---|---:|---:|']
    metrics = {}
    for actual in sorted((out / 'captures').glob('*/capture.png')):
        state = actual.parent.name
        golden = goldens / (GOLDENS.get(state, state + '-state') + '.png')
        metrics[state] = compare(actual, golden, actual.parent / 'native-diff.png')
        lines.append(f"| {state} | {metrics[state]['rmse_normalized']:.6f} | {metrics[state]['different_pixels_2pct']} |")
    (out / 'diffs.json').write_text(json.dumps(metrics, indent=2))
    path = out / 'bench/result.json'
    if path.exists():
        result = json.loads(path.read_text())
        lines += ['', '| Scenario | App | Memory MiB mean ± sample SD | CPU % mean ± sample SD | Runs |',
                  '|---|---|---:|---:|---:|']
        for state in ('main', 'settings-skin'):
            for app in ('tauri', 'slint'):
                runs = [r for r in result['runs'] if r['state'] == state and r['app'] == app]
                if not runs:
                    continue
                memory, memory_sd = summary(runs, 'memory_bytes')
                cpu, cpu_sd = summary(runs, 'cpu_pct')
                lines.append(f'| {state} | {app} | {memory/2**20:.2f} ± {memory_sd/2**20:.2f} | {cpu:.2f} ± {cpu_sd:.2f} | {len(runs)} |')
            pairs = []
            for repeat in range(3):
                pair = {r['app']: r for r in result['runs'] if r['state'] == state and r['repeat'] == repeat}
                if pair.keys() == {'tauri', 'slint'}:
                    pairs.append({k: (1 - pair['slint'][k] / pair['tauri'][k]) * 100
                                  if pair['tauri'][k] else None for k in ('memory_bytes', 'cpu_pct')})
            if len(pairs) == 3:
                savings = {k: statistics.mean(p[k] for p in pairs) if all(p[k] is not None for p in pairs) else None
                           for k in ('memory_bytes', 'cpu_pct')}
                lines += ['', f"{state}: mean savings from three matched pairs: {savings} (percent).", '']
        lines += ['', 'Accounting: ' + result['mac_attribution'], result['limits'],
                  '5s fixed warmup after readiness; 3 × 30s per app/scenario, interleaved on the same runner.',
                  'Windows private working set; Linux PSS; macOS RSS fallback (shared pages may be double counted).',
                  'CPU sums process CPU-time deltas; 100% means one fully occupied core.',
                  'Renderer evidence: ' + json.dumps(result['renderers'])]
    lines += ['', 'Inspect capture proof.json and app.log for loaded font faces, viewport, renderer and errors.',
              'Windows hosted VMs lack a hardware GPU; Slint uses software. These runs do not establish real-GPU performance.']
    for failure in out.rglob('failures.json'):
        lines += ['', f'Capture failures: `{failure.read_text()}`']
    text = '\n'.join(lines) + '\n'
    (out / 'report.md').write_text(text)
    return text


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--goldens', type=Path, required=True)
    args = parser.parse_args()
    print(report(args.out, args.goldens))
