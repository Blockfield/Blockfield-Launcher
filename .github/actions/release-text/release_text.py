"""Тексты релизов и PR в CI.

`release`: факты выпуска собираются без модели (предыдущий релиз, сверка пинов,
выдержки компонентов, слитые PR, отпечаток клиента), затем одна модель
(OpenRouter) пишет release-notes.md и announcement.json. Любой сбой даёт
release-notes.md только из фактов без announcement.json; выход всегда 0, чтобы
выпуск и деплой не зависели от генерации текста.

`pr`: если у PR пустое описание, оно пишется по diff и коммитам.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

import release_notes_tool as rn

HERE = Path(__file__).resolve().parent
API_URL = "https://openrouter.ai/api/v1/chat/completions"
ATTEMPTS = 2
FRONT_KIND = {
    "client": "client",
    "server": "server",
    "launcher": "launcher",
    "mod": "mod",
    "decor": "mod",
    "proxy": "server",
}
SECTIONS = (
    ("brief", "Кратко"),
    ("players", "Для игроков"),
    ("technical", "Технические изменения"),
    ("components", "Обновлённые компоненты"),
    ("compatibility", "Совместимость и необходимые действия"),
    ("known_issues", "Известные проблемы"),
)
ANNOUNCEMENT_LIMITS = {
    "title": 256,
    "summary": 4000,
    "player_changes": 1024,
    "bug_fixes": 1024,
}
# Бот анонсирует только клиентские релизы, поэтому клиентский выпуск несёт и сервер.
SERVER_REPOSITORY = "Blockfield/blockfield-server"
SERVER_NAME = "blockfield-server"
MAX_PRS = 80
PR_BODY_CHARS = 2500
PIN_MAX_PRS = 30
PIN_PR_BODY_CHARS = 1200
FACTS_BUDGET = 60000
PATCH_FILE_CHARS = 4000
PATCH_EXCERPT_CHARS = 8000
PATCH_TOTAL_CHARS = 80000
PR_BODY_LIMIT = 8000
NAME_ONLY = (".lock", "package-lock.json", ".min.js", ".min.css")
# Файлы, которые игрок не видит: их diff получает бюджет последним.
SUPPORT_RE = re.compile(
    r"(?i)(?:^|/)(?:tests?|\.github|docs?|dev|scripts)/|\.(?:md|txt)$|(?:^|/)(?:NOTICE|PROVENANCE|LICENSE)"
)


def importance(f):
    """Вес файла для описания PR: объём изменений, у служебных файлов в десять раз меньше."""
    lines = f["additions"] + f["deletions"]
    return lines / 10 if SUPPORT_RE.search(f["filename"]) else lines


DENY_WORDS = re.compile(r"dokploy|webhook|homeserver|ghcr", re.I)
HOST_RE = re.compile(
    r"(?i)(?:https?://)?\b((?:[a-z0-9-]+\.)+(?:com|net|org|io|ru|dev|app|pro|cloud|xyz|me|tv|gg|info|site|online|eu|de|su))\b"
    r"|https?://([^/\s)>\]\"']+)"
)
# ponytail: разобранная четвёрка чисел считается IP, в том числе версия вида 1.1.7.10;
# ложное срабатывание даёт текст только из фактов. Ослабить, если мешает на практике.
IP_RE = re.compile(r"(?<![\w.-])(?:\d{1,3}\.){3}\d{1,3}(?![\w-])")
PR_NUMBER_RE = re.compile(r"^Merge pull request #(\d+)\b|\(#(\d+)\)\s*$")


# Заглушки, которыми выпуск «рассказывает» об обновлении, не говоря, что изменилось.
NOT_RESTORED_RE = re.compile(
    r"(?i)(?:подробност|сведени|содержани|изменени|информаци|детал)\w*[^.\n]{0,80}?"
    r"\sне\s+(?:восстановлен|приведен|получен|установлен|найден)\w*"
    r"|отсутствие\s+сведений|в\s+фактах"
)
VERSION = r"(?:v\d+(?:\.\d+)+|\d+\.\d+\.\d+|bf\d+)"
BUMP_RE = re.compile(
    r"(?i)обновл[её]н\w*[^\n]{0,80}?\sс\s+v?\d[\d.]*\s+до\s+v?\d[\d.]*"
    rf"|\b{VERSION}\s*(?:→|->|до)\s*{VERSION}"
)
ADVICE_RE = re.compile(r"(?i)рекоменду\w+\s+обновит")
NO_CHANGE_SENTENCE = (
    "Видимых для игроков изменений по доступным источникам установить не удалось."
)
NO_CHANGE_RE = re.compile(r"(?i)установить\s+не\s+удалось|не\s+удалось\s+установить")


def annotate(message, title="release-text"):
    escaped = message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::warning title={title}::{escaped}", flush=True)


def warn(message):
    annotate(message)


def describe(exc):
    """Тип и текст исключения; у вызова подпроцесса ещё и его stderr (gh, git)."""
    parts = [type(exc).__name__, str(exc)]
    stderr = getattr(exc, "stderr", None)
    if stderr:
        parts.append(
            stderr.decode(errors="replace") if isinstance(stderr, bytes) else stderr
        )
    return " ".join(" ".join(parts).split())[:300]


def gh(*args, binary=False):
    return subprocess.check_output(
        ["gh", *args], text=not binary, stderr=subprocess.PIPE
    )


def gh_pages(path):
    return rn.gh_json(path)


def flatten(pages, key=None):
    out = []
    for page in pages:
        out.extend(page.get(key, []) if key else page)
    return out


def denied(text):
    """Что в тексте нельзя публиковать: чужие хосты, IP, внутренние названия."""
    found = [m.group(0) for m in DENY_WORDS.finditer(text)]
    for m in HOST_RE.finditer(text):
        host = (m.group(1) or m.group(2)).lower().split(":")[0]
        if host != "blockfield.pro" and not host.endswith(".blockfield.pro"):
            found.append(host)
    for m in IP_RE.finditer(text):
        if all(int(part) <= 255 for part in m.group(0).split(".")):
            found.append(m.group(0))
    return found


def published_releases(repository):
    return [
        r
        for r in flatten(gh_pages(f"repos/{repository}/releases?per_page=100"))
        if not r["draft"]
    ]


def previous_version(releases, version):
    """Последний опубликованный релиз той же линии ниже version; сожжённые номера релиза не имеют."""
    key = rn.version_key(version)
    found = [
        (rn.version_key(r["tag_name"]), r["tag_name"])
        for r in releases
        if r["tag_name"] != version
    ]
    found = [(k, tag) for k, tag in found if rn.comparable(k, key) and k < key]
    return max(found)[1] if found else None


def merged_pr_numbers(commits):
    numbers = []
    for commit in commits:
        first = commit["commit"]["message"].split("\n", 1)[0]
        m = PR_NUMBER_RE.search(first)
        number = int(m.group(1) or m.group(2)) if m else None
        if number and number not in numbers:
            numbers.append(number)
    return numbers


# Blacksmith appends this block to every PR body, so a body holding only it is still empty.
FOOTER = re.compile(
    r"<!-- codesmith:footer -->.*?(?:<!-- /codesmith:footer -->|\Z)", re.S
)
# An agent's attribution line alone is not a description either.
ATTRIBUTION = re.compile(
    r"(?m)^[ \t]*🤖 Generated with \[Claude Code\]\([^)\n]*\)[ \t]*$"
)


def split_footer(body):
    """The author's text and the codesmith footer block ("" when absent)."""
    body = body or ""
    found = FOOTER.search(body)
    author = ATTRIBUTION.sub("", FOOTER.sub("", body))
    return author.strip(), found.group(0) if found else ""


