import importlib.util
import io
import os
import re
import struct
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
appimage_pkg = _load(
    "package_native_appimage", ROOT / "scripts" / "package-native-appimage.py"
)
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


def _fake_elf(appimage=False):
    elf = bytearray(65)
    elf[0:4] = b"\x7fELF"
    elf[4] = 2
    elf[5] = 1
    if appimage:
        elf[8:11] = b"AI\x02"
    elf[18:20] = (62).to_bytes(2, "little")
    return bytes(elf)


class PackageNativeDebTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binary = self.root / "blockfield-native"
        self.binary.write_bytes(_fake_elf())

    def test_builds_valid_deb_matching_updater_contract(self):
        out = deb_pkg.build_deb(self.binary, "2.0.1", self.root / "out")
        self.assertEqual(out.name, "blockfield-launcher_2.0.1_amd64.deb")
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
        self.assertIn("Package: blockfield-launcher\n", control)
        self.assertIn("Replaces: blockfield-native\n", control)
        self.assertIn("Version: 2.0.1\n", control)
        self.assertIn("Architecture: amd64\n", control)

        with tarfile.open(
            fileobj=io.BytesIO(by_name["data.tar.gz"]), mode="r:gz"
        ) as tar:
            entries = {m.name: m for m in tar.getmembers()}
            self.assertIn("./usr/bin/blockfield-launcher", entries)
            self.assertEqual(entries["./usr/bin/blockfield-launcher"].mode, 0o755)
            self.assertEqual(
                tar.extractfile("./usr/bin/blockfield-launcher").read(),
                self.binary.read_bytes(),
            )
            desktop = (
                tar.extractfile("./usr/share/applications/blockfield-launcher.desktop")
                .read()
                .decode("utf-8")
            )
            self.assertIn("Exec=/usr/bin/blockfield-launcher %U\n", desktop)
            self.assertIn(
                "MimeType=x-scheme-handler/blockfield;x-scheme-handler/discord-1548300877125779456;\n",
                desktop,
            )
            icon_bytes = tar.extractfile(
                "./usr/share/icons/hicolor/256x256/apps/blockfield-launcher.png"
            ).read()
            self.assertEqual(
                icon_bytes,
                (ROOT / "native" / "packaging" / "assets" / "256x256.png").read_bytes(),
            )

    def test_dev_deb_installs_side_by_side_without_prod_schemes(self):
        out = deb_pkg.build_deb(self.binary, "2.0.1", self.root / "out", dev=True)
        self.assertEqual(out.name, "blockfield-launcher-dev_2.0.1_amd64.deb")
        by_name = dict(_parse_ar(out.read_bytes()))
        with tarfile.open(
            fileobj=io.BytesIO(by_name["control.tar.gz"]), mode="r:gz"
        ) as tar:
            control = tar.extractfile("./control").read().decode("utf-8")
        self.assertIn("Package: blockfield-launcher-dev\n", control)
        self.assertNotIn("Replaces:", control)
        with tarfile.open(
            fileobj=io.BytesIO(by_name["data.tar.gz"]), mode="r:gz"
        ) as tar:
            desktop = (
                tar.extractfile(
                    "./usr/share/applications/blockfield-launcher-dev.desktop"
                )
                .read()
                .decode("utf-8")
            )
        self.assertIn("Name=Blockfield Launcher DEV\n", desktop)
        self.assertNotIn("MimeType=", desktop)

    def test_rejects_bad_version_and_empty_binary(self):
        with self.assertRaises(ValueError):
            deb_pkg.build_deb(self.binary, "bad version", self.root)
        empty = self.root / "empty"
        empty.write_bytes(b"")
        with self.assertRaises(ValueError):
            deb_pkg.build_deb(empty, "2.0.1", self.root)


class PackageNativeAppImageTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binary = self.root / "blockfield-native"
        self.binary.write_bytes(_fake_elf())
        self.runtime = self.root / "runtime-x86_64"
        self.runtime.write_bytes(_fake_elf(appimage=True))

    def _fake_mksquashfs(self, capture_dir):
        # POSIX stub script that records AppDir layout and emits a fake squashfs marker.
        script = self.root / "fake-mksquashfs"
        script.write_text(
            "#!/bin/sh\n"
            "set -eu\n"
            'src="$1"\n'
            'dst="$2"\n'
            f'cp -a "$src/." "{capture_dir}/"\n'
            'printf "hsqsFAKE_SQUASHFS" > "$dst"\n',
            encoding="utf-8",
        )
        script.chmod(0o755)
        return script

    @unittest.skipUnless(os.name == "posix", "POSIX fake mksquashfs")
    def test_builds_type2_appimage_with_expected_appdir(self):
        captured = self.root / "captured-appdir"
        captured.mkdir()
        out = appimage_pkg.build_appimage(
            self.binary,
            "2.0.1",
            self.root / "out",
            runtime_path=self.runtime,
            mksquashfs_bin=self._fake_mksquashfs(captured),
        )
        self.assertEqual(out.name, "blockfield-launcher_2.0.1_amd64.AppImage")
        raw = out.read_bytes()
        self.assertTrue(raw.startswith(self.runtime.read_bytes()))
        self.assertTrue(raw.endswith(b"hsqsFAKE_SQUASHFS"))
        self.assertTrue(out.stat().st_mode & 0o111)

        apprun = (captured / "AppRun").read_text(encoding="utf-8")
        self.assertIn('exec "$HERE/usr/bin/blockfield-launcher" "$@"', apprun)
        self.assertTrue((captured / "usr/bin/blockfield-launcher").is_file())
        self.assertTrue((captured / ".DirIcon").is_file())
        self.assertTrue((captured / "blockfield-launcher.png").is_file())
        desktop = (captured / "blockfield-launcher.desktop").read_text(encoding="utf-8")
        self.assertIn("X-AppImage-Version=2.0.1\n", desktop)
        self.assertIn("StartupWMClass=blockfield-native\n", desktop)
        self.assertIn(
            "MimeType=x-scheme-handler/blockfield;x-scheme-handler/discord-1548300877125779456;\n",
            desktop,
        )

    @unittest.skipUnless(os.name == "posix", "POSIX fake mksquashfs")
    def test_dev_appimage_omits_production_schemes(self):
        captured = self.root / "captured-dev"
        captured.mkdir()
        out = appimage_pkg.build_appimage(
            self.binary,
            "2.0.1",
            self.root / "out",
            dev=True,
            runtime_path=self.runtime,
            mksquashfs_bin=self._fake_mksquashfs(captured),
        )
        self.assertEqual(out.name, "blockfield-launcher-dev_2.0.1_amd64.AppImage")
        desktop = (captured / "blockfield-launcher-dev.desktop").read_text(
            encoding="utf-8"
        )
        self.assertIn("Name=Blockfield Launcher DEV\n", desktop)
        self.assertNotIn("MimeType=", desktop)

    def test_rejects_non_appimage_runtime(self):
        bad_runtime = self.root / "not-runtime"
        bad_runtime.write_bytes(_fake_elf(appimage=False))
        with self.assertRaisesRegex(ValueError, "Invalid AppImage type-2 runtime"):
            appimage_pkg.validate_runtime_bytes(bad_runtime.read_bytes())


