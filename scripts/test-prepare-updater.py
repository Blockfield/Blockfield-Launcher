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
        self.version = "2.0.1"
        self.assets = {
            "linux-x86_64": f"blockfield-launcher_{self.version}_amd64.AppImage",
            "linux-x86_64-deb": f"blockfield-launcher_{self.version}_amd64.deb",
            "windows-x86_64": f"blockfield-launcher_{self.version}_x64_en-US.msi",
            "windows-x86_64-nsis": f"blockfield-launcher_{self.version}_x64-setup.exe",
        }
        for name in self.assets.values():
            (self.root / name).write_bytes(b"bundle")
            (self.root / (name + ".sig")).write_text("signature-" + name + "\n")

    def test_builds_fresh_latest_json_with_all_native_platforms(self):
        updater.prepare(self.root, self.version, notes="Release notes")
        data = json.loads(self.manifest.read_text())
        self.assertEqual(data["version"], self.version)
        self.assertEqual(data["notes"], "Release notes")
        self.assertTrue(data["pub_date"].endswith("Z"))
        self.assertEqual(len(data["platforms"]), 6)
        base = f"https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{self.version}/"
        for platform, name in self.assets.items():
            with self.subTest(platform=platform):
                entry = data["platforms"][platform]
                self.assertEqual(entry["signature"], "signature-" + name)
                self.assertEqual(unquote(entry["url"]), base + name)
        for platform, bundle in (
            ("linux-x86_64", "appimage"),
            ("windows-x86_64", "msi"),
        ):
            self.assertEqual(
                data["platforms"][platform], data["platforms"][f"{platform}-{bundle}"]
            )

    def test_preserves_existing_metadata_and_drops_obsolete_platforms(self):
        original = {
            "version": self.version,
            "platforms": {"obsolete-platform": {}},
            "notes": "release",
            "pub_date": "2026-10-04T12:00:00Z",
        }
        self.manifest.write_text(json.dumps(original))
        updater.prepare(self.root, self.version)
        data = json.loads(self.manifest.read_text())
        self.assertNotIn("obsolete-platform", data["platforms"])
        for key in ("version", "notes", "pub_date"):
            self.assertEqual(data[key], original[key])

    def test_rejects_a_different_version_without_changing_manifest(self):
        self.manifest.write_text(json.dumps({"version": self.version}))
        original = self.manifest.read_bytes()
        with self.assertRaisesRegex(ValueError, "version does not match"):
            updater.prepare(self.root, "2.0.2")
        self.assertEqual(self.manifest.read_bytes(), original)

    def test_rejects_missing_asset_or_signature_without_writing_manifest(self):
        for name in self.assets.values():
            with self.subTest(asset=name):
                asset = self.root / name
                content = asset.read_bytes()
                asset.unlink()
                with self.assertRaisesRegex(ValueError, "Incomplete release asset set"):
                    updater.prepare(self.root, self.version)
                self.assertFalse(self.manifest.exists())
                asset.write_bytes(content)

                signature = self.root / (name + ".sig")
                sig_content = signature.read_text()
                signature.write_text(" \n\t")
                with self.assertRaisesRegex(ValueError, "Incomplete release asset set"):
                    updater.prepare(self.root, self.version)
                self.assertFalse(self.manifest.exists())
                signature.unlink()
                with self.assertRaisesRegex(ValueError, "Incomplete release asset set"):
                    updater.prepare(self.root, self.version)
                self.assertFalse(self.manifest.exists())
                signature.write_text(sig_content)


class PrepareNativeUpdaterTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.version = "2.0.1"
        self.names = {
            "linux-x86_64": f"blockfield-launcher_{self.version}_amd64.AppImage",
            "linux-x86_64-deb": f"blockfield-launcher_{self.version}_amd64.deb",
            "windows-x86_64": f"blockfield-launcher_{self.version}_x64_en-US.msi",
            "windows-x86_64-nsis": f"blockfield-launcher_{self.version}_x64-setup.exe",
        }
        for name in self.names.values():
            (self.root / name).write_bytes(b"bundle")
            (self.root / (name + ".sig")).write_text("signature-" + name + "\n")

    def test_builds_compat_native_manifest_for_all_platforms(self):
        updater.prepare_native(self.root, self.version)
        data = json.loads((self.root / "latest-native.json").read_text())
        self.assertEqual(data["version"], self.version)
        self.assertEqual(len(data["platforms"]), 6)
        base = f"https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{self.version}/"
        for platform, name in self.names.items():
            with self.subTest(platform=platform):
                entry = data["platforms"][platform]
                self.assertEqual(entry["signature"], "signature-" + name)
                self.assertEqual(unquote(entry["url"]), base + name)

    def test_partial_set_failure_names_everything_missing(self):
        names = list(self.names.values())
        (self.root / names[0]).unlink()
        (self.root / (names[1] + ".sig")).unlink()
        with self.assertRaisesRegex(
            ValueError, "Incomplete release asset set"
        ) as raised:
            updater.prepare_native(self.root, self.version)
        self.assertIn(names[0], str(raised.exception))
        self.assertIn(names[1] + ".sig", str(raised.exception))
        self.assertFalse((self.root / "latest-native.json").exists())


class NativeManifestNewerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def write_published(self, version):
        path = self.root / "published-native.json"
        path.write_text(json.dumps({"version": version, "platforms": {}}))
        return path

    def test_newer_version_publishes(self):
        self.assertTrue(
            updater.native_manifest_newer(self.write_published("1.0.20"), "1.0.21")
        )

    def test_same_version_republishes(self):
        self.assertTrue(
            updater.native_manifest_newer(self.write_published("1.0.20"), "1.0.20")
        )

    def test_older_version_skips(self):
        self.assertFalse(
            updater.native_manifest_newer(self.write_published("1.0.21"), "1.0.20")
        )

    def test_numeric_not_lexicographic(self):
        self.assertTrue(
            updater.native_manifest_newer(self.write_published("1.0.9"), "1.0.20")
        )
        self.assertFalse(
            updater.native_manifest_newer(self.write_published("1.0.20"), "1.0.9")
        )

    def test_missing_or_broken_manifest_publishes(self):
        self.assertTrue(
            updater.native_manifest_newer(self.root / "absent.json", "1.0.20")
        )
        broken = self.root / "broken.json"
        broken.write_text("not json")
        self.assertTrue(updater.native_manifest_newer(broken, "1.0.20"))

    def test_stable_beats_its_prerelease(self):
        self.assertTrue(
            updater.native_manifest_newer(self.write_published("1.0.20-rc.1"), "1.0.20")
        )
        self.assertFalse(
            updater.native_manifest_newer(self.write_published("1.0.20"), "1.0.20-rc.1")
        )

    def test_prerelease_identifiers_ordered(self):
        newer = lambda pub, new: updater.native_manifest_newer(
            self.write_published(pub), new
        )
        self.assertTrue(newer("1.0.20-alpha", "1.0.20-alpha.1"))
        self.assertTrue(newer("1.0.20-alpha.1", "1.0.20-beta"))
        self.assertTrue(newer("1.0.20-beta.2", "1.0.20-beta.11"))
        self.assertTrue(newer("1.0.20-beta.11", "1.0.20-rc.1"))
        self.assertTrue(newer("1.0.20-rc.1", "1.0.20-rc.1"))
        self.assertTrue(newer("1.0.20-1", "1.0.20-alpha"))
        self.assertTrue(
            updater.native_manifest_newer(
                self.write_published("1.0.20"), "1.0.20+build.1"
            )
        )


class DownloadErrorIsMissingTests(unittest.TestCase):
    def test_missing_channel_or_asset(self):
        self.assertTrue(updater.download_error_is_missing("HTTP 404: not found"))
        self.assertTrue(
            updater.download_error_is_missing("release not found: native-channel")
        )
        self.assertTrue(
            updater.download_error_is_missing(
                "no assets match the pattern 'latest-native.json'"
            )
        )

    def test_other_failures_are_not_missing(self):
        for log in (
            "HTTP 500: internal server error",
            "network is unreachable",
            "Bad credentials",
            "",
        ):
            with self.subTest(log=log):
                self.assertFalse(updater.download_error_is_missing(log))


if __name__ == "__main__":
    unittest.main()
