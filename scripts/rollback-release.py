#!/usr/bin/env python3
"""Plan and stage a launcher rollback release.

The Tauri updater only moves forward: it offers an update when the feed
version in latest.json is higher than the installed version, never a
downgrade. Rolling back a bad release therefore means publishing a HIGHER
version built from the last-good code, so every client (on the bad or the
good build) updates forward onto good code.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

VERSION_FILES = (
    "package.json",
    "src-tauri/Cargo.toml",
    "src-tauri/tauri.conf.json",
)

_VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


def parse_version(text):
    match = _VERSION_RE.match(text.strip())
    if not match:
        raise ValueError(f"Not a plain X.Y.Z version: {text!r}")
    return tuple(int(part) for part in match.groups())


def update_available(installed, feed):
    """Model of the Tauri updater: forward-only, version-ordered."""
    return parse_version(feed) > parse_version(installed)


def plan_rollback(good, bad, new):
    """Validate the rollback shape good < bad < new and return the steps."""
    versions = {
        name: parse_version(text)
        for name, text in (("good", good), ("bad", bad), ("new", new))
    }
    if not versions["good"] < versions["bad"] < versions["new"]:
        raise ValueError(
            f"Rollback needs good < bad < new, got good={good} bad={bad} new={new}"
        )
    return {
        "branch": f"rollback/launcher-v{new}",
        "tag": f"launcher-v{new}",
        "steps": [
            f"git switch -c rollback/launcher-v{new} origin/main",
            f"git restore --source=launcher-v{good} --staged --worktree -- . "
            "# tree now equals the good tag; main's later commits are gone",
            f"scripts/rollback-release.py --good {good} --bad {bad} --new {new} --apply",
            "pnpm version:check",
            f"git commit -am 'Rollback launcher to {good} code as v{new}'",
            f"git push -u origin rollback/launcher-v{new}  # open a PR to main and merge it",
            "git checkout main && git pull  # main now carries the rollback commit",
            f"scripts/rollback-release.py --good {good} --bad {bad} --new {new} "
            "--verify  # refuses unless main differs from the good tag only in version files",
            f"git tag launcher-v{new}  # tag the main commit: the tag run only publishes main commits",
            f"git push origin launcher-v{new}  # tag run builds, signs and publishes",
            f"gh release download launcher-v{new} --pattern latest.json "
            "--output /tmp/rollback-latest.json && "
            "python -c \"import json;print(json.load(open('/tmp/rollback-latest.json'))['version'])\"  "
            f"# must print {new}",
        ],
    }


def read_current_versions(root):
    package = json.loads((root / "package.json").read_text())["version"]
    cargo = re.search(
        r'^version\s*=\s*"([^"]+)"', (root / "src-tauri/Cargo.toml").read_text(), re.M
    ).group(1)
    tauri = json.loads((root / "src-tauri/tauri.conf.json").read_text())["version"]
    return {
        "package.json": package,
        "src-tauri/Cargo.toml": cargo,
        "src-tauri/tauri.conf.json": tauri,
    }


def apply_version(root, good, new):
    """Bump the three version files from `good` to `new`; refuse otherwise."""
    parse_version(good)
    parse_version(new)
    current = read_current_versions(root)
    if len(set(current.values())) != 1:
        raise ValueError(f"Version files disagree, refusing: {current}")
    if next(iter(current.values())) != good:
        raise ValueError(
            f"Tree is at {next(iter(current.values()))}, not the good version"
            f" {good}: refusing"
        )
    (root / "package.json").write_text(
        re.sub(
            r'"version":\s*"[^"]+"',
            f'"version": "{new}"',
            (root / "package.json").read_text(),
            count=1,
        )
    )
    cargo = root / "src-tauri/Cargo.toml"
    cargo.write_text(
        re.sub(
            r'^version\s*=\s*"[^"]+"',
            f'version = "{new}"',
            cargo.read_text(),
            count=1,
            flags=re.M,
        )
    )
    conf = root / "src-tauri/tauri.conf.json"
    data = json.loads(conf.read_text())
    data["version"] = new
    conf.write_text(json.dumps(data, indent=2) + "\n")
    return sorted(VERSION_FILES)


def verify_rollback_tree(root, good):
    """Refuse unless HEAD differs from the good tag only in version files."""
    tag = f"launcher-v{good}"
    proc = subprocess.run(
        ["git", "-C", str(root), "diff", "--name-only", tag, "HEAD", "--"],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise ValueError(f"Cannot diff against {tag}: {proc.stderr.strip()}")
    changed = sorted(line for line in proc.stdout.splitlines() if line.strip())
    extra = [path for path in changed if path not in VERSION_FILES]
    if extra:
        raise ValueError(
            f"Rollback tree still carries non-version changes vs {tag}, refusing:"
            f" {extra}"
        )
    return changed


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--good", required=True, help="Last-good released version")
    parser.add_argument(
        "--bad", required=True, help="Bad released version to roll back"
    )
    parser.add_argument("--new", required=True, help="Rollback version to publish")
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Edit the version files in the current checkout",
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="Refuse unless HEAD differs from the good tag only in version files",
    )
    args = parser.parse_args(argv)
    plan = plan_rollback(args.good, args.bad, args.new)
    if args.apply:
        changed = apply_version(Path.cwd(), args.good, args.new)
        print("Bumped: " + ", ".join(changed))
    if args.verify:
        changed = verify_rollback_tree(Path.cwd(), args.good)
        print(f"Tree matches launcher-v{args.good} except: " + ", ".join(changed))
    for step in plan["steps"]:
        print(step)
    return plan


if __name__ == "__main__":
    main(sys.argv[1:])
