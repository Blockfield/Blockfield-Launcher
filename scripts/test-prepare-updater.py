import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from urllib.parse import unquote

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "updater", Path(__file__).with_name("prepare-updater.py")
)
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class PrepareUpdaterTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.manifest = self.root / "latest.json"
        self.original = {
            "version": "1.0.1",
            "platforms": {"obsolete-platform": {}},
            "notes": "release",
            "pub_date": "2026-10-04T12:00:00Z",
        }
        self.manifest.write_text(json.dumps(self.original))
        self.assets = {
            "linux-x86_64": "blockfield-launcher_1.0.1_amd64.AppImage",
            "windows-x86_64": "blockfield-launcher_1.0.1_x64_en-US.msi",
            "darwin-aarch64": "blockfield-launcher_1.0.1_aarch64.app.tar.gz",
            "darwin-x86_64": "blockfield-launcher_1.0.1_x64.app.tar.gz",
            "windows-x86_64-nsis": "blockfield-launcher_1.0.1_x64-setup.exe",
            "linux-x86_64-deb": "blockfield-launcher_1.0.1_amd64.deb",
            "linux-x86_64-rpm": "blockfield-launcher-1.0.1-1.x86_64.rpm",
        }
        for name in self.assets.values():
            (self.root / name).write_bytes(b"bundle")
            (self.root / (name + ".sig")).write_text("signature-" + name + "\n")

    def test_reconstructs_signed_platforms_with_versioned_mac_archives(self):
        updater.prepare(self.root, "1.0.1")
        data = json.loads(self.manifest.read_text())
        self.assertEqual(len(data["platforms"]), 11)
        self.assertNotIn("obsolete-platform", data["platforms"])
        for key in ("version", "notes", "pub_date"):
            self.assertEqual(data[key], self.original[key])
        base = "https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v1.0.1/"
        for platform, name in self.assets.items():
            with self.subTest(platform=platform):
                entry = data["platforms"][platform]
                self.assertEqual(entry["signature"], "signature-" + name)
                self.assertEqual(unquote(entry["url"]), base + name)
        for platform, bundle in (
            ("linux-x86_64", "appimage"),
            ("windows-x86_64", "msi"),
            ("darwin-x86_64", "app"),
            ("darwin-aarch64", "app"),
        ):
            self.assertEqual(
                data["platforms"][platform], data["platforms"][f"{platform}-{bundle}"]
            )

    def test_rejects_a_different_version_without_changing_manifest(self):
        original = self.manifest.read_bytes()
        with self.assertRaisesRegex(ValueError, "version does not match"):
            updater.prepare(self.root, "1.0.2")
        self.assertEqual(self.manifest.read_bytes(), original)

    def test_rejects_each_missing_bundle_without_changing_manifest(self):
        original = self.manifest.read_bytes()
        for name in self.assets.values():
            with self.subTest(asset=name):
                asset = self.root / name
                content = asset.read_bytes()
                asset.unlink()
                with self.assertRaisesRegex(ValueError, "Missing updater asset"):
                    updater.prepare(self.root, "1.0.1")
                self.assertEqual(self.manifest.read_bytes(), original)
                asset.write_bytes(content)

    def test_rejects_each_empty_signature_without_changing_manifest(self):
        original = self.manifest.read_bytes()
        for name in self.assets.values():
            with self.subTest(asset=name):
                signature = self.root / (name + ".sig")
                content = signature.read_text()
                signature.write_text(" \n\t")
                with self.assertRaisesRegex(ValueError, "Empty updater signature"):
                    updater.prepare(self.root, "1.0.1")
                self.assertEqual(self.manifest.read_bytes(), original)
                signature.write_text(content)

    def test_rejects_each_missing_signature_without_changing_manifest(self):
        original = self.manifest.read_bytes()
        for name in self.assets.values():
            with self.subTest(asset=name):
                signature = self.root / (name + ".sig")
                content = signature.read_text()
                signature.unlink()
                with self.assertRaises(FileNotFoundError):
                    updater.prepare(self.root, "1.0.1")
                self.assertEqual(self.manifest.read_bytes(), original)
                signature.write_text(content)


if __name__ == "__main__":
    unittest.main()
