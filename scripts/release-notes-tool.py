"""Единый формат release-notes.md: генерация скелета, сверка пинов и проверка.

Скелет готовится из git-истории (теги/коммиты/вошедшие PR) и сверки
*.pw.toml между предыдущим и новым релизом той же линии. Текст разделов
пишет человек в релизном PR; CI только проверяет формат/соответствие
версии и публикует файл. Вызов внешней модели не нужен.
"""

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SCHEMA_VERSION = 1
KINDS = ('client', 'server', 'launcher', 'mod', 'resource')
REQUIRED_KEYS = ('schema_version', 'repository', 'version', 'previous_version',
                 'date', 'kind', 'backfilled')
REQUIRED_SECTIONS = ('Кратко', 'Для игроков', 'Технические изменения',
                     'Обновлённые компоненты', 'Совместимость и необходимые действия',
                     'Известные проблемы', 'Источники и ограничения полноты')

VERSION_RE = {'client': r'v\d+\.\d+\.\d+', 'server': r'v\d+\.\d+\.\d+',
              'mod': r'v\d+\.\d+\.\d+', 'resource': r'.+',
              'launcher': r'launcher-v\d+\.\d+\.\d+'}


def sh(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()


def pin_url_version(url):
    """Тег компонента из URL пина: .../releases/download/<tag>/<file>."""
    m = re.search(r'/releases/download/([^/]+)/', url)
    return m.group(1) if m else None


def read_pins(root):
    pins = {}
    for path in sorted(Path(root, 'mods').glob('*.pw.toml')):
        try:
            mod = tomllib.loads(path.read_text())
        except (OSError, tomllib.TOMLDecodeError):
            continue
        url = (mod.get('download') or {}).get('url', '')
        tag = pin_url_version(url)
        if tag:
            pins[path.stem] = {'name': mod.get('name', path.stem),
                               'filename': mod.get('filename', ''),
                               'url': url, 'tag': tag,
                               'hash': (mod.get('download') or {}).get('hash', '')}
    return pins


def pin_diff(old_pins, new_pins):
    """added/removed/changed/rollback/url-change между двумя наборами пинов."""
    rows = []
    for key in sorted(set(old_pins) | set(new_pins)):
        old, new = old_pins.get(key), new_pins.get(key)
        if old is None:
            rows.append((key, 'added', None, new['tag'],
                         f"Добавлен {new['name']}: {new['filename']} ({new['tag']})."))
        elif new is None:
            rows.append((key, 'removed', old['tag'], None,
                         f"Удалён {old['name']} ({old['tag']})."))
        elif old['tag'] == new['tag'] and old['url'] == new['url'] and old['hash'] == new['hash']:
            continue
        elif old['tag'] == new['tag'] and old['hash'] == new['hash'] and old['url'] != new['url']:
            rows.append((key, 'url-change', old['tag'], new['tag'],
                         f"{new['name']}: тот же тег {new['tag']}, байты те же, URL замены: {new['url']}."))
        elif old['tag'] == new['tag'] and old['hash'] != new['hash']:
            rows.append((key, 're-pinned', old['tag'], new['tag'],
                         f"{new['name']}: тег {new['tag']} пересобран (хеш изменился)."))
        else:
            direction = 'откат' if _is_rollback(old['tag'], new['tag']) else 'обновление'
            rows.append((key, 'changed', old['tag'], new['tag'],
                         f"{new['name']}: {direction} {old['tag']} → {new['tag']} "
                         f"({new['filename']})."))
    return rows


def _is_rollback(old_tag, new_tag):
    mo, mn = re.fullmatch(r'v(\d+)\.(\d+)\.(\d+)', old_tag or ''), \
        re.fullmatch(r'v(\d+)\.(\d+)\.(\d+)', new_tag or '')
    if mo and mn:
        return tuple(map(int, mn.groups())) < tuple(map(int, mo.groups()))
    mo, mn = re.fullmatch(r'bf(\d+)', old_tag or ''), re.fullmatch(r'bf(\d+)', new_tag or '')
    if mo and mn:
        return int(mn.group(1)) < int(mo.group(1))
    return False


def parse_front_matter(text):
    if not text.startswith('---\n'):
        return None, 'нет YAML-заголовка (файл должен начинаться с ---\n)'
    end = text.find('\n---\n', 4)
    if end < 0:
        return None, 'YAML-заголовок не закрыт строкой ---\n'
    meta, body = {}, text[4:end]
    for lineno, line in enumerate(body.split('\n'), 1):
        if not line.strip() or line.strip().startswith('#'):
            continue
        m = re.fullmatch(r'([A-Za-z_]+):\s*(.*)', line.strip())
        if not m:
            return None, f'заголовок, строка {lineno}: ожидается `ключ: значение`, got {line!r}'
        key, value = m.group(1), m.group(2).strip()
        if (value.startswith('"') and value.endswith('"')) or \
                (value.startswith("'") and value.endswith("'")):
            value = value[1:-1]
        meta[key] = value
    return meta, None


def check(path, repository=None, version=None, kind=None):
    """Проверить release-notes.md. Возвращает список ошибок (пусто = ок)."""
    errors = []
    try:
        text = Path(path).read_text(encoding='utf-8')
    except OSError as exc:
        return [f'не читается: {exc}']
    meta, err = parse_front_matter(text)
    if err:
        return [err]
    for key in REQUIRED_KEYS:
        if key not in meta:
            errors.append(f'заголовок: отсутствует ключ `{key}`')
    if errors:
        return errors
    body = text.split('\n---\n', 1)[1]
    if 'schema_version: 1' not in text.split('\n---\n', 1)[0]:
        errors.append('заголовок: schema_version должен быть 1')
    try:
        if int(meta['schema_version']) != SCHEMA_VERSION:
            errors.append(f'заголовок: schema_version={meta["schema_version"]}, ожидается {SCHEMA_VERSION}')
    except ValueError:
        errors.append('заголовок: schema_version не число')
    if meta.get('repository') != (repository or meta.get('repository')):
        pass
    if repository and meta.get('repository') != repository:
        errors.append(f'заголовок: repository={meta["repository"]!r}, ожидается {repository!r}')
    if version and meta.get('version') != version:
        errors.append(f'заголовок: version={meta["version"]!r}, ожидается {version!r}')
    exp_kind = kind or meta.get('kind')
    if meta.get('kind') not in KINDS:
        errors.append(f'заголовок: kind={meta.get("kind")!r}, ожидается одно из {", ".join(KINDS)}')
    elif kind and meta.get('kind') != kind:
        errors.append(f'заголовок: kind={meta["kind"]!r}, ожидается {kind!r}')
    pattern = VERSION_RE.get(meta.get('kind', ''), r'.+')
    if not re.fullmatch(pattern, meta.get('version', '')):
        errors.append(f'заголовок: version={meta.get("version")!r} не matches {pattern} для kind={meta.get("kind")}')
    if meta.get('previous_version') in (None, ''):
        errors.append('заголовок: previous_version должен быть тегом или null')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', meta.get('date', '')):
        errors.append(f'заголовок: date={meta.get("date")!r}, ожидается YYYY-MM-DD')
    if meta.get('backfilled') not in ('true', 'false'):
        errors.append('заголовок: backfilled должен быть true/false')
    for section in REQUIRED_SECTIONS:
        if not re.search(rf'(?m)^##\s+{re.escape(section)}\s*$', body):
            errors.append(f'раздел отсутствует: `## {section}`')
    notes = re.search(r'(?m)^##\s+Источники и ограничения полноты\s*$(.*?)(?=^##\s|\Z)',
                      body, re.S)
    if notes and not notes.group(1).strip():
        errors.append('раздел «Источники и ограничения полноты» пуст')
    if re.search(r'(?mi)изменений для игроков нет', body):
        players = re.search(r'(?m)^##\s+Для игроков\s*$(.*?)(?=^##\s|\Z)', body, re.S)
        confirmed = players and re.search(r'(?i)подтвержден|подтверждено|по тестам|пустой diff|без изменений', players.group(1))
        sources = notes.group(1) if notes else ''
        if not confirmed and not re.search(r'(?i)подтвержден|проверен|пустой diff', sources):
            errors.append('«Изменений для игроков нет» без указания подтверждения в разделе/источниках')
    return errors


def skeleton(repository, version, previous_version, kind, date, commits=(),
             components=(), sources=(), gaps=()):
    lines = ['---', 'schema_version: 1', f'repository: {repository}',
             f'version: {version}',
             f'previous_version: {previous_version or "null"}',
             f'date: {date}', f'kind: {kind}', 'backfilled: false', '---', '',
             f'# {repository} {version}', '']
    lines += ['## Кратко', '', 'TODO: 2–4 предложения: что это за выпуск и зачем обновляться.', '']
    lines += ['## Для игроков', '',
              'TODO. Новое / исправления / изменения поведения. Если игровых изменений',
              'подтверждённо нет — так и написать с указанием подтверждения.',
              '']
    lines += ['## Технические изменения', '', 'TODO.', '']
    lines += ['## Обновлённые компоненты', '']
    if components:
        for _key, _change, old, new, text in components:
            lines.append(f'- {text}')
            if old and new and old != new:
                lines.append(f'  Содержание изменений за промежуток {old} → {new}: TODO (release-notes компонентов).')
    else:
        lines.append('Зависимости не изменились относительно предыдущего релиза (сверка *.pw.toml).')
    if gaps:
        lines.append('')
        for gap in gaps:
            lines.append(f'- ВНИМАНИЕ: пропущенные промежуточные версии: {gap} — сверить перед публикацией.')
    lines += ['', '## Совместимость и необходимые действия', '', 'TODO: совместимость, нужен ли вайп/перезаход/обновление лаунчера.', '']
    lines += ['## Известные проблемы', '', 'Нет известных проблем. / TODO.', '']
    lines += ['## Источники и ограничения полноты', '']
    if commits:
        lines.append('Вошли коммиты (по git-истории, проверить в PR):')
        for sha, subject in commits:
            lines.append(f'- {sha} {subject}')
        lines.append('')
    for src in sources:
        lines.append(f'- {src}')
    if not sources and not commits:
        lines.append('- TODO: перечислить источники (теги, PR, manifests/pins).')
    lines += ['', 'Неполнота: TODO или «полно — сверено с ...».', '']
    return '\n'.join(lines)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Скелет и проверка release-notes.md')
    sub = parser.add_subparsers(dest='cmd', required=True)
    c = sub.add_parser('check')
    c.add_argument('file')
    c.add_argument('--repository')
    c.add_argument('--version')
    c.add_argument('--kind', choices=KINDS)
    s = sub.add_parser('skeleton')
    s.add_argument('--repository', required=True)
    s.add_argument('--version', required=True)
    s.add_argument('--previous-version', default=None)
    s.add_argument('--kind', required=True, choices=KINDS)
    s.add_argument('--date', required=True)
    s.add_argument('--repo-root', default='.')
    args = parser.parse_args()
    if args.cmd == 'check':
        errs = check(args.file, args.repository, args.version, args.kind)
        for err in errs:
            print(f'ERROR: {err}')
        sys.exit(1 if errs else 0)
    old, new = {}, {}
    if args.previous_version and args.repo_root != '-':
        try:
            old_src = sh(args.repo_root, 'show', f'{args.previous_version}:mods')
            new_src = sh(args.repo_root, 'show', f'{args.version}:mods')
            import tempfile
            with tempfile.TemporaryDirectory() as tmp:
                for label, ref, store in (('o', args.previous_version, old), ('n', args.version, new)):
                    d = Path(tmp, label)
                    d.mkdir()
                    subprocess.check_call(['git', '-C', args.repo_root, 'archive', ref, 'mods',
                                           f'resources.lock.json'],
                                          stdout=open(Path(tmp, f'{label}.tar'), 'wb'))
        except subprocess.CalledProcessError:
            pass
        try:
            import io
            import tarfile
            for label, ref, store in (('o', args.previous_version, old), ('n', args.version, new)):
                raw = subprocess.check_output(['git', '-C', args.repo_root, 'archive', ref, 'mods'])
                with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
                    for member in tar.getmembers():
                        if member.name.endswith('.pw.toml'):
                            mod = tomllib.loads(tar.extractfile(member).read().decode())
                            url = (mod.get('download') or {}).get('url', '')
                            tag = pin_url_version(url)
                            if tag:
                                store[Path(member.name).stem] = {
                                    'name': mod.get('name', ''), 'filename': mod.get('filename', ''),
                                    'url': url, 'tag': tag,
                                    'hash': (mod.get('download') or {}).get('hash', '')}
        except subprocess.CalledProcessError as exc:
            print(f'WARNING: не удалось прочитать пины тегов: {exc}', file=sys.stderr)
    rows = pin_diff(old, new)
    print(skeleton(args.repository, args.version, args.previous_version,
                   args.kind, args.date, components=rows))
