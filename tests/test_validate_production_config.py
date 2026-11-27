import os
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "validate-production-config.py"


def _run(pack_url, workshop_url=""):
    env = os.environ.copy()
    env["VITE_BLOCKFIELD_PACK_URL"] = pack_url or ""
    env["VITE_BLOCKFIELD_WORKSHOP_PACK_URL"] = workshop_url
    return subprocess.run(
        [sys.executable, str(SCRIPT)],
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


class ValidateProductionConfigTests(unittest.TestCase):
    def test_accepts_production_origin_and_optional_workshop_url(self):
        self.assertEqual(_run("https://blockfield.pro").returncode, 0)
        self.assertEqual(
            _run(
                "https://blockfield.pro", "https://blockfield.pro/workshop"
            ).returncode,
            0,
        )

    def test_rejects_missing_insecure_private_or_credential_urls(self):
        for bad in (
            "",
            "http://blockfield.pro",
            "https://localhost/pack",
            "https://127.0.0.1/pack",
            "https://10.0.0.1/pack",
            "https://192.168.1.5/pack",
            "https://172.16.0.1/pack",
            "https://[::1]/pack",
            "https://user:pass@blockfield.pro",
        ):
            with self.subTest(url=bad):
                self.assertNotEqual(_run(bad).returncode, 0)

    def test_rejects_invalid_workshop_url(self):
        for bad in ("http://blockfield.pro/workshop", "https://10.0.0.1/workshop"):
            with self.subTest(workshop=bad):
                self.assertNotEqual(_run("https://blockfield.pro", bad).returncode, 0)


if __name__ == "__main__":
    unittest.main()
