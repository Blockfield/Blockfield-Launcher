import contextlib
import io
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import release_notes_tool as rn
import release_text as rt


def pin(tag, sha, repo="Blockfield/x"):
    return {
        "name": "X",
        "filename": f"x-{tag}.jar",
        "repo": repo,
        "tag": tag,
        "hash": sha,
        "url": f"https://github.com/{repo}/releases/download/{tag}/x.jar",
    }


class PinDiff(unittest.TestCase):
    def changes(self, old, new):
        return [row[1] for row in rn.pin_diff(old, new)]

    def test_kinds(self):
        self.assertEqual(self.changes({}, {"x": pin("bf1", "a")}), ["added"])
        self.assertEqual(self.changes({"x": pin("bf1", "a")}, {}), ["removed"])
        self.assertEqual(
            self.changes({"x": pin("bf1", "a")}, {"x": pin("bf3", "b")}), ["update"]
        )
        self.assertEqual(
            self.changes({"x": pin("v1.10.0", "a")}, {"x": pin("v1.9.0", "b")}),
            ["rollback"],
        )
        self.assertEqual(
            self.changes({"x": pin("bf1", "a")}, {"x": pin("bf1", "b")}), ["re-pinned"]
        )
        self.assertEqual(
            self.changes({"x": pin("bf1", "a")}, {"x": pin("bf1", "a")}), []
        )
        self.assertEqual(
            self.changes({"x": pin("import-2.53.0", "a")}, {"x": pin("bf9", "b")}),
            ["update"],
        )
        self.assertEqual(
            self.changes({"x": pin("bf10", "a")}, {"x": pin("bf9", "b")}), ["rollback"]
        )

    def test_same_bytes_new_tag_is_url_change(self):
        moved = pin("import-2.53.0", "a", repo="Blockfield/y")
        self.assertEqual(
            self.changes({"x": pin("bf9", "a")}, {"x": moved}), ["url-change"]
        )

    def test_r2_move_preserves_component_release_provenance(self):
        old = pin("music-v3", "same", "Blockfield/blockfield-music")
        moved = rn.pin_from(
            "s3://blockfield-assets/music/music-v3/x.jar",
            "X",
            old["filename"],
            old["hash"],
            old["url"],
        )
        self.assertEqual((moved["repo"], moved["tag"]), (old["repo"], old["tag"]))
        self.assertEqual(self.changes({"x": old}, {"x": moved}), ["url-change"])


class Check(unittest.TestCase):
    def write(self, text):
        path = Path(tempfile.mkdtemp(), "release-notes.md")
        path.write_text(text, encoding="utf-8", newline="\n")
        return path

    def test_rejects_skeleton_wrong_version_and_missing_file(self):
        text = rn.skeleton(
            "Blockfield/blockfield-releases", "v9.9.9", "v9.9.8", "client", "2026-09-27"
        )
        path = self.write(text)
        self.assertTrue(any("скелет" in e for e in rn.check(path)))
        filled = text.replace("TODO", "Заполнено")
        self.assertEqual(
            rn.check(self.write(filled), version="v9.9.9", kind="client"), []
        )
        self.assertTrue(rn.check(self.write(filled), version="v9.9.10"))
        self.assertTrue(rn.check(Path(tempfile.mkdtemp(), "absent.md")))


def release(tag, draft=False, assets=(), published="2026-10-01T00:00:00Z"):
    return {
        "tag_name": tag,
        "draft": draft,
        "published_at": published,
        "assets": [{"id": i, "name": n} for i, n in enumerate(assets)],
    }


def commit(message):
    return {"commit": {"message": message}}


def facts(**over):
    base = {
        "repository": "Blockfield/blockfield-releases",
        "version": "v2.61.0",
        "previous_version": "v2.60.0",
        "kind": "client",
        "date": "2026-10-02",
        "no_client_changes": None,
        "pins_checked": True,
        "pins": [],
        "pull_requests": [{"number": 5, "title": "Новый HUD", "body": "Тело"}],
        "gaps": [],
    }
    return base | over


SECTIONS = {
    "brief": "Новый нижний HUD.",
    "players": "- HUD перерисован.",
    "technical": "- Пин мода обновлён.",
    "components": "- X: обновление bf1 → bf2.",
    "compatibility": "Особых действий не требуется.",
    "known_issues": "Неизвестны.",
}
ANNOUNCEMENT = {
    "title": "🚀 Blockfield v2.61.0",
    "summary": "Новый HUD.",
    "player_changes": "• **HUD:** перерисован",
    "bug_fixes": "",
    "color_hex": "5865f2",
}


