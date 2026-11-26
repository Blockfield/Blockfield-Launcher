import importlib.util
import io
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


deb_pkg = _load("package_native_deb", ROOT / "scripts" / "package-native-deb.py")
win_pkg = _load(
    "package_native_windows", ROOT / "scripts" / "package-native-windows.py"
)


def _parse_ar(data):
    if not data.startswith(b"!<arch>\n"):
        raise AssertionError("missing ar magic")
    pos = 8
    members = []
    while pos < len(data):
        header = data[pos : pos + 60]
        if len(header) != 60 or header[58:60] != b"`\n":
            raise AssertionError(f"corrupt ar header at offset {pos}")
        name = header[0:16].decode("ascii").rstrip(" /")
        size = int(header[48:58].decode("ascii").strip())
        pos += 60
        payload = data[pos : pos + size]
        if len(payload) != size:
            raise AssertionError(f"truncated ar member {name}")
        pos += size
        if size % 2:
            if data[pos : pos + 1] != b"\n":
                raise AssertionError(f"missing ar odd-byte padding after {name}")
            pos += 1
        members.append((name, payload))
    return members


class PackageNativeDebTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # Minimal valid 64-byte x86_64 ELF header + odd length to exercise ar padding.
        elf = bytearray(65)
        elf[0:4] = b"\x7fELF"
        elf[4] = 2
        elf[5] = 1
        elf[18:20] = (62).to_bytes(2, "little")
        self.binary = self.root / "blockfield-native"
        self.binary.write_bytes(bytes(elf))

    def test_builds_valid_deb_matching_updater_contract(self):
        out = deb_pkg.build_deb(self.binary, "1.0.31", self.root / "out")
        self.assertEqual(out.name, "blockfield-native_1.0.31_amd64.deb")
        members = _parse_ar(out.read_bytes())
        self.assertEqual(
            [name for name, _ in members],
            ["debian-binary", "control.tar.gz", "data.tar.gz"],
        )
        by_name = dict(members)
        self.assertEqual(by_name["debian-binary"], b"2.0\n")

        with tarfile.open(
            fileobj=io.BytesIO(by_name["control.tar.gz"]), mode="r:gz"
        ) as tar:
            control = tar.extractfile("./control").read().decode("utf-8")
        self.assertIn("Package: blockfield-native\n", control)
        self.assertIn("Version: 1.0.31\n", control)
        self.assertIn("Architecture: amd64\n", control)

        with tarfile.open(
            fileobj=io.BytesIO(by_name["data.tar.gz"]), mode="r:gz"
        ) as tar:
            entries = {m.name: m for m in tar.getmembers()}
            self.assertIn("./usr/bin/blockfield-native", entries)
            self.assertEqual(entries["./usr/bin/blockfield-native"].mode, 0o755)
            self.assertEqual(
                tar.extractfile("./usr/bin/blockfield-native").read(),
                self.binary.read_bytes(),
            )
            desktop = (
                tar.extractfile("./usr/share/applications/blockfield-native.desktop")
                .read()
                .decode("utf-8")
            )
            self.assertIn("Exec=/usr/bin/blockfield-native %U\n", desktop)
            self.assertIn(
                "MimeType=x-scheme-handler/blockfield;x-scheme-handler/discord-1548300877125779456;\n",
                desktop,
            )
            icon_bytes = tar.extractfile(
                "./usr/share/icons/hicolor/256x256/apps/blockfield-native.png"
            ).read()
            self.assertEqual(
                icon_bytes,
                (ROOT / "src-tauri" / "icons" / "256x256.png").read_bytes(),
            )

    def test_rejects_bad_version_and_empty_binary(self):
        with self.assertRaises(ValueError):
            deb_pkg.build_deb(self.binary, "bad version", self.root)
        empty = self.root / "empty"
        empty.write_bytes(b"")
        with self.assertRaises(ValueError):
            deb_pkg.build_deb(empty, "1.0.31", self.root)


class PackageNativeWindowsHelperTests(unittest.TestCase):
    def test_normalizes_semver_for_msi_and_nsis(self):
        self.assertEqual(win_pkg.normalize_msi_version("1.0.31"), "1.0.31")
        self.assertEqual(win_pkg.normalize_msi_version("1.0.31-rc.1"), "1.0.31")
        self.assertEqual(win_pkg.normalize_nsis_quad("1.0.31"), "1.0.31.0")
        with self.assertRaises(ValueError):
            win_pkg.normalize_msi_version("not-a-version")
        with self.assertRaises(ValueError):
            win_pkg.normalize_msi_version("256.0.0")


if __name__ == "__main__":
    unittest.main()
