import importlib.util
import json
import shutil
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

    def test_plan_cuts_branch_from_main_not_good_tag(self):
        plan = rollback.plan_rollback("1.0.22", "1.0.23", "1.0.24")
        self.assertIn(
            "git switch -c rollback/launcher-v1.0.24 origin/main", plan["steps"]
        )
        for step in plan["steps"]:
            self.assertNotIn("checkout -b rollback/launcher-v1.0.24 launcher-v", step)

    def test_plan_restores_good_tree_before_apply_and_verifies_before_tag(self):
        plan = rollback.plan_rollback("1.0.22", "1.0.23", "1.0.24")
        steps = plan["steps"]
        restore_at = next(
            i for i, step in enumerate(steps) if step.startswith("git restore ")
        )
        self.assertIn("--source=launcher-v1.0.22", steps[restore_at])
        apply_at = next(i for i, step in enumerate(steps) if "--apply" in step)
        verify_at = next(i for i, step in enumerate(steps) if "--verify" in step)
        tag_at = next(i for i, step in enumerate(steps) if step.startswith("git tag "))
        self.assertLess(restore_at, apply_at)
        self.assertLess(verify_at, tag_at)


@unittest.skipUnless(shutil.which("git"), "rollback recipe test needs git")
class RollbackRecipeRepoTests(unittest.TestCase):
    """End-to-end recipe checks on a scratch git repo (good tag + bad commit)."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self._git("init", "-q", "-b", "main", ".")
        self._git("config", "user.email", "rollback-test@example.com")
        self._git("config", "user.name", "rollback-test")
        (self.root / "src-tauri").mkdir()
        self._write_versions("1.0.22")
        (self.root / "keep.txt").write_text("good\n")
        self._git("add", "-A")
        self._git("commit", "-qm", "good release")
        self._git("tag", "launcher-v1.0.22")
        self._write_versions("1.0.23")
        (self.root / "keep.txt").write_text("good\nbad\n")
        (self.root / "bad-feature.txt").write_text("bad\n")
        self._git("add", "-A")
        self._git("commit", "-qm", "bad release")
        self._git("tag", "launcher-v1.0.23")

    def _git(self, *args):
        proc = subprocess.run(
            ["git", "-C", str(self.root), *args],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return proc.stdout

    def _write_versions(self, version):
        (self.root / "package.json").write_text(json.dumps({"version": version}))
        (self.root / "src-tauri/Cargo.toml").write_text(
            f'[package]\nversion = "{version}"\n'
        )
        (self.root / "src-tauri/tauri.conf.json").write_text(
            json.dumps({"version": version})
        )

    def test_old_recipe_keeps_bad_code_on_main(self):
        # Old recipe: branch from the good tag, bump, merge into main.
        self._git("checkout", "-qb", "rollback/launcher-v1.0.24", "launcher-v1.0.22")
        rollback.apply_version(self.root, "1.0.22", "1.0.24")
        self._git("commit", "-qam", "rollback to 1.0.22 code as v1.0.24")
        self._git("checkout", "-q", "main")
        # Version lines collide (both sides bumped them); resolving in favour
        # of the branch keeps main's non-version changes, as in the incident.
        self._git("merge", "-X", "theirs", "--no-ff", "-q", "rollback/launcher-v1.0.24")
        # The merge keeps main's bad commit: the bad file ships in the rollback.
        self.assertTrue((self.root / "bad-feature.txt").is_file())
        with self.assertRaisesRegex(ValueError, "bad-feature.txt"):
            rollback.verify_rollback_tree(self.root, "1.0.22")

    def test_new_recipe_tree_matches_good_tag_except_versions(self):
        # Current recipe: branch from main, restore the tree to the good tag.
        self._git("checkout", "-qb", "rollback/launcher-v1.0.24", "main")
        proc = subprocess.run(
            [
                "git",
                "restore",
                "--source=launcher-v1.0.22",
                "--staged",
                "--worktree",
                "--",
                ".",
            ],
            cwd=str(self.root),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        rollback.apply_version(self.root, "1.0.22", "1.0.24")
        self._git("commit", "-qam", "rollback to 1.0.22 code as v1.0.24")
        self.assertFalse((self.root / "bad-feature.txt").exists())
        self.assertEqual(
            rollback.verify_rollback_tree(self.root, "1.0.22"),
            sorted(rollback.VERSION_FILES),
        )

    def test_verify_refuses_tree_carrying_bad_code(self):
        with self.assertRaisesRegex(ValueError, "bad-feature.txt"):
            rollback.verify_rollback_tree(self.root, "1.0.22")


if __name__ == "__main__":
    unittest.main()