def reply(sections=SECTIONS, announcement=ANNOUNCEMENT):
    return json.dumps({"release_notes": sections, "announcement": announcement})


class Versions(unittest.TestCase):
    def test_previous_is_latest_published_lower_of_same_line(self):
        releases = [
            release("v2.59.6"),
            release("v2.60.0"),
            release("v2.61.0"),
            release("v2.60.5", draft=True),
            release("launcher-v1.0.0"),
            release("build-123"),
        ]
        published = [r for r in releases if not r["draft"]]
        self.assertEqual(rt.previous_version(published, "v2.61.0"), "v2.60.0")
        self.assertEqual(rt.previous_version(published, "v2.60.0"), "v2.59.6")
        self.assertIsNone(rt.previous_version(published, "v2.59.6"))
        self.assertIsNone(rt.previous_version([], "v1.0.0"))

    def test_pr_numbers_from_merge_and_squash_commits(self):
        commits = [
            commit("Merge pull request #38 from Blockfield/x\n\nbody"),
            commit("Fix thing (#41)"),
            commit("plain commit"),
            commit("Merge pull request #38 from Blockfield/x"),
        ]
        self.assertEqual(rt.merged_pr_numbers(commits), [38, 41])


class Denylist(unittest.TestCase):
    def test_denied(self):
        for text in (
            "см. https://example.org/x",
            "github.com/Blockfield",
            "Dokploy обновлён",
            "webhook",
            "ghcr.io/blockfield",
            "сервер 192.168.1.5",
            "my-home.homeserver",
        ):
            with self.subTest(text=text):
                self.assertTrue(rt.denied(text))

    def test_allowed(self):
        for text in (
            "https://blockfield.pro/launcher",
            "play.blockfield.pro",
            "обновление v1.2.3.4 и 2.61.0",
            "release-notes.md, mods/x.pw.toml",
        ):
            with self.subTest(text=text):
                self.assertEqual(rt.denied(text), [])


class FactsOnly(unittest.TestCase):
    def passes(self, f):
        text = rt.facts_only_notes(f)
        self.assertEqual(
            rn.check(self.path(text), f["repository"], f["version"], f["kind"]), []
        )
        return text

    def path(self, text):
        path = Path(tempfile.mkdtemp(), "release-notes.md")
        path.write_text(text, encoding="utf-8")
        return path

    def test_variants_pass_check(self):
        self.passes(facts())
        self.passes(facts(previous_version=None, pull_requests=[], pins_checked=False))
        self.passes(facts(gaps=["PR #3 не получен"]))
        text = self.passes(facts(no_client_changes=True))
        self.assertIn("подтверждено отпечатком", text)
        self.passes(
            facts(
                pins=[
                    {
                        "name": "X",
                        "change": "update",
                        "text": "X: обновление bf1 → bf2.",
                        "component_notes": ["- bf2 — Кратко: правка"],
                    }
                ]
            )
        )

    def test_unpublishable_titles_are_dropped(self):
        f = facts(
            pull_requests=[
                {"number": 1, "title": "Fix webhook on 10.0.0.1", "body": ""},
                {"number": 2, "title": "Исправлен прицел", "body": ""},
            ]
        )
        text = self.passes(f)
        self.assertIn("Исправлен прицел", text)
        self.assertNotIn("webhook", text)
        self.assertIn("Неполнота: часть записей опущена", text)

    def test_titles_that_break_check_fall_back_to_minimal(self):
        text = self.passes(
            facts(pull_requests=[{"number": 1, "title": "TODO: HUD", "body": ""}])
        )
        self.assertNotIn("TODO", text)