def merged_prs(
    source, previous, version, gaps, limit=MAX_PRS, body_chars=PR_BODY_CHARS
):
    commits = flatten(
        gh_pages(f"repos/{source}/compare/{previous}...{version}?per_page=100"),
        "commits",
    )
    numbers = merged_pr_numbers(commits)
    if len(numbers) > limit:
        gaps.append(f"слитых PR {len(numbers)}, разобраны первые {limit}")
        numbers = numbers[:limit]
    prs = []
    for number in numbers:
        try:
            pr = json.loads(gh("api", f"repos/{source}/pulls/{number}"))
        except (subprocess.CalledProcessError, json.JSONDecodeError) as exc:
            gaps.append(f"PR #{number} не получен ({describe(exc)})")
            continue
        if (pr.get("user") or {}).get("type") == "Bot":
            continue
        prs.append(
            {
                "number": number,
                "title": pr["title"].strip(),
                "body": split_footer(pr.get("body"))[0][:body_chars],
                "changed_lines": pr.get("additions", 0) + pr.get("deletions", 0),
            }
        )
    return prs


def asset_bytes(repository, asset):
    return gh(
        "api",
        "-H",
        "Accept: application/octet-stream",
        f"repos/{repository}/releases/assets/{asset['id']}",
        binary=True,
    )


