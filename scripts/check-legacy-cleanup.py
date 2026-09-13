#!/usr/bin/env python3
"""Guard the static launcher architecture; requires Python 3.11+."""
import json
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
errors = []
workspace = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']
members = workspace['members']
if 'server' in members or (root / 'server').exists():
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
