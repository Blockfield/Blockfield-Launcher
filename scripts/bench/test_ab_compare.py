#!/usr/bin/env python3
"""Tests for scripts/bench/ab_compare.py (stdlib unittest, no deps)."""

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("ab_compare", HERE / "ab_compare.py")
ab_compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ab_compare)
bench_spec = importlib.util.spec_from_file_location("bench", HERE / "bench.py")
bench = importlib.util.module_from_spec(bench_spec)
bench_spec.loader.exec_module(bench)


def fake_result(name, pss, startup):
    samples = [
        {"totals": {"pss_kb": v, "rss_kb": v * 2, "cpu_pct": 10.0, "threads": 4}}
        for v in pss
    ]
    return {
        "name": name,
        "command": [name],
        "config": {"interval": 0.5, "duration": 1.0, "repeats": 1},
        "machine": {"cpu": "t", "gpu": "g", "cpu_count": 4},
        "git_sha": "abc",
        "repeats": [
            {"startup_s": startup, "exited_mid_run": False, "samples": samples}
        ],
    }


class CompareTests(unittest.TestCase):
    def test_savings_negative_when_slint_smaller(self):
        tauri = fake_result("tauri", [200, 210, 220], 1.0)
        slint = fake_result("slint", [100, 110, 120], 0.5)
        out = ab_compare.compare_results(tauri, slint)
        self.assertAlmostEqual(out["pss_kb"]["savings_pct"], -47.6, places=1)
        self.assertIn("savings", ab_compare.render_ab("ab", tauri, slint, out))

    def test_empty_window_is_invalid(self):
        tauri = fake_result("tauri", [200], 1.0)
        slint = fake_result("slint", [100], 0.5)
        slint["repeats"][0]["samples"] = []
        with self.assertRaises(ValueError):
            ab_compare.compare_results(tauri, slint)

    def test_mismatched_config_is_invalid(self):
        tauri = fake_result("tauri", [200], 1.0)
        slint = fake_result("slint", [100], 0.5)
        slint["config"] = dict(tauri["config"], repeats=2)
        with self.assertRaises(ValueError):
            ab_compare.compare_results(tauri, slint)


class SmokeTests(unittest.TestCase):
    def test_run_both_sides_with_synth_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            synth = str(HERE / "synth_tree.py")
            ab_compare.main(
                [
                    "--out",
                    str(out),
                    "--repeats",
                    "1",
                    "--duration",
                    "1.0",
                    "--interval",
                    "0.2",
                    "--warmup",
                    "0.1",
                    "--ready-regex",
                    "READY-parent",
                    "--tauri-cmd",
                    f"{sys.executable} {synth} 4",
                    "--slint-cmd",
                    f"{sys.executable} {synth} 4",
                ]
            )
            payload = json.loads((out / "ab.json").read_text())
            self.assertIn("pss_kb", payload["comparison"])
            self.assertIn("savings", (out / "ab.md").read_text())


if __name__ == "__main__":
    unittest.main()
