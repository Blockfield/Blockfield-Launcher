"""Small, dependency-free checks for the capture CLI trust boundary."""

import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import tempfile

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
                    "ru",
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
            ["--lang", "en"],
            ["--validate-build", "--record", "3"],
        ):
            with self.subTest(options=options), self.assertRaises(SystemExit):
                MODULE.arguments(["main", "--out", "/tmp/capture-test", *options])

    def test_invalid_binary_never_starts_a_container(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cache = root / ".cache/capture"
            cache.mkdir(parents=True)
            (cache / "build.json").write_text(
                '{"valid": false, "failure": "missing build URL"}'
            )
            args = MODULE.arguments(["main", "--out", str(root / "out")])
            with (
                patch.object(MODULE, "ROOT", root),
                patch.object(MODULE.subprocess, "check_output") as start,
            ):
                with self.assertRaisesRegex(SystemExit, "missing build URL"):
                    MODULE.run(args)
                start.assert_not_called()


if __name__ == "__main__":
    unittest.main()