class Validate(unittest.TestCase):
    def run_validate(self, raw, f=None):
        return rt.validate_release(rt.parse_json_object(raw), f or facts())

    def test_good_reply_and_fenced_json(self):
        text, announcement, errors = self.run_validate(reply())
        self.assertEqual(errors, [])
        self.assertEqual(announcement["color_hex"], "5865F2")
        self.assertEqual(
            set(announcement),
            {"title", "summary", "player_changes", "bug_fixes", "color_hex"},
        )
        self.assertIn("backfilled: false", text)
        self.assertEqual(self.run_validate("```json\n" + reply() + "\n```")[2], [])

    def test_rejections(self):
        bad_section = dict(SECTIONS, brief="")
        no_check = dict(SECTIONS, players="TODO")
        leaks = dict(ANNOUNCEMENT, summary="Скачайте с https://example.org")
        long_title = dict(ANNOUNCEMENT, title="x" * 300)
        bad_color = dict(ANNOUNCEMENT, color_hex="nope")
        cases = {
            "empty section": reply(bad_section),
            "check": reply(no_check),
            "host": reply(announcement=leaks),
            "title length": reply(announcement=long_title),
            "color": reply(announcement=bad_color),
            "missing announcement": json.dumps({"release_notes": SECTIONS}),
        }
        for name, raw in cases.items():
            with self.subTest(name):
                self.assertTrue(self.run_validate(raw)[2])

    def test_every_changed_pin_must_appear(self):
        f = facts(
            pins=[{"name": "Gun", "change": "added", "text": "", "component_notes": []}]
        )
        self.assertTrue(self.run_validate(reply(), f)[2])
        ok = dict(SECTIONS, components="- Gun: добавлен")
        self.assertEqual(self.run_validate(reply(ok), f)[2], [])


