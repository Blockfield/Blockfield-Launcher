"""Pinned project-local quality tools and reproducible build Python."""

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / ".cache" / "quality"
PYTHON_ASSETS = {
    "linux-arm64": [
        "https://github.com/astral-sh/python-build-standalone/releases/download/20261003/cpython-3.12.15%2B20261003-aarch64-unknown-linux-gnu-install_only.tar.gz",
        "95c01982c9fcb9d95b0acfdb5eb8a6e0099dd11edf062314a474228d93f2b765",
    ],
    "linux-amd64": [
        "https://github.com/astral-sh/python-build-standalone/releases/download/20261003/cpython-3.12.15%2B20261003-x86_64-unknown-linux-gnu-install_only.tar.gz",
        "f937814031eab4698ca6d07ec606ede1825768f3f3e99af76d9db3900bee03c5",
    ],
    "windows-arm64": [
        "https://github.com/astral-sh/python-build-standalone/releases/download/20261003/cpython-3.12.15%2B20261003-aarch64-pc-windows-msvc-install_only.tar.gz",
        "2de96a340fecdabe215094345ed3aea1b473e858bd098360f2253299e18f24d2",
    ],
    "windows-amd64": [
        "https://github.com/astral-sh/python-build-standalone/releases/download/20261003/cpython-3.12.15%2B20261003-x86_64-pc-windows-msvc-install_only.tar.gz",
        "4b6f0beebbb695a0f3ea237b8c3eaa5bd424f47a7bc25b2fbe3a43390c770f08",
    ],
}
BINARY_ASSETS = {
    "actionlint-linux-amd64": [
        "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_amd64.tar.gz",
        "8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8",
    ],
    "actionlint-linux-arm64": [
        "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_arm64.tar.gz",
        "325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6",
    ],
    "actionlint-windows-amd64": [
        "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_windows_amd64.zip",
        "6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9",
    ],
    "actionlint-windows-arm64": [
        "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_windows_arm64.zip",
        "cadcf7ea4efe3a68728893813643cebe1185e5b1d4be5b96245f65c9a4d5ea41",
    ],
    "shfmt-linux-amd64": [
        "https://github.com/mvdan/sh/releases/download/v3.14.1/shfmt_v3.14.1_linux_amd64",
        "76e77641faa025814b77f153b29796b8e6fa2fca03e0c76a691608b86c7ea7bf",
    ],
    "shfmt-linux-arm64": [
        "https://github.com/mvdan/sh/releases/download/v3.14.1/shfmt_v3.14.1_linux_arm64",
        "5f2db09dae91fca848f7adbdd014632e921a383863a2ad7e0450ad3aba0c6489",
    ],
    "shfmt-windows-amd64": [
        "https://github.com/mvdan/sh/releases/download/v3.14.1/shfmt_v3.14.1_windows_amd64.exe",
        "13629ce28442ca80b6b5a819f7574ab39e1c28c6e26734ca816c9714e04851df",
    ],
    "shellcheck-linux-arm64": [
        "https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.aarch64.tar.gz",
        "68a8133197a50beb8803f8d42f9908d1af1c5540d4bb05fdfca8c1fa47decefc",
    ],
    "shellcheck-linux-amd64": [
        "https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.x86_64.tar.gz",
        "b7af85e41cc99489dcc21d66c6d5f3685138f06d34651e6d34b42ec6d54fe6f6",
    ],
    "shellcheck-windows-amd64": [
        "https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.zip",
        "8a4e35ab0b331c85d73567b12f2a444df187f483e5079ceffa6bda1faa2e740e",
    ],
    "just-windows-arm64": [
        "https://github.com/casey/just/releases/download/1.58.0/just-1.58.0-aarch64-pc-windows-msvc.zip",
        "3a39ed629eb67678976c811a4da46f7985a2c22f4dbabe017b8b2eb5ceb5d01c",
    ],
    "just-linux-arm64": [
        "https://github.com/casey/just/releases/download/1.58.0/just-1.58.0-aarch64-unknown-linux-musl.tar.gz",
        "748237128c4c40cbdabc65e841d05ceba13cc23a91eaba395495894c1d9764df",
    ],
    "just-windows-amd64": [
        "https://github.com/casey/just/releases/download/1.58.0/just-1.58.0-x86_64-pc-windows-msvc.zip",
        "759f16fb7aa17c5c8b9594b6d4a8c1a6630dfd042cf2b3ff84841454d3d188dc",
    ],
    "just-linux-amd64": [
        "https://github.com/casey/just/releases/download/1.58.0/just-1.58.0-x86_64-unknown-linux-musl.tar.gz",
        "4a5cc2f53e6f0f8c59092a6cc38291eb729d46a7dd95d3ae582008881b84931d",
    ],
}