def client_unchanged(build_json, releases, repository, previous):
    release = next(r for r in releases if r["tag_name"] == previous)
    asset = next(a for a in release["assets"] if a["name"] == "build.json")
    old = json.loads(asset_bytes(repository, asset))["client"]
    return json.loads(Path(build_json).read_text(encoding="utf-8"))["client"] == old


def weak_notes(lines):
    """Выдержек нет или все они — «не восстановлено»: по ним смысл обновления не узнать."""
    return not lines or all(NOT_RESTORED_RE.search(line) for line in lines)


def pin_evidence(change, old, new, gaps):
    """(выдержки release-notes, слитые PR) компонента за промежуток пина.

    Когда release-notes компонента пусты или «не восстановлены», смысл берётся из
    его слитых PR между тегами; при откате это PR снятых изменений.
    """
    notes, missing = [], []
    try:
        notes, missing = rn.component_notes(old, new)
    except Exception as exc:
        missing.append(f"{new['name']}: release-notes не получены ({describe(exc)})")
    prs, weak = [], weak_notes(notes)
    if weak and new["repo"] and new["repo"] == old["repo"]:
        lo, hi = (old, new) if change == "update" else (new, old)
        found = []
        try:
            prs = merged_prs(
                new["repo"],
                lo["tag"],
                hi["tag"],
                found,
                PIN_MAX_PRS,
                PIN_PR_BODY_CHARS,
            )
        except Exception as exc:
            found.append(f"слитые PR не получены ({describe(exc)})")
        gaps.extend(f"{new['name']}: {gap}" for gap in found)
    if prs:
        return [], prs
    if weak:
        notes = []
        missing.append(
            f"{new['name']}: что изменилось между {old['tag']} и {new['tag']}, источников нет"
        )
    gaps.extend(missing)
    return notes, []


def server_range(server_releases, since):
    """(серверный тег на момент since, последний серверный тег); since — published_at прошлого анонса."""
    line = [
        (rn.version_key(r["tag_name"]), r)
        for r in server_releases
        if rn.comparable(rn.version_key(r["tag_name"]), ("v", ()))
    ]
    if not line:
        return None, None
    before = [(k, r["tag_name"]) for k, r in line if r["published_at"] <= since]
    newest = max(line, key=lambda kr: kr[0])[1]["tag_name"]
    return (max(before)[1] if before else None), newest


def server_pin(previous_release, gaps):
    """Изменения сервера, вышедшие после прошлого клиентского релиза, как пин `blockfield-server`."""
    try:
        old, new = server_range(
            published_releases(SERVER_REPOSITORY), previous_release["published_at"]
        )
    except Exception as exc:
        gaps.append(f"изменения сервера не собраны ({describe(exc)})")
        return None
    if not old or not new:
        gaps.append("изменения сервера не собраны: нет серверного релиза до прошлого")
        return None
    if old == new:
        return None
    # Только PR самого сервера: его release-notes пересказывают мод, который клиент уже описал своим пином.
    try:
        prs = merged_prs(
            SERVER_REPOSITORY, old, new, gaps, PIN_MAX_PRS, PIN_PR_BODY_CHARS
        )
    except Exception as exc:
        gaps.append(f"{SERVER_NAME}: слитые PR не получены ({describe(exc)})")
        prs = []
    return {
        "name": SERVER_NAME,
        "change": "update",
        "text": f"{SERVER_NAME}: {old} → {new}",
        "component_notes": [],
        "pull_requests": prs,
    }


