#!/usr/bin/env python3
"""Checks for the quiet gate and the report's CPU/percent calculations."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from final_ab import quiet_gate
from final_report import change, distributions


class FinalTests(unittest.TestCase):
    def test_gate_quiet_and_noisy_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            with (
                patch("final_ab.os.getloadavg", return_value=(1.9, 4, 5)),
                patch(
                    "final_ab.subprocess.check_output", return_value="uptime fixture"
                ),
            ):
                self.assertFalse(quiet_gate(out, limit=0)["noisy"])
            with (
                patch("final_ab.os.getloadavg", return_value=(2, 4, 5)),
                patch(
                    "final_ab.subprocess.check_output", return_value="uptime fixture"
                ),
            ):
                self.assertTrue(quiet_gate(out, limit=0)["noisy"])
            self.assertEqual(len((out / "gate.jsonl").read_text().splitlines()), 2)

    def test_gate_rechecks_after_wait(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch("final_ab.os.getloadavg", side_effect=[(3, 4, 5), (1, 4, 5)]),
            patch("final_ab.subprocess.check_output", return_value="uptime fixture"),
            patch("final_ab.time.sleep") as sleep,
        ):
            gate = quiet_gate(Path(directory))
            self.assertFalse(gate["noisy"])
            self.assertEqual(len(gate["history"]), 2)
            sleep.assert_called_once()

    def test_distributions_exclude_only_first_cpu_sample(self):
        totals = {"pss_kb": 1024, "rss_kb": 2048, "uss_kb": 512, "threads": 8}
        stats = distributions(
            [
                {
                    "cpu_count": 4,
                    "samples": [
                        {"totals": totals | {"cpu_pct": 0}},
                        {"totals": totals | {"cpu_pct": 100}},
                        {"totals": totals | {"cpu_pct": 200}},
                    ],
                }
            ]
        )
        self.assertEqual(stats["pss_kb"]["n"], 3)
        self.assertEqual(stats["pss_kb"]["median"], 1)
        self.assertEqual(stats["cpu_pct"]["median"], 150)
        self.assertEqual(stats["cpu_normalized"]["median"], 37.5)
        self.assertEqual(change(70, 100), "-30.0%")
        self.assertEqual(change(1, 0), "n/a (zero baseline)")


if __name__ == "__main__":
    unittest.main()
