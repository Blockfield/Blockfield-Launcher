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


class PrepareNativeUpdaterTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.version = "1.0.20"
        self.names = {
            "linux-x86_64-deb": f"blockfield-native_{self.version}_amd64.deb",
            "windows-x86_64": f"blockfield-native_{self.version}_x64_en-US.msi",
            "windows-x86_64-nsis": f"blockfield-native_{self.version}_x64-setup.exe",
        }
        for name in self.names.values():
            (self.root / name).write_bytes(b"bundle")
            (self.root / (name + ".sig")).write_text("signature-" + name + "\n")

    def test_builds_native_manifest_without_touching_tauri_files(self):
        tauri = self.root / "latest.json"
        tauri.write_text(json.dumps({"version": self.version}))
        updater.prepare_native(self.root, self.version)
        data = json.loads((self.root / "latest-native.json").read_text())
        self.assertEqual(data["version"], self.version)
        self.assertEqual(set(data["platforms"]), set(self.names))
        base = f"https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{self.version}/"
        for platform, name in self.names.items():
            with self.subTest(platform=platform):
                entry = data["platforms"][platform]
                self.assertEqual(entry["signature"], "signature-" + name)
                self.assertEqual(unquote(entry["url"]), base + name)
        # The Tauri channel is untouched: same bytes, no native keys in it.
        self.assertEqual(json.loads(tauri.read_text()), {"version": self.version})

    def test_rejects_missing_or_empty_native_asset_signature(self):
        for name in self.names.values():
            with self.subTest(asset=name):
                asset = self.root / name
                content = asset.read_bytes()
                asset.unlink()
                with self.assertRaisesRegex(ValueError, "Incomplete native asset set"):
                    updater.prepare_native(self.root, self.version)
                self.assertFalse((self.root / "latest-native.json").exists())
                asset.write_bytes(content)
                signature = self.root / (name + ".sig")
                signature.write_text(" \n\t")
                with self.assertRaisesRegex(ValueError, "Incomplete native asset set"):
                    updater.prepare_native(self.root, self.version)
                self.assertFalse((self.root / "latest-native.json").exists())
                signature.write_text("signature-" + name + "\n")

    def test_partial_set_failure_names_everything_missing(self):
        names = list(self.names.values())
        (self.root / names[0]).unlink()
        (self.root / (names[1] + ".sig")).unlink()
        with self.assertRaisesRegex(
            ValueError, "Incomplete native asset set"
        ) as raised:
            updater.prepare_native(self.root, self.version)
        self.assertIn(names[0], str(raised.exception))
        self.assertIn(names[1] + ".sig", str(raised.exception))
        self.assertFalse((self.root / "latest-native.json").exists())

    def test_empty_directory_fails_without_publishing(self):
        for name in self.names.values():
            (self.root / name).unlink()
            (self.root / (name + ".sig")).unlink()
        with self.assertRaisesRegex(ValueError, "Incomplete native asset set"):
            updater.prepare_native(self.root, self.version)
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
        # 1.0.9 < 1.0.20 numerically, but "9" > "2" lexicographically.
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
        # Semver §11: 1.0.20-rc.1 < 1.0.20, so the stable release publishes.
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
        self.assertFalse(newer("1.0.20-rc.1", "1.0.20-rc.1"))
        # Numeric identifiers rank below alphanumeric ones.
        self.assertTrue(newer("1.0.20-1", "1.0.20-alpha"))
        # Build metadata does not affect precedence.
        self.assertFalse(
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
