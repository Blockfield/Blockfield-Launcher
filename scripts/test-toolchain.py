"""Offline regressions for pinned tool caches and native/container selection."""

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolchain  # noqa: E402


class ToolCacheTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.cache = Path(self.directory.name)
        self.patch = patch.object(toolchain, "CACHE", self.cache)
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def test_npm_cache_reuses_matching_pin_and_repairs_stale_or_missing_tools(self):
        def install(*args):
            name, version = args[-1].split("@")
            package = self.cache / "npm/node_modules" / name / "package.json"
            package.parent.mkdir(parents=True, exist_ok=True)
            package.write_text(json.dumps({"version": version}))
            binary = (
                self.cache
                / "npm/node_modules/.bin"
                / (name + ".cmd" if os.name == "nt" else name)
            )
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.touch()

        with patch.object(toolchain, "call", side_effect=install) as call:
            binary = toolchain.tool("pnpm")
            call.assert_called_once()
            self.assertIn("--ignore-scripts", call.call_args.args)
            self.assertEqual(toolchain.tool("pnpm"), binary)
            self.assertEqual(call.call_count, 1)
            package = self.cache / "npm/node_modules/pnpm/package.json"
            for metadata in ('{"version":"0.0.0"}', "invalid json"):
                with self.subTest(metadata=metadata):
                    package.write_text(metadata)
                    previous = call.call_count
                    toolchain.tool("pnpm")
                    self.assertEqual(call.call_count, previous + 1)
            binary.unlink()
            toolchain.tool("pnpm")
            self.assertEqual(call.call_count, 4)

    def test_download_checks_new_bytes_before_caching_and_reuses_verified_cache(self):
        payload = b"verified tool"
        checksum = hashlib.sha256(payload).hexdigest()
        with patch.object(
            toolchain.urllib.request, "urlopen", return_value=io.BytesIO(payload)
        ) as request:
            target = toolchain.download(
                "tool", "https://example.invalid/tool", checksum
            )
            self.assertEqual(target.read_bytes(), payload)
            self.assertEqual(
                toolchain.download("tool", "https://example.invalid/tool", checksum),
                target,
            )
            request.assert_called_once()
        with patch.object(
            toolchain.urllib.request, "urlopen", return_value=io.BytesIO(b"corrupt")
        ):
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                toolchain.download("bad-tool", "https://example.invalid/tool", checksum)
        self.assertFalse((self.cache / "bad-tool").exists())

    def test_corrupt_cached_download_is_rejected_without_network(self):
        (self.cache / "tool").write_bytes(b"corrupt")
        with patch.object(toolchain.urllib.request, "urlopen") as request:
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                toolchain.download(
                    "tool",
                    "https://example.invalid/tool",
                    hashlib.sha256(b"ok").hexdigest(),
                )
            request.assert_not_called()

    def test_host_recognizes_darwin_architectures(self):
        for machine, arch in (
            ("arm64", "arm64"),
            ("aarch64", "arm64"),
            ("x86_64", "amd64"),
            ("amd64", "amd64"),
        ):
            with self.subTest(machine=machine):
                with (
                    patch.object(toolchain.platform, "system", return_value="Darwin"),
                    patch.object(toolchain.platform, "machine", return_value=machine),
                ):
                    self.assertEqual(toolchain.host(), ("darwin", arch))

    def test_canonical_python_pins_each_platform_compressor(self):
        for system, compressor in (
            ("darwin", "1.2.12"),
            ("linux", "1.3.2"),
            ("windows", "1.3.2"),
        ):
            with self.subTest(system=system):
                python = self.cache / f"python-3.12.15-{system}-arm64" / "python"
                python /= "python.exe" if os.name == "nt" else "bin/python3"
                python.parent.mkdir(parents=True)
                python.touch()
                with (
                    patch.object(toolchain, "host", return_value=(system, "arm64")),
                    patch.object(toolchain.sys, "version_info", (3, 14, 8)),
                    patch.object(toolchain, "python_version") as version,
                    patch.object(toolchain, "download") as download,
                ):
                    version.return_value = f"(3, 12, 15) {compressor} {compressor}"
                    self.assertEqual(toolchain.canonical_python(), python)
                    version.return_value = "(3, 12, 15) 0.0.0 0.0.0"
                    with self.assertRaisesRegex(RuntimeError, "compressor mismatch"):
                        toolchain.canonical_python()
                    download.assert_not_called()

    def test_darwin_pins_cover_both_architectures(self):
        for key, fragment in (
            ("darwin-arm64", "aarch64-apple-darwin-install_only.tar.gz"),
            ("darwin-amd64", "x86_64-apple-darwin-install_only.tar.gz"),
        ):
            with self.subTest(key=key):
                url, checksum = toolchain.PYTHON_ASSETS[key]
                self.assertIn(fragment, url)
                self.assertRegex(checksum, r"^[0-9a-f]{64}$")
        for name, suffix in (
            ("actionlint", ".tar.gz"),
            ("shfmt", ""),
            ("shellcheck", ".tar.gz"),
            ("just", ".tar.gz"),
        ):
            for arch in ("arm64", "amd64"):
                with self.subTest(tool=name, arch=arch):
                    url, checksum = toolchain.BINARY_ASSETS[f"{name}-darwin-{arch}"]
                    self.assertRegex(checksum, r"^[0-9a-f]{64}$")
                    self.assertFalse(url.endswith(".exe"))
                    if suffix:
                        self.assertTrue(url.endswith(suffix))
                    else:
                        self.assertFalse(url.endswith((".tar.gz", ".zip")))

    def test_darwin_tool_extraction_uses_tar_member_and_raw_bytes(self):
        def make_tar(path, member, payload):
            with tarfile.open(path, "w:gz") as archive:
                info = tarfile.TarInfo(member)
                info.size = len(payload)
                info.mode = 0o755
                archive.addfile(info, io.BytesIO(payload))

        flat = self.cache / "flat.tar.gz"
        make_tar(flat, "actionlint", b"fake-darwin-actionlint")
        nested = self.cache / "nested.tar.gz"
        make_tar(nested, "shellcheck-v0.11.0/shellcheck", b"fake-darwin-shellcheck")
        raw = self.cache / "shfmt-raw"
        raw.write_bytes(b"fake-darwin-shfmt")

        def fake_download(name, url, checksum):
            if name.startswith("actionlint-"):
                return flat
            if name.startswith("shellcheck-"):
                return nested
            return raw

        with (
            patch.object(toolchain, "host", return_value=("darwin", "arm64")),
            patch.object(toolchain, "download", side_effect=fake_download),
        ):
            for name, payload in (
                ("actionlint", b"fake-darwin-actionlint"),
                ("shellcheck", b"fake-darwin-shellcheck"),
                ("shfmt", b"fake-darwin-shfmt"),
            ):
                with self.subTest(tool=name):
                    binary = toolchain.tool(name)
                    self.assertEqual(binary, self.cache / "bin" / name)
                    self.assertEqual(binary.read_bytes(), payload)
                    if os.name == "posix":
                        self.assertTrue(binary.stat().st_mode & 0o111)


