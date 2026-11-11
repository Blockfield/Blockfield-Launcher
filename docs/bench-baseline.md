# Bench baseline (Linux, harness self-check)

One-command release-vs-release A/B lives in `scripts/bench/` (see its README).
Same-machine, same-config, matched pairs only; empty windows fail.

```sh
python3 scripts/bench/ab_compare.py --out /tmp/bench/ab --repeats 3 \
  --duration 10 --warmup 2 --ready-regex "READY" \
  --tauri-cmd "./target/release/blockfield-launcher" \
  --slint-cmd "./native/app/target/release/blockfield-native"
```

Output: `tauri/{result.json,summary.md}`, `slint/{...}`, `ab.json` (medians +
savings) and `ab.md` (median/p10/p90/spread table).

## Baseline recorded here

Synthetic harness self-check only (`synth_tree.py`, 3×5 s, 2026-11-11,
`a7369bc`, Ryzen 5 7535U): identical trees both sides, PSS medians within
1.5% — proves the harness runs matched pairs and renders the table, not that
either launcher is faster. Evidence: `.dev/u-launcher-bench/baseline/ab.json`.

Not measured here: real Tauri vs Slint release binaries, GPU/VRAM, animation
sweep, download/update, hidden/minimized. GO gate needs ≥30% tree-PSS cut with
no CPU/GPU regression on release builds; headless llvmpipe is not real-GPU
data.
