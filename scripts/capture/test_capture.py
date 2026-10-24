"""Small, dependency-free checks for the capture CLI trust boundary."""

import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "capture_run", Path(__file__).with_name("run.py")
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CaptureArguments(unittest.TestCase):
    def test_supported_matrix(self):
        for state in MODULE.STATES:
            args = MODULE.arguments(
                [
                    state,
                    "--size",
                    "1600x900",
                    "--scale",
                    "1.25",
                    "--lang",
                    "uk",
                    "--out",
                    "/tmp/capture-test",
                ]
            )
            self.assertEqual((args.width, args.height, args.scale), (1600, 900, 1.25))

    def test_invalid_dimensions_and_recording_are_rejected(self):
        for options in (
            ["--size", "1279x720"],
            ["--size", "1280x719"],
            ["--size", "../../file"],
            ["--record", "-1"],
            ["--record", "61"],
            ["--fps", "30"],
            ["--scale", "2"],
        ):
            with self.subTest(options=options), self.assertRaises(SystemExit):
                MODULE.arguments(["main", "--out", "/tmp/capture-test", *options])


if __name__ == "__main__":
    unittest.main()