class Release(unittest.TestCase):
    def setUp(self):
        self.out = Path(tempfile.mkdtemp(), "out")
        self.args = mock.Mock(
            repository="Blockfield/blockfield-releases",
            source_repository="Blockfield/blockfield-client",
            version="v2.61.0",
            kind="client",
            out_dir=str(self.out),
            repo_root=tempfile.mkdtemp(),
            build_json=None,
            date="2026-10-02",
            model="m",
            fresh=False,
        )
        patches = [
            mock.patch.object(
                rt,
                "published_releases",
                return_value=[release("v2.60.0"), release("v2.59.6")],
            ),
            mock.patch.object(
                rt,
                "merged_prs",
                return_value=[{"number": 5, "title": "Новый HUD", "body": "Тело"}],
            ),
            mock.patch.dict("os.environ", {"OPENROUTER_API_KEY": "k"}),
            mock.patch.object(rt.time, "sleep"),
        ]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def files(self):
        return sorted(p.name for p in self.out.iterdir())

    def run_release(self, *replies):
        with mock.patch.object(rt, "chat", side_effect=list(replies)) as chat:
            rt.run_release(self.args)
        return chat

    def test_model_reply_writes_both_files(self):
        chat = self.run_release(reply())
        self.assertEqual(self.files(), ["announcement.json", "release-notes.md"])
        self.assertEqual(chat.call_count, 1)
        self.assertEqual(
            rn.check(
                self.out / "release-notes.md",
                self.args.repository,
                "v2.61.0",
                "client",
            ),
            [],
        )
        self.assertEqual(
            json.loads((self.out / "announcement.json").read_text("utf-8"))["title"],
            "🚀 Blockfield v2.61.0",
        )
        sent = chat.call_args.args[1]
        self.assertIn("Новый HUD", sent)
        self.assertIn('"previous_version": "v2.60.0"', sent)

    def test_retry_gets_errors_and_can_succeed(self):
        chat = self.run_release("не JSON", reply())
        self.assertEqual(chat.call_count, 2)
        self.assertIn("отклонён", chat.call_args_list[1].args[1])
        self.assertEqual(self.files(), ["announcement.json", "release-notes.md"])

    def test_two_bad_replies_give_facts_only(self):
        leaks = dict(ANNOUNCEMENT, summary="https://example.org")
        chat = self.run_release(reply(announcement=leaks), reply(announcement=leaks))
        self.assertEqual(chat.call_count, 2)
        self.assertEqual(self.files(), ["release-notes.md"])
        self.assertEqual(
            rn.check(self.out / "release-notes.md", version="v2.61.0", kind="client"),
            [],
        )

    def test_llm_exception_gives_facts_only(self):
        self.run_release(OSError("network"), OSError("network"))
        self.assertEqual(self.files(), ["release-notes.md"])

    def test_no_key_or_model_skips_the_call(self):
        with mock.patch.dict("os.environ", {}, clear=True):
            with mock.patch.object(rt, "chat") as chat:
                rt.run_release(self.args)
        chat.assert_not_called()
        self.assertEqual(self.files(), ["release-notes.md"])

    def test_published_files_are_reused(self):
        published = release("v2.61.0", assets=("release-notes.md", "announcement.json"))
        good = rt.assemble(facts(), SECTIONS, by_model=True).encode()
        with (
            mock.patch.object(rt, "published_releases", return_value=[published]),
            mock.patch.object(rt, "asset_bytes", side_effect=[good, b'{"title": "t"}']),
            mock.patch.object(rt, "chat") as chat,
        ):
            rt.run_release(self.args)
        chat.assert_not_called()
        self.assertEqual(self.files(), ["announcement.json", "release-notes.md"])

    def pin_args(self):
        Path(self.args.repo_root, "mods").mkdir()
        old, new = (
            pin("v1.41.0", "a", "Blockfield/m"),
            pin("v1.42.0", "b", "Blockfield/m"),
        )
        row = ("x", "update", old, new, "X: обновление v1.41.0 → v1.42.0.")
        return [
            mock.patch.object(rn, "read_pins", return_value={}),
            mock.patch.object(rn, "pin_diff", return_value=[row]),
            mock.patch.object(
                rn,
                "component_notes",
                return_value=([], ["m v1.42.0: нет release-notes.md"]),
            ),
            mock.patch.object(
                rt,
                "merged_prs",
                side_effect=lambda repo, *a: (
                    [{"number": 7, "title": "Значки HUD", "body": "Новые иконки"}]
                    if repo == "Blockfield/m"
                    else [{"number": 5, "title": "Релиз", "body": ""}]
                ),
            ),
        ]

    def test_filler_reply_is_retried_with_the_reason_then_accepted(self):
        for p in self.pin_args():
            p.start()
            self.addCleanup(p.stop)
        filler = dict(
            SECTIONS,
            players="- Обновлён X с v1.41.0 до v1.42.0.",
            components="- X: v1.41.0 → v1.42.0.",
        )
        good = dict(
            SECTIONS,
            players="- Значки HUD перерисованы.",
            components="- X: v1.41.0 → v1.42.0, новые значки HUD.",
        )
        chat = self.run_release(reply(filler), reply(good))
        self.assertEqual(chat.call_count, 2)
        self.assertIn("Обновлён X с v1.41.0", chat.call_args_list[1].args[1])
        self.assertIn("Значки HUD", chat.call_args_list[0].args[1])
        self.assertEqual(self.files(), ["announcement.json", "release-notes.md"])
        notes = (self.out / "release-notes.md").read_text(encoding="utf-8")
        self.assertIn("Значки HUD перерисованы", notes)
        self.assertNotIn("нет release-notes.md", notes)

    def test_filler_twice_gives_facts_only_with_pin_pr_titles(self):
        for p in self.pin_args():
            p.start()
            self.addCleanup(p.stop)
        filler = dict(
            SECTIONS,
            brief="Рекомендуется обновиться.",
            components="- X: v1.41.0 → v1.42.0.",
        )
        chat = self.run_release(reply(filler), reply(filler))
        self.assertEqual(chat.call_count, 2)
        self.assertEqual(self.files(), ["release-notes.md"])
        notes = (self.out / "release-notes.md").read_text(encoding="utf-8")
        self.assertIn("- Значки HUD", notes)

    def test_main_never_raises_and_still_writes_notes(self):
        argv = [
            "release",
            "--repository",
            self.args.repository,
            "--version",
            "v2.61.0",
            "--kind",
            "client",
            "--out-dir",
            str(self.out),
            "--model",
            "m",
        ]
        with (
            mock.patch.object(rt, "published_releases", side_effect=OSError("gh")),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            rt.main(argv)
        self.assertEqual(self.files(), ["release-notes.md"])
        self.assertEqual(
            rn.check(self.out / "release-notes.md", version="v2.61.0", kind="client"),
            [],
        )


class Launcher(unittest.TestCase):
    def test_cli_uses_last_published_launcher_and_generates_valid_notes_without_ai(
        self,
    ):
        repository, version = "Blockfield/Blockfield-Launcher", "launcher-v1.0.13"
        with (
            tempfile.TemporaryDirectory() as tmp,
            mock.patch.object(
                rt,
                "gh_pages",
                return_value=[
                    [
                        release("launcher-v1.0.11"),
                        release("launcher-v1.0.12", draft=True),
                    ]
                ],
            ),
            mock.patch.object(
                rt,
                "merged_prs",
                return_value=[
                    {"number": 42, "title": "Game and Workshop profiles", "body": ""}
                ],
            ),
            mock.patch.dict("os.environ", {"OPENROUTER_API_KEY": ""}),
            mock.patch.object(rt, "chat") as chat,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            rt.main(
                [
                    "release",
                    "--repository",
                    repository,
                    "--version",
                    version,
                    "--kind",
                    "launcher",
                    "--out-dir",
                    tmp,
                    "--repo-root",
                    tmp,
                    "--model",
                    "",
                ]
            )
            notes = Path(tmp, "release-notes.md")
            self.assertEqual(rn.check(notes, repository, version, "launcher"), [])
            self.assertIn("previous_version: launcher-v1.0.11", notes.read_text())
            self.assertIn("Game and Workshop profiles", notes.read_text())
            self.assertFalse(Path(tmp, "announcement.json").exists())
            chat.assert_not_called()


REAL_PLAYERS = "- Значки HUD перерисованы.\n- Дрон видит дальше."


class Substance(unittest.TestCase):
    """Игровые разделы обязаны говорить, ЧТО изменилось, а не что обновилось."""

    def run_validate(self, f=None, **sections):
        obj = json.loads(reply(dict(SECTIONS, **sections)))
        return rt.validate_release(obj, f or facts())[2]

    def test_filler_is_rejected_with_a_reason(self):
        bump = "- Обновлён blockfield-client с v1.41.0 до v1.42.0 в составе сборки."
        arrow = "- Мод X: v1.41.0 → v1.42.0."
        cases = {
            "bump": {"players": bump},
            "arrow": {"players": arrow},
            "not restored": {
                "players": "- Подробности игровых изменений не восстановлены."
            },
            "not given": {
                "brief": "Подробное содержание изменений в фактах не приведено."
            },
            "advice": {"brief": "Выпуск с правками. Рекомендуется обновиться."},
            "only filler": {"players": bump + "\n- Подробности не восстановлены."},
        }
        for name, sections in cases.items():
            with self.subTest(name):
                errors = self.run_validate(**sections)
                self.assertTrue(errors, name)
                self.assertTrue(any("«" in e for e in errors))

    def test_announcement_filler_is_rejected(self):
        bad = dict(
            ANNOUNCEMENT, player_changes="• **Мод:** обновлён с v1.0.0 до v1.1.0"
        )
        errors = rt.validate_release(
            rt.parse_json_object(reply(announcement=bad)), facts()
        )[2]
        self.assertTrue(any("announcement.player_changes" in e for e in errors))

    def test_versions_are_fine_in_technical_sections(self):
        self.assertEqual(
            self.run_validate(
                technical="- Пин мода обновлён с v1.41.0 до v1.42.0.",
                components="- X: обновление v1.41.0 → v1.42.0.",
            ),
            [],
        )

    def test_ranges_in_prose_are_not_bumps(self):
        self.assertEqual(
            self.run_validate(players="- Дальность выстрела от 1.5 до 3 блоков."), []
        )

    def test_empty_evidence_must_say_so_plainly(self):
        empty = facts(pull_requests=[])
        self.assertTrue(self.run_validate(empty))
        self.assertEqual(self.run_validate(empty, players=rt.NO_CHANGE_SENTENCE), [])
        confirmed = facts(pull_requests=[], no_client_changes=True)
        self.assertEqual(
            self.run_validate(
                confirmed,
                players="Изменений нет — подтверждено отпечатком клиентских файлов.",
            ),
            [],
        )

    def test_evidence_counts_pin_prs_and_added_pins(self):
        base = facts(pull_requests=[])
        pin_pr = {"number": 3, "title": "HUD", "body": ""}
        updated = {
            "name": "X",
            "change": "update",
            "text": "",
            "component_notes": [],
            "pull_requests": [pin_pr],
        }
        added = dict(updated, change="added", pull_requests=[])
        bare = dict(updated, pull_requests=[])
        self.assertTrue(rt.has_evidence(dict(base, pins=[updated])))
        self.assertTrue(rt.has_evidence(dict(base, pins=[added])))
        self.assertFalse(rt.has_evidence(dict(base, pins=[bare])))

    def test_facts_only_without_evidence_says_so_and_with_pin_prs_lists_them(self):
        bare = rt.facts_only_notes(facts(pull_requests=[]))
        self.assertIn(rt.NO_CHANGE_SENTENCE, bare)
        pin_row = {
            "name": "X",
            "change": "update",
            "text": "X: обновление v1 → v2.",
            "component_notes": [],
            "pull_requests": [{"number": 3, "title": "Значки HUD", "body": ""}],
        }
        text = rt.facts_only_notes(facts(pull_requests=[], pins=[pin_row]))
        self.assertIn("- Значки HUD", text)
        self.assertNotIn(rt.NO_CHANGE_SENTENCE, text)


class Server(unittest.TestCase):
    RELEASES = [
        release("v2.64.0", published="2026-10-02T20:24:33Z"),
        release("world-seed-v13", published="2026-10-02T20:30:00Z"),
        release("v2.63.0", published="2026-10-02T19:42:36Z"),
        release("v2.62.0", published="2026-10-02T18:05:15Z"),
    ]

    def test_range_starts_at_the_server_live_when_the_last_client_shipped(self):
        self.assertEqual(
            rt.server_range(self.RELEASES, "2026-10-02T20:17:39Z"),
            ("v2.63.0", "v2.64.0"),
        )

    def test_no_server_before_the_last_client_has_no_start(self):
        self.assertEqual(
            rt.server_range(self.RELEASES, "2026-09-01T00:00:00Z"), (None, "v2.64.0")
        )

    def test_client_release_carries_server_changes_as_a_pin(self):
        prs = [{"number": 63, "title": "Инженер: две ракеты", "body": ""}]
        gaps = []
        with (
            mock.patch.object(rt, "published_releases", return_value=self.RELEASES),
            mock.patch.object(rn, "component_notes") as notes,
            mock.patch.object(rt, "merged_prs", return_value=prs) as fetch,
        ):
            got = rt.server_pin({"published_at": "2026-10-02T20:17:39Z"}, gaps)
        self.assertEqual(got["name"], "blockfield-server")
        self.assertEqual(got["pull_requests"], prs)
        self.assertEqual(
            fetch.call_args.args[:3], (rt.SERVER_REPOSITORY, "v2.63.0", "v2.64.0")
        )
        self.assertEqual((got["component_notes"], gaps), ([], []))
        notes.assert_not_called()  # server notes retell the mod the client pin covers

    def test_unchanged_server_adds_nothing(self):
        gaps = []
        with mock.patch.object(rt, "published_releases", return_value=self.RELEASES):
            got = rt.server_pin({"published_at": "2026-10-03T00:00:00Z"}, gaps)
        self.assertEqual((got, gaps), (None, []))


class PinEvidence(unittest.TestCase):
    OLD = pin("v1.41.0", "a", "Blockfield/m")
    NEW = pin("v1.42.0", "b", "Blockfield/m")
    PRS = [{"number": 7, "title": "Значки HUD", "body": "Новые иконки"}]

    def evidence(self, notes, missing=(), prs=PRS, change="update", error=None):
        gaps = []
        with (
            mock.patch.object(
                rn, "component_notes", return_value=(notes, list(missing))
            ),
            mock.patch.object(
                rt, "merged_prs", return_value=prs, side_effect=error
            ) as fetch,
        ):
            result = rt.pin_evidence(change, self.OLD, self.NEW, gaps)
        return result, gaps, fetch

    def test_real_notes_do_not_fetch_prs(self):
        notes = ["    - v1.42.0 — Для игроков: Новые значки"]
        (got, prs), gaps, fetch = self.evidence(notes)
        self.assertEqual((got, prs, gaps), (notes, [], []))
        fetch.assert_not_called()

    def test_missing_notes_are_replaced_by_prs_between_the_tags(self):
        (notes, prs), gaps, fetch = self.evidence(
            [], missing=["Blockfield/m v1.42.0: нет release-notes.md"]
        )
        self.assertEqual((notes, prs, gaps), ([], self.PRS, []))
        self.assertEqual(
            fetch.call_args.args[:3], ("Blockfield/m", "v1.41.0", "v1.42.0")
        )

    def test_not_restored_notes_are_dropped_in_favour_of_prs(self):
        weak = [
            "    - v1.41.0 — Для игроков: Подробности игровых изменений не восстановлены."
        ]
        (notes, prs), _, _ = self.evidence(weak)
        self.assertEqual((notes, prs), ([], self.PRS))

    def test_rollback_reads_the_removed_range(self):
        _, _, fetch = self.evidence([], change="rollback")
        self.assertEqual(fetch.call_args.args[1:3], ("v1.42.0", "v1.41.0"))

    def test_no_evidence_is_one_loud_gap(self):
        (notes, prs), gaps, _ = self.evidence([], prs=[])
        self.assertEqual((notes, prs), ([], []))
        self.assertEqual(len(gaps), 1)
        self.assertIn("источников нет", gaps[0])

    def test_failed_pr_fetch_reports_the_message(self):
        error = subprocess.CalledProcessError(1, ["gh"], stderr=b"HTTP 403 forbidden")
        _, gaps, _ = self.evidence([], error=error)
        self.assertTrue(any("HTTP 403 forbidden" in g for g in gaps))


class MergedPrs(unittest.TestCase):
    def test_bots_and_footers_are_skipped_and_failures_say_why(self):
        commits = [
            commit("Merge pull request #1 from a/b"),
            commit("Merge pull request #2 from a/c"),
            commit("Merge pull request #3 from a/d"),
        ]
        footer = (
            "<!-- codesmith:footer -->\n---\nBlacksmith\n<!-- /codesmith:footer -->"
        )
        bodies = {
            1: {
                "title": "HUD",
                "body": f"Новые значки\n\n{footer}",
                "additions": 9,
                "deletions": 3,
            },
            2: {"title": "Bump x", "body": "", "user": {"type": "Bot"}},
        }

        def api(*args, **_):
            number = int(args[1].rsplit("/", 1)[1])
            if number == 3:
                raise subprocess.CalledProcessError(1, ["gh"], stderr=b"HTTP 403")
            return json.dumps(bodies[number])

        gaps = []
        with (
            mock.patch.object(rt, "gh_pages", return_value=[{"commits": commits}]),
            mock.patch.object(rt, "gh", side_effect=api),
        ):
            prs = rt.merged_prs("o/r", "v1", "v2", gaps)
        self.assertEqual(
            prs,
            [
                {
                    "number": 1,
                    "title": "HUD",
                    "body": "Новые значки",
                    "changed_lines": 12,
                }
            ],
        )
        self.assertEqual(len(gaps), 1)
        self.assertIn("HTTP 403", gaps[0])


class Loud(unittest.TestCase):
    def test_gaps_become_annotations_and_a_summary(self):
        summary = Path(tempfile.mkdtemp(), "summary.md")
        out = io.StringIO()
        with (
            mock.patch.dict("os.environ", {"GITHUB_STEP_SUMMARY": str(summary)}),
            contextlib.redirect_stdout(out),
        ):
            rt.report(["PR #3 не получен (HTTP 403)\nвторая строка"], "тест", "v1.0.0")
        printed = out.getvalue()
        self.assertIn(
            "::warning title=release-text: неполнота::PR #3 не получен (HTTP 403)%0A",
            printed,
        )
        text = summary.read_text(encoding="utf-8")
        self.assertIn("HTTP 403", text)
        self.assertIn("Текст: тест", text)

    def test_describe_includes_the_subprocess_stderr(self):
        exc = subprocess.CalledProcessError(
            128, ["git"], stderr=b"fatal: bad object v1"
        )
        self.assertIn("fatal: bad object v1", rt.describe(exc))
        self.assertIn("CalledProcessError", rt.describe(exc))


class PullRequest(unittest.TestCase):
    PR = {"title": "Fix aim", "body": ""}
    FILES = [
        {
            "filename": "a.py",
            "status": "modified",
            "additions": 1,
            "deletions": 0,
            "patch": "@@ -1 +1 @@\n+x",
        },
        {
            "filename": "logo.png",
            "status": "added",
            "additions": 0,
            "deletions": 0,
        },
        {
            "filename": "big.json",
            "status": "modified",
            "additions": 9,
            "deletions": 9,
            "patch": "y" * (rt.PATCH_FILE_CHARS + 1),
        },
    ]
    COMMITS = [commit("Fix aim\n\nlonger text")]

    def setUp(self):
        self.args = mock.Mock(repository="Blockfield/x", pr_number=7, model="m")
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.calls = []
        for p in (
            mock.patch.dict("os.environ", {"OPENROUTER_API_KEY": "k"}),
            mock.patch.object(rt.time, "sleep"),
            mock.patch.object(rt, "gh_pages", side_effect=self.pages),
        ):
            p.start()
            self.addCleanup(p.stop)

    def pages(self, path):
        return [self.COMMITS if path.endswith("commits?per_page=100") else self.FILES]

    def gh(self, body):
        def call(*args, **_):
            self.calls.append(args)
            if args[0] == "api" and "-X" not in args:
                return json.dumps({"title": "Fix aim", "body": body})
            return ""

        return mock.patch.object(rt, "gh", side_effect=call)

    def test_prompt_lists_binary_and_large_files_by_name_only(self):
        prompt = rt.pr_prompt(self.PR, self.COMMITS, self.FILES)
        self.assertIn("+x", prompt)
        self.assertIn("logo.png (added, +0/-0): только имя", prompt)
        self.assertIn("big.json (modified, +9/-9): только имя", prompt)
        self.assertNotIn("y" * 100, prompt)

    def test_player_facing_files_get_the_budget_first(self):
        files = [
            {
                "filename": "src/test/HudTest.java",
                "status": "added",
                "additions": 400,
                "deletions": 0,
                "patch": "+test",
            },
            {
                "filename": "src/main/Grip.java",
                "status": "modified",
                "additions": 5,
                "deletions": 1,
                "patch": "+grip",
            },
            {
                "filename": "src/main/Hud.java",
                "status": "modified",
                "additions": 300,
                "deletions": 50,
                "patch": "+hud",
            },
        ]
        prompt = rt.pr_prompt(self.PR, self.COMMITS, files)
        order = [prompt.index(n) for n in ("Hud.java", "HudTest.java", "Grip.java")]
        self.assertEqual(order, sorted(order))

    def test_large_patch_keeps_its_added_lines(self):
        patch = "\n".join(["+new hud"] * 600 + [" context", "-old"] * 600)
        big = dict(self.FILES[0], filename="Hud.java", patch=patch)
        prompt = rt.pr_prompt(self.PR, self.COMMITS, [big])
        self.assertIn("только добавленные строки", prompt)
        self.assertIn("+new hud", prompt)
        self.assertNotIn("-old", prompt)

    def test_empty_body_is_filled(self):
        text = "Что изменилось: прицел.\n\nДля игроков: прицел точнее."
        with self.gh(""), mock.patch.object(rt, "chat", return_value=text):
            rt.run_pr(self.args)
        patch = [c for c in self.calls if "-X" in c]
        self.assertEqual(len(patch), 1)
        self.assertEqual(
            patch[0][:4], ("api", "-X", "PATCH", "repos/Blockfield/x/pulls/7")
        )

    def test_body_with_only_the_codesmith_footer_is_empty_and_keeps_it(self):
        footer = "<!-- codesmith:footer -->\n---\n<a href='x'>y</a>\n<!-- /codesmith:footer -->"
        text = "Что изменилось: прицел.\n\nДля игроков: прицел точнее."
        with self.gh(f"\n\n{footer}"), mock.patch.object(rt, "chat", return_value=text):
            with mock.patch.object(rt.tempfile, "TemporaryDirectory") as tmp:
                tmp.return_value.__enter__.return_value = self.tmp
                rt.run_pr(self.args)
        patch = [c for c in self.calls if "-X" in c]
        self.assertEqual(len(patch), 1)
        self.assertEqual(
            Path(self.tmp, "body.md").read_text(encoding="utf-8"),
            f"{text}\n\n{footer}",
        )

    def test_agent_attribution_alone_is_empty(self):
        body = "🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\n<!-- codesmith:footer -->\nf"
        self.assertEqual(rt.split_footer(body), ("", "<!-- codesmith:footer -->\nf"))
        self.assertEqual(
            rt.split_footer("Сам\n\n🤖 Generated with [Claude Code](https://x)")[0],
            "Сам",
        )

    def test_footer_split(self):
        self.assertEqual(rt.split_footer(None), ("", ""))
        self.assertEqual(
            rt.split_footer("Сам\n\n<!-- codesmith:footer -->\nf"),
            ("Сам", "<!-- codesmith:footer -->\nf"),
        )

    def test_existing_body_is_left_alone(self):
        with self.gh("Описал сам"), mock.patch.object(rt, "chat") as chat:
            rt.run_pr(self.args)
        chat.assert_not_called()
        self.assertFalse([c for c in self.calls if "-X" in c])

    def test_missing_players_line_retries_then_gives_up(self):
        with (
            self.gh(""),
            mock.patch.object(rt, "chat", return_value="Без итога") as chat,
        ):
            rt.run_pr(self.args)
        self.assertEqual(chat.call_count, 2)
        self.assertFalse([c for c in self.calls if "-X" in c])

    def test_validate_pr(self):
        self.assertEqual(rt.validate_pr("A\n\nДля игроков: нет")[1], [])
        self.assertEqual(
            rt.validate_pr("```markdown\nA\n\nДля игроков: нет\n```")[0],
            "A\n\nДля игроков: нет",
        )
        self.assertTrue(rt.validate_pr("A\n\nДля игроков: нет http://x.com")[1])

    def test_no_key_is_a_noop(self):
        with mock.patch.dict("os.environ", {}, clear=True), self.gh("") as gh:
            rt.run_pr(self.args)
        gh.assert_not_called()


if __name__ == "__main__":
    unittest.main()