@unittest.skipUnless(os.name == "posix", "Linux shell wrapper")
class RustEnvironmentTests(unittest.TestCase):
    def run_wrapper(self, compiler=False, podman=False, configured_compiler=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record = root / "arguments"
            scripts = {
                "uname": "echo Linux",
                "pkg-config": "exit 0",
                "mkdir": "exit 0",
                "cargo": 'printf "%s\\n" "$@" > "$CALL_RECORD"',
            }
            if compiler:
                scripts["custom-cc" if configured_compiler else "cc"] = "exit 0"
            if podman:
                scripts["podman"] = (
                    'if [ "$1 $2" = "image exists" ]; then exit 0; fi\n'
                    'printf "%s\\n" "$@" > "$CALL_RECORD"'
                )
            for name, body in scripts.items():
                executable = root / name
                executable.write_text("#!/bin/sh\n" + body + "\n")
                executable.chmod(0o755)
            (root / "dirname").symlink_to(shutil.which("dirname"))
            env = os.environ.copy()
            env.pop("CC", None)
            env.update(
                PATH=str(root),
                CALL_RECORD=str(record),
                BLOCKFIELD_CARGO_TARGET=str(root / "target"),
            )
            if configured_compiler:
                env["CC"] = str(root / "custom-cc")
            result = subprocess.run(
                [
                    shutil.which("bash"),
                    str(Path(__file__).with_name("rust-env.sh")),
                    "cargo",
                    "check",
                    "--locked",
                ],
                env=env,
                text=True,
                capture_output=True,
                timeout=10,
            )
            return result, record.read_text().splitlines() if record.exists() else []

    def test_native_build_requires_available_compiler_and_honors_cc(self):
        for configured in (False, True):
            with self.subTest(configured_compiler=configured):
                result, args = self.run_wrapper(
                    compiler=True, configured_compiler=configured
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(args, ["check", "--locked"])

    def test_missing_compiler_uses_existing_container_image(self):
        result, args = self.run_wrapper(podman=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(args[0], "run")
        self.assertEqual(args[-3:], ["cargo", "check", "--locked"])

    def test_unavailable_build_environment_fails_instead_of_skipping_checks(self):
        result, args = self.run_wrapper()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("native Tauri build environment", result.stderr)
        self.assertEqual(args, [])


if __name__ == "__main__":
    unittest.main()