def child_env():
    env = os.environ.copy()
    for key in (
        "APPIMAGE",
        "__MISE_SHIM",
        "__MISE_DIFF",
        "PYTHONHOME",
        "PYTHONPATH",
        "JAVA_TOOL_OPTIONS",
        "_JAVA_OPTIONS",
        "JDK_JAVA_OPTIONS",
        "CLASSPATH",
    ):
        env.pop(key, None)
    env["PYTHONUTF8"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"
    sdk = Path(env.get("JAVA_HOME", "")) / "bin"
    env["PATH"] = os.pathsep.join(
        [
            str(CACHE / "bin"),
            *([str(sdk)] if env.get("JAVA_HOME") and sdk.is_dir() else []),
            *(
                entry
                for entry in env.get("PATH", "").split(os.pathsep)
                if not (
                    Path(entry).name.lower() == "shims"
                    and Path(entry).parent.name.lower() == "mise"
                )
            ),
        ]
    )
    return env


def native(name):
    executable = shutil.which(str(name), path=child_env()["PATH"])
    if executable is None:
        raise RuntimeError(
            f"Missing native {name}; install the repository's documented SDK prerequisites"
        )
    return executable


def call(*args, capture=False):
    executable = native(args[0])
    return subprocess.run(
        [executable, *map(str, args[1:])],
        check=True,
        env=child_env(),
        text=True,
        stdout=subprocess.PIPE if capture else sys.stderr,
        shell=os.name == "nt" and Path(executable).suffix.lower() in (".cmd", ".bat"),
    ).stdout


def host():
    system = platform.system().lower()
    machine = platform.machine().lower()
    arch = {
        "amd64": "amd64",
        "x86_64": "amd64",
        "arm64": "arm64",
        "aarch64": "arm64",
    }.get(machine)
    if system not in ("linux", "windows") or arch is None:
        raise RuntimeError(f"Unsupported platform: {system}/{machine}")
    return system, arch


def download(name, url, checksum):
    target = CACHE / name
    if target.is_file():
        data = target.read_bytes()
    else:
        with urllib.request.urlopen(url, timeout=60) as response:
            data = response.read()
    if hashlib.sha256(data).hexdigest() != checksum:
        raise RuntimeError(f"Tool checksum mismatch: {name}")
    if not target.is_file():
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return target


def python_version(python):
    return call(
        python,
        "-I",
        "-X",
        "utf8",
        "-c",
        "import sys,zlib; print(sys.version_info[:3],zlib.ZLIB_VERSION,zlib.ZLIB_RUNTIME_VERSION)",
        capture=True,
    ).strip()


def canonical_python():
    if sys.version_info < (3, 12):
        raise RuntimeError("Install native Python 3.12 or later to run project tools")
    if sys.version_info[:3] == (3, 12, 15) and (
        zlib.ZLIB_VERSION,
        zlib.ZLIB_RUNTIME_VERSION,
    ) == ("1.3.2", "1.3.2"):
        return Path(sys.executable)
    key = "-".join(host())
    directory = CACHE / ("python-3.12.15-" + key)
    relative = Path("python") / ("python.exe" if os.name == "nt" else "bin/python3")
    python = directory / relative
    if not python.is_file():
        archive = download("python-3.12.15-" + key + ".tar.gz", *PYTHON_ASSETS[key])
        with tempfile.TemporaryDirectory(dir=CACHE) as temporary:
            staging = Path(temporary) / "install"
            staging.mkdir()
            with tarfile.open(archive) as source:
                source.extractall(staging, filter="data")
            version = python_version(staging / relative)
            if version != "(3, 12, 15) 1.3.2 1.3.2":
                raise RuntimeError(f"Canonical Python compressor mismatch: {version}")
            staging.rename(directory)
    version = python_version(python)
    if version != "(3, 12, 15) 1.3.2 1.3.2":
        raise RuntimeError(f"Canonical Python compressor mismatch: {version}")
    return python


def tool(name):
    if name == "ruff":
        directory = CACHE / "venv-3.12.15"
        python = directory / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        if not python.is_file():
            call(canonical_python(), "-I", "-m", "venv", "--clear", directory)
        probe = subprocess.run(
            [str(python), "-I", "-m", "ruff", "--version"],
            env=child_env(),
            capture_output=True,
            text=True,
        )
        if probe.returncode or probe.stdout.strip() != "ruff 0.16.10":
            call(
                python,
                "-I",
                "-m",
                "pip",
                "--isolated",
                "install",
                "--disable-pip-version-check",
                "ruff==0.16.10",
            )
        return directory / ("Scripts/ruff.exe" if os.name == "nt" else "bin/ruff")
    if name in ("prettier", "pnpm"):
        version = {"prettier": "3.9.9", "pnpm": "12.9.1"}[name]
        directory = CACHE / "npm"
        binary = (
            directory
            / "node_modules"
            / ".bin"
            / (name + ".cmd" if os.name == "nt" else name)
        )
        package = directory / "node_modules" / name / "package.json"
        try:
            installed_version = json.loads(package.read_text()).get("version")
        except (OSError, ValueError):
            installed_version = None
        if not binary.is_file() or installed_version != version:
            call(
                "npm",
                "install",
                "--prefix",
                directory,
                "--no-audit",
                "--no-fund",
                "--ignore-scripts",
                "--package-lock=false",
                "--save-exact",
                name + "@" + version,
            )
        return binary
    system, arch = host()
    # ShellCheck/shfmt publish amd64 Windows binaries, supported by Windows 11 ARM64 emulation.
    if system == "windows" and name in ("shellcheck", "shfmt"):
        arch = "amd64"
    url, checksum = BINARY_ASSETS[name + "-" + system + "-" + arch]
    archive = download(name + "-" + Path(url).name, url, checksum)
    filename = name + (".exe" if system == "windows" else "")
    if url.endswith(".zip"):
        with zipfile.ZipFile(archive) as source:
            member = next(n for n in source.namelist() if Path(n).name == filename)
            data = source.read(member)
    elif url.endswith(".tar.gz"):
        with tarfile.open(archive) as source:
            member = next(
                m for m in source.getmembers() if Path(m.name).name == filename
            )
            data = source.extractfile(member).read()
    else:
        data = archive.read_bytes()
    binary = CACHE / "bin" / filename
    binary.parent.mkdir(parents=True, exist_ok=True)
    if not binary.is_file() or binary.read_bytes() != data:
        binary.write_bytes(data)
    binary.chmod(0o755)
    return binary


if __name__ == "__main__":
    if sys.argv[1:] == ["--ci"]:
        tool("just")
        python = canonical_python()
        with Path(os.environ["GITHUB_PATH"]).open(
            "a", encoding="utf-8", newline="\n"
        ) as output:
            output.write(str(CACHE / "bin") + "\n" + str(python.parent) + "\n")
    else:
        call(canonical_python(), "-I", "-X", "utf8", *sys.argv[1:])
