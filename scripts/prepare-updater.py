#!/usr/bin/env python3
"""Build the public updater manifest from completed, signed release assets."""
import json
import sys
from pathlib import Path
from urllib.parse import quote


def prepare(directory, version):
    path = directory / 'latest.json'
    data = json.loads(path.read_text())
    if data['version'] != version:
        raise ValueError('Updater version does not match release')
    base = f'https://github.com/netherg-io/blockfield-launcher-releases/releases/download/launcher-v{version}/'
    assets = {
        'linux-x86_64': (f'blockfield-launcher_{version}_amd64.AppImage', 'appimage'),
        'windows-x86_64': (f'blockfield-launcher_{version}_x64_en-US.msi', 'msi'),
        'darwin-aarch64': ('blockfield-launcher_aarch64.app.tar.gz', 'app'),
        'darwin-x86_64': ('blockfield-launcher_x64.app.tar.gz', 'app'),
        'windows-x86_64-nsis': (f'blockfield-launcher_{version}_x64-setup.exe', None),
        'linux-x86_64-deb': (f'blockfield-launcher_{version}_amd64.deb', None),
        'linux-x86_64-rpm': (f'blockfield-launcher-{version}-1.x86_64.rpm', None),
    }
    platforms = {}
    for platform, (name, bundle) in assets.items():
        if not (directory / name).is_file():
            raise ValueError(f'Missing updater asset: {name}')
        signature = (directory / (name + '.sig')).read_text().strip()
        if not signature:
            raise ValueError(f'Empty updater signature: {name}')
        entry = {'signature': signature, 'url': base + quote(name)}
        platforms[platform] = entry
        if bundle:
            platforms[f'{platform}-{bundle}'] = entry
    data['platforms'] = platforms
    path.write_text(json.dumps(data, indent=2) + '\n')


if __name__ == '__main__':
    prepare(Path(sys.argv[1]), sys.argv[2])