def collect_facts(a, releases):
    gaps = []
    previous = previous_version(releases, a.version)
    facts = {
        "repository": a.repository,
        "version": a.version,
        "previous_version": previous,
        "kind": FRONT_KIND[a.kind],
        "date": a.date,
        "no_client_changes": None,
        "pins_checked": False,
        "pins": [],
        "pull_requests": [],
        "gaps": gaps,
    }
    if previous is None:
        gaps.append("предыдущего релиза линии нет, промежуток не сверен")
        return facts
    if Path(a.repo_root, "mods").is_dir():
        try:
            rows = rn.pin_diff(
                rn.read_pins(a.repo_root, previous), rn.read_pins(a.repo_root, "HEAD")
            )
            facts["pins_checked"] = True
        except Exception as exc:
            rows = []
            gaps.append(f"сверка пинов не выполнена ({describe(exc)})")
        for _key, change, old, new, text in rows:
            notes, prs = [], []
            if change in ("update", "rollback"):
                notes, prs = pin_evidence(change, old, new, gaps)
            facts["pins"].append(
                {
                    "name": (new or old)["name"],
                    "change": change,
                    "text": text,
                    "component_notes": [n.strip() for n in notes],
                    "pull_requests": prs,
                }
            )
    if a.kind == "client":
        server = server_pin(
            next(r for r in releases if r["tag_name"] == previous), gaps
        )
        if server:
            facts["pins"].append(server)
    try:
        facts["pull_requests"] = merged_prs(
            a.source_repository, previous, a.version, gaps
        )
    except Exception as exc:
        gaps.append(f"слитые PR не получены ({describe(exc)})")
    if a.build_json:
        try:
            facts["no_client_changes"] = client_unchanged(
                a.build_json, releases, a.repository, previous
            )
        except Exception as exc:
            gaps.append(f"отпечаток клиента не сверен ({describe(exc)})")
    return facts


def sources_text(facts, by_model):
    lines = []
    previous = facts["previous_version"]
    if previous:
        lines.append(f"- Предыдущий опубликованный релиз линии: {previous}.")
        if facts["pins_checked"]:
            lines.append(f"- Сверка пинов {previous} → {facts['version']}.")
    else:
        lines.append("- Предыдущего опубликованного релиза линии нет.")
    if facts["pull_requests"]:
        numbers = ", ".join(f"#{pr['number']}" for pr in facts["pull_requests"])
        lines.append(f"- Слитые pull request: {numbers}.")
    if facts["no_client_changes"] is True:
        lines.append(
            "- Отпечаток клиентских файлов совпадает с предыдущим релизом (build.json)."
        )
    lines += [f"- Неполнота: {gap.rstrip('.')}." for gap in facts["gaps"]]
    lines.append(
        "- Текст подготовлен автоматически по перечисленным источникам."
        if by_model
        else "- Текст собран без модели: названия pull request приведены как есть."
    )
    return "\n".join(lines)


def assemble(facts, sections, by_model):
    head = [
        "---",
        "schema_version: 1",
        f"repository: {facts['repository']}",
        f"version: {facts['version']}",
        f"previous_version: {facts['previous_version'] or 'null'}",
        f"date: {facts['date']}",
        f"kind: {facts['kind']}",
        "backfilled: false",
        "---",
        "",
        f"# {facts['repository']} {facts['version']}",
        "",
    ]
    body = []
    for key, title in SECTIONS:
        body += [f"## {title}", "", sections[key].strip(), ""]
    body += [
        "## Источники и ограничения полноты",
        "",
        sources_text(facts, by_model),
        "",
    ]
    return "\n".join(head + body)


def clean_prs(prs):
    """(публикуемые PR, число опущенных)."""
    kept, dropped = [], 0
    for pr in prs:
        if denied(pr["title"]):
            dropped += 1
            continue
        lines = [line for line in pr["body"].split("\n") if not denied(line)]
        kept.append(dict(pr, body="\n".join(lines)))
    return kept, dropped


def clean(facts):
    """Из фактов убираются записи, которые нельзя публиковать; число опущенных уходит в неполноту."""
    out = dict(facts, pins=[], gaps=list(facts["gaps"]))
    out["pull_requests"], dropped = clean_prs(facts["pull_requests"])
    for pin in facts["pins"]:
        notes = [n for n in pin["component_notes"] if not denied(n)]
        dropped += len(pin["component_notes"]) - len(notes)
        prs, skipped = clean_prs(pin.get("pull_requests", []))
        dropped += skipped
        if denied(pin["text"]):
            dropped += 1
        else:
            out["pins"].append(dict(pin, component_notes=notes, pull_requests=prs))
    out["gaps"] = [g for g in out["gaps"] if not denied(g)]
    if dropped:
        out["gaps"].append(f"часть записей опущена при проверке текста ({dropped})")
    return out


