"""Unit + smoke tests for scripts/bench/bench.py (stdlib unittest, no deps)."""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("bench", HERE / "bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class StatTests(unittest.TestCase):
    def test_quantile_matches_linear_interpolation(self):
        self.assertEqual(bench.quantile([7], 90), 7.0)
        self.assertEqual(bench.quantile([1, 2, 3, 4], 50), 2.5)
        self.assertAlmostEqual(bench.quantile([10, 20, 30], 10), 12.0)

    def test_summarize_reports_median_p10_p90_min_max(self):
        stats = bench.summarize([1, 2, 3, 4, 5])
        self.assertEqual(
            (stats["n"], stats["min"], stats["max"], stats["median"]),
            (5, 1, 5, 3.0),
        )
        self.assertLess(stats["p10"], stats["median"])
        self.assertGreater(stats["p90"], stats["median"])
        self.assertEqual(bench.summarize([]), {"n": 0})

    def test_descendants_covers_transitive_children_only(self):
        procs = {
            1: {"ppid": 0},
            10: {"ppid": 1},
            11: {"ppid": 10},
            12: {"ppid": 10},
            99: {"ppid": 1},
        }
        self.assertEqual(bench.descendants(10, procs), {10, 11, 12})

    def test_smaps_rollup_parses_uss_from_private_pages(self):
        out = subprocess.run(
            ["cat", f"/proc/{os.getpid()}/smaps_rollup"],
            capture_output=True,
            text=True,
        )
        if out.returncode != 0:
            self.skipTest("smaps_rollup unavailable")
        parsed = bench.read_smaps_rollup(os.getpid())
        self.assertIsNotNone(parsed)
        rss, pss, uss = parsed
        self.assertGreater(rss, 0)
        self.assertLessEqual(pss, rss)
        self.assertLessEqual(uss, rss)

    def test_sample_tree_covers_current_process(self):
        snapshot = bench.sample_tree(os.getpid(), None)
        pids = {p["pid"] for p in snapshot["processes"]}
        self.assertIn(os.getpid(), pids)
        self.assertGreater(snapshot["totals"]["pss_kb"], 0)


class SmokeTests(unittest.TestCase):
    def test_run_measures_tree_with_churn_and_writes_json_and_markdown(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            result = bench.run_scenario(
                [sys.executable, str(HERE / "synth_tree.py"), "6"],
                interval=0.2,
                duration=2.0,
                repeats=2,
                warmup=0.2,
                ready_regex="READY-parent",
                ready_file=None,
                ready_timeout=15.0,
                out_dir=out,
                name="synth",
            )
            (out / "result.json").write_text(json.dumps(result, indent=2))
            (out / "summary.md").write_text(bench.render_markdown(result))
            self.assertEqual(len(result["repeats"]), 2)
            for repeat in result["repeats"]:
                self.assertIsNotNone(repeat["startup_s"])
                self.assertGreaterEqual(len(repeat["samples"]), 5)
                totals = repeat["samples"][-1]["totals"]
                self.assertGreater(totals["pss_kb"], 0)
                names = {p["name"] for p in repeat["samples"][-1]["processes"]}
                self.assertTrue(names, "expected at least the root process")
            markdown = bench.render_markdown(result)
            self.assertIn("pooled totals", markdown)
            self.assertIn("pss_kb", markdown)


if __name__ == "__main__":
    unittest.main()
