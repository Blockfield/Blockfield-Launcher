#!/usr/bin/env python3
"""Guard the static launcher architecture; requires Git and Python 3.11+."""
import json
import subprocess
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
errors = []
workspace = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']
members = workspace['members']
# Ignore historical local data that may remain in an existing checkout. Never
# require deleting data directories just to pass an architecture check.
try:
    candidates = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', 'server'],
        cwd=root,
    ).decode().split('\0')
except (OSError, subprocess.CalledProcessError) as error:
    print(f'Unable to inspect legacy source with Git: {error}', file=sys.stderr)
    raise SystemExit(1) from error
legacy_source = [path for path in candidates if path and (root / path).exists()]
if 'server' in members or legacy_source:
    errors.append('Retired server source/workspace member has returned.')
if not {'shared', 'src-tauri'}.issubset(members):
    errors.append('The active native launcher/shared workspace members are missing.')
scripts = json.loads((root / 'package.json').read_text())['scripts']
if {'api:dev', 'cms:publish-modpack'}.intersection(scripts):
    errors.append('Retired API/CMS npm entrypoints have returned.')
if any('blockfield-server' in value or 'server/publish-modpack' in value for value in scripts.values()):
    errors.append('An npm command still targets the retired backend.')
lock = tomllib.loads((root / 'Cargo.lock').read_text())
if any(package['name'] == 'blockfield-server' for package in lock['package']):
    errors.append('Cargo.lock still contains the retired workspace package.')
native = tomllib.loads((root / 'src-tauri/Cargo.toml').read_text())
if native['dependencies'].get('blockfield-shared', {}).get('path') != '../shared':
    errors.append('The active shared contract dependency must be retained.')
readme = (root / 'README.md').read_text()
for stale in ('Three independent deployable parts:', 'VITE_BLOCKFIELD_API_URL', 'pnpm cms:publish-modpack', '## Local server smoke test'):
    if stale in readme:
        errors.append(f'Active README still contains retired setup instructions: {stale}')
for relative in ('docs/LAUNCHER_SMOKE.md', 'docs/ROOMS-AND-DISCORD.md', 'docs/legacy/README.md'):
    if not (root / relative).is_file():
        errors.append(f'Missing current/archive documentation: {relative}')
if errors:
    print('\n'.join(errors), file=sys.stderr)
    raise SystemExit(1)
print('Static launcher boundary verified; active shared contracts preserved.')
