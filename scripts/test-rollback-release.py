import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "rollback", Path(__file__).with_name("rollback-release.py")
)
rollback = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rollback)


def fake_feed(version):
    # Mocked updater feed: what latest.json would carry after the rollback.
    return {"version": version, "platforms": {}, "notes": "rollback"}


class RollbackReleaseTests(unittest.TestCase):
    def test_bad_and_good_clients_move_forward_to_rollback(self):
        feed = fake_feed("1.0.24")
        self.assertTrue(rollback.update_available("1.0.23", feed["version"]))
        self.assertTrue(rollback.update_available("1.0.22", feed["version"]))

    def test_no_update_once_on_rollback_or_newer(self):
        self.assertFalse(rollback.update_available("1.0.24", "1.0.24"))
        self.assertFalse(rollback.update_available("1.0.25", "1.0.24"))

    def test_plan_names_branch_and_tag(self):
        plan = rollback.plan_rollback("1.0.22", "1.0.23", "1.0.24")
        self.assertEqual(plan["branch"], "rollback/launcher-v1.0.24")
        self.assertEqual(plan["tag"], "launcher-v1.0.24")
        self.assertTrue(any("launcher-v1.0.22" in step for step in plan["steps"]))
        self.assertTrue(any("launcher-v1.0.24" in step for step in plan["steps"]))

    def test_plan_lands_rollback_on_main_before_tagging(self):
        plan = rollback.plan_rollback("1.0.22", "1.0.23", "1.0.24")
        steps = plan["steps"]
        tag_at = next(i for i, step in enumerate(steps) if step.startswith("git tag "))
        self.assertIn("launcher-v1.0.24", steps[tag_at])
        earlier = " ".join(steps[:tag_at])
        self.assertIn("main", earlier)
        self.assertTrue(any("PR to main" in step for step in steps[:tag_at]))

    def test_plan_rejects_non_rollback_ordering(self):
        for good, bad, new in (
            ("1.0.23", "1.0.22", "1.0.24"),  # bad older than good
            ("1.0.22", "1.0.24", "1.0.23"),  # new below bad: updater would ignore it
            ("1.0.22", "1.0.23", "1.0.23"),  # new must be higher, not equal
            ("1.0.22", "1.0.22", "1.0.23"),  # nothing to roll back
        ):
            with (
                self.subTest(good=good, bad=bad, new=new),
                self.assertRaises(ValueError),
            ):
                rollback.plan_rollback(good, bad, new)

    def test_plan_rejects_non_versions(self):
        with self.assertRaises(ValueError):
            rollback.plan_rollback("1.0.22", "1.0.23", "next")

    def test_apply_bumps_all_three_version_files(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "src-tauri").mkdir()
        (root / "package.json").write_text(json.dumps({"version": "1.0.22"}) + "\n")
        (root / "src-tauri/Cargo.toml").write_text('[package]\nversion = "1.0.22"\n')
        (root / "src-tauri/tauri.conf.json").write_text(
            json.dumps({"version": "1.0.22"}) + "\n"
        )
        changed = rollback.apply_version(root, "1.0.22", "1.0.24")
        self.assertEqual(changed, sorted(rollback.VERSION_FILES))
        current = rollback.read_current_versions(root)
        self.assertEqual(set(current.values()), {"1.0.24"})

    def test_apply_refuses_tree_at_bad_version(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "src-tauri").mkdir()
        (root / "package.json").write_text(json.dumps({"version": "1.0.23"}) + "\n")
        (root / "src-tauri/Cargo.toml").write_text('[package]\nversion = "1.0.23"\n')
        (root / "src-tauri/tauri.conf.json").write_text(
            json.dumps({"version": "1.0.23"}) + "\n"
        )
        with self.assertRaisesRegex(ValueError, "not the good version"):
            rollback.apply_version(root, "1.0.22", "1.0.24")
        # Nothing was bumped: the bad tree is untouched.
        self.assertEqual(set(rollback.read_current_versions(root).values()), {"1.0.23"})

    def test_apply_refuses_disagreeing_version_files(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "src-tauri").mkdir()
        (root / "package.json").write_text(json.dumps({"version": "1.0.22"}) + "\n")
        (root / "src-tauri/Cargo.toml").write_text('[package]\nversion = "1.0.23"\n')
        (root / "src-tauri/tauri.conf.json").write_text(
            json.dumps({"version": "1.0.22"}) + "\n"
        )
        with self.assertRaisesRegex(ValueError, "disagree"):
            rollback.apply_version(root, "1.0.22", "1.0.24")

    def test_dry_run_prints_steps_without_touching_files(self):
        script = Path(__file__).with_name("rollback-release.py")
        proc = subprocess.run(
            [
                sys.executable,
                str(script),
                "--good",
                "1.0.22",
                "--bad",
                "1.0.23",
                "--new",
                "1.0.24",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("rollback/launcher-v1.0.24", proc.stdout)
        self.assertIn("launcher-v1.0.22", proc.stdout)


if __name__ == "__main__":
    unittest.main()
