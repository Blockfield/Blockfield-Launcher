import importlib.util
import json
import tempfile
import sys
from pathlib import Path

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('updater', Path(__file__).with_name('prepare-updater.py'))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)

with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    manifest = root / 'latest.json'
    manifest.write_text(json.dumps({'version': '1.0.1', 'platforms': {'obsolete-platform': {}}, 'notes': 'release'}))
    names = ['blockfield-launcher_1.0.1_amd64.AppImage', 'blockfield-launcher_1.0.1_x64_en-US.msi',
             'blockfield-launcher_aarch64.app.tar.gz', 'blockfield-launcher_x64.app.tar.gz',
             'blockfield-launcher_1.0.1_x64-setup.exe', 'blockfield-launcher_1.0.1_amd64.deb',
             'blockfield-launcher-1.0.1-1.x86_64.rpm']
    for name in names:
        (root / name).write_bytes(b'bundle')
        (root / (name + '.sig')).write_text('signature-' + name + '\n')
    updater.prepare(root, '1.0.1')
    data = json.loads(manifest.read_text())
    assert len(data['platforms']) == 11
    assert 'obsolete-platform' not in data['platforms']
    assert 'darwin-x86_64' in data['platforms']
    assert data['platforms']['darwin-x86_64'] == data['platforms']['darwin-x86_64-app']
    assert data['notes'] == 'release'
    for platform in data['platforms'].values():
        name = platform['url'].rsplit('/', 1)[-1]
        assert platform['signature'] == 'signature-' + name
        assert platform['url'].startswith('https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v1.0.1/')
    assert 'darwin-aarch64' in data['platforms']
    original = manifest.read_bytes()
    (root / 'blockfield-launcher_x64.app.tar.gz').unlink()
    try:
        updater.prepare(root, '1.0.1')
    except ValueError as error:
        assert 'Missing updater asset' in str(error)
    else:
        raise AssertionError('Missing Intel Mac bundle accepted')
    assert manifest.read_bytes() == original
print('PASS: reconstructs all platforms and rejects missing bundles before changing manifest')