def facts_sections(facts):
    prs, pins = facts["pull_requests"], facts["pins"]
    pin_titles = [pr["title"] for pin in pins for pr in pin.get("pull_requests", [])]
    if prs:
        titles = "; ".join(pr["title"] for pr in prs[:5])
        brief = f"Выпуск {facts['version']}: {titles}."
    else:
        brief = f"Выпуск {facts['version']}. Описание изменений автоматически не подготовлено."
    if facts["no_client_changes"] is True:
        players = "Изменений для игроков в клиентской сборке нет — подтверждено отпечатком клиентских файлов."
    elif prs or pin_titles:
        players = "\n".join(
            f"- {title}" for title in [*(p["title"] for p in prs), *pin_titles]
        )
    else:
        players = NO_CHANGE_SENTENCE
    technical = (
        "\n".join(f"- {pr['title']} (#{pr['number']})" for pr in prs)
        if prs
        else "Сведений нет."
    )
    if pins:
        lines = []
        for pin in pins:
            lines.append(f"- {pin['text']}")
            lines += [f"    {n}" for n in pin["component_notes"]]
            lines += [
                f"    - {pr['title']} (#{pr['number']})"
                for pr in pin.get("pull_requests", [])
            ]
        components = "\n".join(lines)
    elif facts["pins_checked"]:
        components = "Пины не изменились относительно предыдущего релиза."
    else:
        components = "Сверка компонентов для этого релиза не выполнялась."
    return {
        "brief": brief,
        "players": players,
        "technical": technical,
        "components": components,
        "compatibility": "Сведения о совместимости автоматически не подготовлены.",
        "known_issues": "Сведения об известных проблемах автоматически не подготовлены.",
    }


def check_text(text, facts):
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp, "release-notes.md")
        path.write_text(text, encoding="utf-8", newline="\n")
        return rn.check(path, facts["repository"], facts["version"], facts["kind"])


def facts_only_notes(facts):
    """release-notes.md из фактов; если и он не проходит check, остаётся минимальный."""
    cleaned = clean(facts)
    text = assemble(cleaned, facts_sections(cleaned), by_model=False)
    errors = check_text(text, cleaned)
    if errors:
        warn("текст из фактов не прошёл check: " + "; ".join(errors[:3]))
        bare = dict(cleaned, pull_requests=[], pins=[], no_client_changes=None)
        sections = facts_sections(bare)
        sections["brief"] = f"Выпуск {facts['version']}."
        text = assemble(bare, sections, by_model=False)
    return text


def parse_json_object(text):
    text = text.strip()
    fenced = re.search(r"(?s)```(?:json)?\s*(.*?)```", text)
    if fenced:
        text = fenced.group(1)
    start, end = text.find("{"), text.rfind("}")
    if start < 0 or end <= start:
        raise ValueError("в ответе нет JSON-объекта")
    return json.loads(text[start : end + 1])


def has_evidence(facts):
    """Есть ли из чего узнать, ЧТО изменилось: PR, смысл от компонентов, добавленные/удалённые пины."""
    return bool(facts["pull_requests"]) or any(
        pin["component_notes"]
        or pin.get("pull_requests")
        or pin["change"] in ("added", "removed")
        for pin in facts["pins"]
    )


FILLER_RULES = (
    (
        BUMP_RE,
        "строка об обновлении версий; номера версий допустимы только в «Технические изменения» "
        "и «Обновлённые компоненты», игроку напиши, ЧТО изменилось (поведение, внешний вид, исправления)",
    ),
    (
        NOT_RESTORED_RE,
        "заглушка «не восстановлено»; нет сведений об изменении - убери пункт (пробел укажут "
        "«Источники и ограничения полноты»)",
    ),
    (ADVICE_RE, "призыв обновиться вместо описания изменений"),
)


