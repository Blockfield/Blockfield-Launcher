#!/usr/bin/env python3
"""Capture the RU-only matrix twice, compare static pixels, and index all artifacts."""

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "capture_run", Path(__file__).with_name("run.py")
)
RUN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUN)


def main():
    out = Path(sys.argv[1]).resolve()
    cases = [(state, "1280x720", 1) for state in RUN.STATES]
    cases += [
        (state, "1280x720", scale)
        for state in ("main", "settings-skin")
        for scale in (1.25, 1.5)
    ]
    cases.append(("main", "1600x900", 1))
    failures, comparisons = [], []
    skin_failed = False

    def capture(state, size, scale, destination, record=False):
        stem = f"{state}-{size}@{scale:g}x-ru" + ("-motion" if record else "")
        metadata = destination / f"{stem}.json"
        if metadata.exists():
            existing = json.loads(metadata.read_text())
            for filename, checksum in existing["artifactsSha256"].items():
                assert (
                    hashlib.sha256((destination / filename).read_bytes()).hexdigest()
                    == checksum
                )
            return True
        command = [
            str(ROOT / "scripts/capture/run.sh"),
            state,
            "--size",
            size,
            "--scale",
            str(scale),
            "--lang",
            "ru",
            "--out",
            str(destination),
        ]
        if record:
            command += ["--record", "3", "--fps", "60"]
        result = subprocess.run(command, check=False, timeout=110)
        if result.returncode:
            failures.append(
                {
                    "state": state,
                    "size": size,
                    "scale": scale,
                    "recording": record,
                    "output": str(destination.relative_to(out)),
                    "exitCode": result.returncode,
                }
            )
        return result.returncode == 0

    for state, size, scale in cases:
        if state == "settings-skin" and skin_failed:
            continue
        first = capture(state, size, scale, out)
        if not first and state == "settings-skin":
            skin_failed = True
            continue
        second = capture(state, size, scale, out / "repeat")
        stem = f"{state}-{size}@{scale:g}x-ru.png"
        if first and second:
            result = subprocess.run(
                [
                    "magick",
                    "compare",
                    "-metric",
                    "AE",
                    str(out / stem),
                    str(out / "repeat" / stem),
                    "null:",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            comparisons.append(
                {
                    "state": state,
                    "size": size,
                    "scale": scale,
                    "first": stem,
                    "second": f"repeat/{stem}",
                    "metric": "AE",
                    "value": result.stderr.strip(),
                    "pixelIdentical": result.returncode == 0,
                }
            )
    for state in ("main", "update-42", "settings-skin"):
        if state != "settings-skin" or not skin_failed:
            capture(state, "1280x720", 1, out, record=True)
    entries = []
    for metadata in sorted(out.rglob("*.json")):
        if metadata.name == "manifest.json":
            continue
        data = json.loads(metadata.read_text())
        if not isinstance(data, dict) or "artifactsSha256" not in data:
            continue
        data["metadata"] = str(metadata.relative_to(out))
        artifacts = {}
        for filename, checksum in data["artifactsSha256"].items():
            path = metadata.parent / filename
            assert hashlib.sha256(path.read_bytes()).hexdigest() == checksum, path
            artifacts[str(path.relative_to(out))] = checksum
        data["artifactsSha256"] = artifacts
        for key in ("png", "recording"):
            if data[key]:
                data[key] = str((metadata.parent / data[key]).relative_to(out))
        entries.append(data)
    (out / "manifest.json").write_text(json.dumps(entries, indent=2) + "\n")
    result = {
        "productLanguage": "product is RU-only",
        "failures": failures,
        "skinSkippedAfterFailure": skin_failed,
        "staticComparisons": comparisons,
        "acceptedCaptures": len(entries),
        "pixelIdentical": sum(item["pixelIdentical"] for item in comparisons),
        "residualComparisons": [
            item for item in comparisons if not item["pixelIdentical"]
        ],
    }
    (out / "matrix-status.json").write_text(json.dumps(result, indent=2) + "\n")
    print(
        json.dumps(
            {key: value for key, value in result.items() if key != "staticComparisons"},
            indent=2,
        )
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