class PackageNativeWindowsTests(unittest.TestCase):
    def test_normalizes_semver_for_msi_and_nsis(self):
        self.assertEqual(win_pkg.normalize_msi_version("2.0.1"), "2.0.1")
        self.assertEqual(win_pkg.normalize_msi_version("2.0.1-rc.1"), "2.0.1")
        self.assertEqual(win_pkg.normalize_nsis_quad("2.0.1"), "2.0.1.0")
        with self.assertRaises(ValueError):
            win_pkg.normalize_msi_version("not-a-version")
        with self.assertRaises(ValueError):
            win_pkg.normalize_msi_version("256.0.0")

    def test_branding_assets_are_valid_ico_and_24bit_bmps(self):
        assets = ROOT / "native" / "packaging" / "assets"
        ico = (assets / "icon.ico").read_bytes()
        self.assertEqual(struct.unpack_from("<H", ico, 2)[0], 1)
        for name, width, height in (
            ("header.bmp", 150, 57),
            ("sidebar.bmp", 164, 314),
        ):
            bmp = (assets / name).read_bytes()
            self.assertEqual(bmp[:2], b"BM")
            self.assertEqual(struct.unpack_from("<i", bmp, 18)[0], width)
            self.assertEqual(struct.unpack_from("<i", bmp, 22)[0], height)
            self.assertEqual(struct.unpack_from("<H", bmp, 28)[0], 24)

    def test_nsis_and_wix_installers_keep_safe_uninstall_and_schemes(self):
        pkg = ROOT / "native" / "packaging" / "windows"
        nsis = (pkg / "installer.nsi").read_text(encoding="utf-8")
        wxs = (pkg / "installer.wxs").read_text(encoding="utf-8")

        self.assertNotRegex(nsis, r"(?i)\bRmDir\s+/r\b")
        self.assertRegex(nsis, r'(?m)^!include "Win\\RestartManager\.nsh"$')
        rm_calls = re.findall(r"(?m)^\s*!insertmacro CheckIfAppIsRunning (.+)$", nsis)
        self.assertEqual(len(rm_calls), 2)
        for args in rm_calls:
            self.assertEqual(args, '"$INSTDIR\\${MAINBINARYNAME}.exe"')
        self.assertRegex(
            nsis, r"\$UpdateMode <> 1[\s\S]*ExecWait[^\n]*--uninstall-cleanup"
        )
        self.assertRegex(nsis, r"\$0 <> 0[\s\S]*Abort")
        self.assertIn('!insertmacro RegisterScheme "blockfield"', nsis)
        self.assertIn('!insertmacro RegisterScheme "discord-${DISCORD_APP_ID}"', nsis)
        self.assertIn('!define DISCORD_APP_ID "1548300877125779456"', nsis)
        self.assertIn('!insertmacro UnregisterScheme "blockfield"', nsis)
        self.assertIn('!insertmacro UnregisterScheme "discord-${DISCORD_APP_ID}"', nsis)
        self.assertIn("LangString BlockfieldCleanupFailed ${LANG_RUSSIAN}", nsis)
        self.assertIn('!define PRODUCTNAME "blockfield-launcher"', nsis)
        self.assertIn('!define PRODUCTNAME "blockfield-launcher-dev"', nsis)

        self.assertIn('InstallScope="perUser"', wxs)
        self.assertNotIn("InstallPrivileges=", wxs)
        self.assertRegex(wxs, r'<MajorUpgrade\b[\s\S]*AllowSameVersionUpgrades="yes"')
        self.assertRegex(wxs, r'<MediaTemplate\s+EmbedCab="yes"\s*/>')
        self.assertIn('Key="Software\\Classes\\blockfield"', wxs)
        self.assertIn('Key="Software\\Classes\\discord-$(var.DiscordAppId)"', wxs)
        self.assertRegex(
            wxs,
            r'<CustomAction\s+Id="RunUninstallCleanup"[\s\S]*FileKey="MainExecutable"[\s\S]*ExeCommand="--uninstall-cleanup"[\s\S]*Execute="deferred"[\s\S]*Impersonate="yes"[\s\S]*Return="check"',
        )
        self.assertRegex(
            wxs,
            r'<Custom\s+Action="RunUninstallCleanup"\s+Before="RemoveFiles">REMOVE="ALL" AND NOT UPGRADINGPRODUCTCODE</Custom>',
        )


if __name__ == "__main__":
    unittest.main()