def player_errors(sections, announcement, facts):
    """Игровые разделы и анонс не должны быть заглушками вместо описания изменений."""
    fields = {
        "«Кратко»": sections["brief"],
        "«Для игроков»": sections["players"],
        **{f"announcement.{k}": announcement[k] for k in ANNOUNCEMENT_LIMITS},
    }
    errors = []
    for name, text in fields.items():
        for rx, what in FILLER_RULES:
            found = rx.search(text)
            if found:
                errors.append(f"{name}: «{found.group(0)[:80]}» - {what}")
    if (
        not has_evidence(facts)
        and facts["no_client_changes"] is not True
        and not NO_CHANGE_RE.search(sections["players"])
    ):
        errors.append(
            "«Для игроков»: в фактах нет сведений об изменениях, напиши прямо: "
            f"«{NO_CHANGE_SENTENCE}»"
        )
    return errors


def validate_release(obj, facts):
    """(release-notes.md, announcement, ошибки) по ответу модели."""
    errors = []
    if not isinstance(obj, dict) or not all(
        isinstance(obj.get(k), dict) for k in ("release_notes", "announcement")
    ):
        return (
            None,
            None,
            ["ответ должен содержать объекты release_notes и announcement"],
        )
    sections = {}
    for key, title in SECTIONS:
        value = obj["release_notes"].get(key)
        if isinstance(value, str) and value.strip():
            sections[key] = value
        else:
            errors.append(f"release_notes.{key} ({title}) пуст или не строка")
    announcement = {}
    for key in (*ANNOUNCEMENT_LIMITS, "color_hex"):
        value = obj["announcement"].get(key)
        if not isinstance(value, str):
            errors.append(f"announcement.{key} должен быть строкой")
            continue
        announcement[key] = value.strip()
    for key, limit in ANNOUNCEMENT_LIMITS.items():
        value = announcement.get(key)
        if value is not None and len(value) > limit:
            errors.append(f"announcement.{key} длиннее {limit} символов")
    if announcement.get("title") == "" or announcement.get("summary") == "":
        errors.append(
            "announcement.title и announcement.summary не должны быть пустыми"
        )
    color = announcement.get("color_hex", "").lstrip("#")
    if "color_hex" in announcement:
        if re.fullmatch(r"[0-9A-Fa-f]{6}", color):
            announcement["color_hex"] = color.upper()
        else:
            errors.append("announcement.color_hex должен быть шестью hex-цифрами")
    if errors:
        return None, None, errors
    errors += player_errors(sections, announcement, facts)
    text = assemble(facts, sections, by_model=True)
    errors += check_text(text, facts)
    components = sections["components"].lower()
    errors += [
        f"в «Обновлённые компоненты» нет компонента {pin['name']}"
        for pin in facts["pins"]
        if pin["name"].lower() not in components
    ]
    everything = text + "\n" + "\n".join(announcement.values())
    errors += [
        f"запрещено публиковать: {item}" for item in sorted(set(denied(everything)))
    ]
    return text, announcement, errors


def chat(system, user, model, key):
    body = json.dumps(
        {
            "model": model,
            "temperature": 0.2,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
        }
    ).encode()
    req = urllib.request.Request(
        API_URL,
        data=body,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        return json.load(resp)["choices"][0]["message"]["content"]


def ask(system, user, validate, model, key):
    """Две попытки; вторая получает ошибки первой. Возвращает результат validate или None."""
    retry = ""
    for attempt in range(ATTEMPTS):
        try:
            result = validate(chat(system, user + retry, model, key))
            errors = result[-1]
        except urllib.error.HTTPError as exc:
            errors = [f"HTTP {exc.code}"]
        except Exception as exc:
            errors = [f"{type(exc).__name__}: {str(exc)[:120]}"]
        if not errors:
            return result
        warn(f"попытка {attempt + 1}: " + "; ".join(errors[:5]))
        retry = (
            "\n\nПредыдущий ответ отклонён проверкой. Исправь и верни ответ целиком:\n- "
            + "\n- ".join(errors[:10])
        )
        if attempt + 1 < ATTEMPTS:
            time.sleep(2)
    return None


def fit_facts(facts):
    """Факты для модели в пределах бюджета: сначала сокращаются тела PR."""

    def trim(prs, limit):
        return [dict(pr, body=pr["body"][:limit]) for pr in prs]

    for limit in (PR_BODY_CHARS, 600, 0):
        trimmed = dict(
            facts,
            pull_requests=trim(facts["pull_requests"], limit),
            pins=[
                dict(pin, pull_requests=trim(pin.get("pull_requests", []), limit))
                for pin in facts["pins"]
            ],
        )
        text = json.dumps(trimmed, ensure_ascii=False, indent=1)
        if len(text) <= FACTS_BUDGET:
            break
    return text[:FACTS_BUDGET]


def reuse_published(releases, a, out):
    """Повторный прогон: уже опубликованные файлы остаются как были."""
    release = next((r for r in releases if r["tag_name"] == a.version), None)
    assets = {x["name"]: x for x in release["assets"]} if release else {}
    if "release-notes.md" not in assets:
        return False
    notes = asset_bytes(a.repository, assets["release-notes.md"])
    (out / "release-notes.md").write_bytes(notes)
    if rn.check(out / "release-notes.md", a.repository, a.version, FRONT_KIND[a.kind]):
        (out / "release-notes.md").unlink()
        return False
    if "announcement.json" in assets:
        (out / "announcement.json").write_bytes(
            asset_bytes(a.repository, assets["announcement.json"])
        )
    return True


def report(gaps, outcome, version):
    """Пробелы видны в аннотациях и в сводке шага; сборка не падает."""
    for gap in gaps:
        annotate(gap, "release-text: неполнота")
    target = os.environ.get("GITHUB_STEP_SUMMARY")
    if not target:
        return
    lines = [f"### release-text {version}", "", f"Текст: {outcome}.", ""]
    lines += [f"- Неполнота: {' '.join(gap.split())}" for gap in gaps] or [
        "- Пробелов в источниках нет."
    ]
    with open(target, "a", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n\n")


def run_release(a):
    out = Path(a.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    for name in ("release-notes.md", "announcement.json"):
        (out / name).unlink(missing_ok=True)
    releases = published_releases(a.repository)
    if not a.fresh and reuse_published(releases, a, out):
        print(f"release-text: {a.version} уже опубликован, файлы взяты из релиза")
        return
    collected = collect_facts(a, releases)
    facts = clean(collected)
    key, model = os.environ.get("OPENROUTER_API_KEY"), a.model
    result = None
    if key and model:
        system = (HERE / "prompts" / "release.md").read_text(encoding="utf-8")
        user = "Факты выпуска (JSON):\n" + fit_facts(facts)
        result = ask(
            system,
            user,
            lambda raw: validate_release(parse_json_object(raw), facts),
            model,
            key,
        )
        outcome = "только из фактов: ответы модели отклонены проверкой"
    else:
        warn("нет OPENROUTER_API_KEY или RELEASE_TEXT_MODEL: текст только из фактов")
        outcome = "только из фактов: нет ключа или модели"
    if result:
        report(collected["gaps"], "написан моделью и прошёл проверку", a.version)
        notes, announcement, _ = result
        (out / "release-notes.md").write_text(notes, encoding="utf-8", newline="\n")
        (out / "announcement.json").write_text(
            json.dumps(announcement, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        return
    report(collected["gaps"], outcome, a.version)
    (out / "release-notes.md").write_text(
        facts_only_notes(facts), encoding="utf-8", newline="\n"
    )


def pr_prompt(pr, commits, files):
    parts = [f"Название PR: {pr['title']}", "", "Коммиты:"]
    for c in commits:
        message = c["commit"]["message"].strip()
        parts.append("- " + message[:600].replace("\n", "\n  "))
    parts += ["", "Файлы (сначала крупнейшие изменения того, что видит игрок):"]
    budget = PATCH_TOTAL_CHARS
    for f in sorted(files, key=importance, reverse=True):
        stat = f"{f['filename']} ({f['status']}, +{f['additions']}/-{f['deletions']})"
        patch = f.get("patch") or ""
        if f["filename"].endswith(NAME_ONLY):
            patch = ""
        # Большой diff часто и есть суть PR: модели нужны хотя бы его добавленные строки.
        if len(patch) > PATCH_FILE_CHARS:
            added = [line for line in patch.split("\n") if line.startswith("+")]
            patch = "\n".join(added)[:PATCH_EXCERPT_CHARS]
            if patch:
                stat += ", большой: только добавленные строки, обрезано"
        if not patch or len(patch) > budget:
            parts.append(
                f"- {stat}: только имя (двоичный, большой или сгенерированный)"
            )
            continue
        budget -= len(patch)
        parts += [f"- {stat}", "```diff", patch, "```"]
    return "\n".join(parts)


def validate_pr(text):
    text = text.strip()
    fenced = re.fullmatch(r"(?s)```(?:markdown|md)?\s*(.*?)\s*```", text)
    text = fenced.group(1).strip() if fenced else text
    errors = []
    if not text:
        errors.append("пустое описание")
    if len(text) > PR_BODY_LIMIT:
        errors.append(f"описание длиннее {PR_BODY_LIMIT} символов")
    if not re.search(r"(?m)^Для игроков:\s*\S", text):
        errors.append("нет строки «Для игроков: …» или «Для игроков: нет»")
    errors += [f"запрещено публиковать: {i}" for i in sorted(set(denied(text)))]
    return text, errors


def run_pr(a):
    key = os.environ.get("OPENROUTER_API_KEY")
    if not (key and a.model):
        warn("нет OPENROUTER_API_KEY или RELEASE_TEXT_MODEL: описание PR не создаётся")
        return
    repo, number = a.repository, a.pr_number
    pr = json.loads(gh("api", f"repos/{repo}/pulls/{number}"))
    if split_footer(pr.get("body"))[0]:
        return
    commits = flatten(gh_pages(f"repos/{repo}/pulls/{number}/commits?per_page=100"))
    files = flatten(gh_pages(f"repos/{repo}/pulls/{number}/files?per_page=100"))
    result = ask(
        (HERE / "prompts" / "pr.md").read_text(encoding="utf-8"),
        pr_prompt(pr, commits, files),
        validate_pr,
        a.model,
        key,
    )
    if not result:
        return
    # Автор мог заполнить описание, пока шла генерация.
    current, footer = split_footer(
        json.loads(gh("api", f"repos/{repo}/pulls/{number}")).get("body")
    )
    if current:
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp, "body.md")
        path.write_text(
            result[0] + (f"\n\n{footer}" if footer else ""),
            encoding="utf-8",
            newline="\n",
        )
        gh("api", "-X", "PATCH", f"repos/{repo}/pulls/{number}", "-F", f"body=@{path}")


def write_outputs(out_dir):
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        return
    out = Path(out_dir)
    with open(target, "a", encoding="utf-8") as fh:
        fh.write(f"notes={out / 'release-notes.md'}\n")
        has = (out / "announcement.json").is_file()
        fh.write(f"announcement={out / 'announcement.json' if has else ''}\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    for name in ("release", "pr"):
        p = sub.add_parser(name)
        p.add_argument("--repository", required=True)
        p.add_argument("--model", default=os.environ.get("RELEASE_TEXT_MODEL", ""))
    r = sub.choices["release"]
    r.add_argument("--version", required=True)
    r.add_argument("--kind", required=True, choices=sorted(FRONT_KIND))
    r.add_argument("--out-dir", required=True)
    r.add_argument("--source-repository")
    r.add_argument("--repo-root", default=".")
    r.add_argument("--build-json")
    r.add_argument(
        "--fresh",
        action="store_true",
        help="не брать уже опубликованные файлы, а сгенерировать заново",
    )
    r.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    sub.choices["pr"].add_argument("--pr-number", required=True, type=int)
    a = parser.parse_args(argv)
    try:
        if a.mode == "release":
            a.source_repository = a.source_repository or a.repository
            run_release(a)
        else:
            run_pr(a)
    except Exception as exc:
        warn(describe(exc))
        if a.mode == "release":
            try:
                report(
                    [f"сбор фактов не удался ({describe(exc)})"],
                    "только из фактов: сбой генерации",
                    a.version,
                )
                Path(a.out_dir).mkdir(parents=True, exist_ok=True)
                for name in ("release-notes.md", "announcement.json"):
                    Path(a.out_dir, name).unlink(missing_ok=True)
                facts = {
                    "repository": a.repository,
                    "version": a.version,
                    "previous_version": None,
                    "kind": FRONT_KIND[a.kind],
                    "date": a.date,
                    "no_client_changes": None,
                    "pins_checked": False,
                    "pins": [],
                    "pull_requests": [],
                    "gaps": ["сбор фактов не удался (см. журнал шага)"],
                }
                Path(a.out_dir, "release-notes.md").write_text(
                    facts_only_notes(facts), encoding="utf-8", newline="\n"
                )
            except Exception as inner:
                warn(f"минимальный release-notes.md не создан: {inner}")
    if a.mode == "release":
        write_outputs(a.out_dir)


if __name__ == "__main__":
    main()
    sys.exit(0)
