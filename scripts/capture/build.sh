#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
CACHE="$ROOT/.cache/capture"
mkdir -p "$CACHE/source" "$CACHE/cargo-target" "$HOME/.cargo/registry"
python3 scripts/capture/prepare-fonts.py
# Copy source only: no .env, user data, credentials, or unrelated untracked files.
git ls-files -z | tar --null -T - -cf - | tar -xf - -C "$CACHE/source"
cp -a scripts/capture "$CACHE/source/scripts/"
mkdir -p "$CACHE/source/src/lib/fixtures"
cp -a src/lib/fixtures/. "$CACHE/source/src/lib/fixtures/"
cp src/main.tsx "$CACHE/source/src/main.tsx"
cp "$CACHE/fonts/fonts.css" "$CACHE/source/src/styles/fonts.css"
mkdir -p "$CACHE/source/public/capture-fonts"
cp "$CACHE"/fonts/*.woff2 "$CACHE/source/public/capture-fonts/"
ln -sfn "$ROOT/node_modules" "$CACHE/source/node_modules"
(cd "$CACHE/source" && VITE_CAPTURE_FIXTURES=1 VITE_BLOCKFIELD_PACK_URL=https://capture.invalid pnpm build)
python3 - "$CACHE/source/dist" <<'PY_CHECK'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
js = '\n'.join(p.read_text() for p in root.rglob('*.js'))
css = '\n'.join(p.read_text() for p in root.rglob('*.css'))
assert 'https://capture.invalid' in js, 'Missing capture-only API URL; do not spend a release build'
assert 'capture-ready:' in js, 'Capture fixture flag was not enabled'
assert 'fonts.googleapis.com' not in css and 'fonts.gstatic.com' not in css, 'Remote fonts invalidate captures'
assert 'capture-fonts/' in css, 'Local capture fonts missing'
print('Capture frontend preflight passed')
PY_CHECK
python3 - "$CACHE/source" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
cap = json.loads((root / 'src-tauri/capabilities/default.json').read_text())
cap['permissions'].append('core:webview:allow-set-webview-zoom')
(root / 'src-tauri/capabilities/default.json').write_text(json.dumps(cap))
PY
podman run --rm --userns keep-id --network slirp4netns \
    -v "$CACHE/source:/work:Z" -v "$CACHE/cargo-target:/target:Z" \
    -v "$HOME/.cargo/registry:/usr/local/cargo/registry:z" \
    -w /work/src-tauri -e CARGO_BUILD_JOBS=4 -e CARGO_TARGET_DIR=/target \
    localhost/blockfield-capture:latest cargo build --release --locked --features tauri/custom-protocol
python3 - "$ROOT" <<'PY'
import hashlib, json, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1])
cache = root / '.cache/capture'
binary = cache / 'cargo-target/release/blockfield-launcher'
info = {'valid': True, 'buildSha': subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(),
        'sourceDiffSha256': hashlib.sha256(subprocess.check_output(['git','diff','HEAD'])).hexdigest(),
        'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'profile': 'release', 'jobs': 4,
        'image': subprocess.check_output(['podman','image','inspect','localhost/blockfield-capture:latest','--format','{{.Id}}'], text=True).strip(),
        'fontSources': json.loads((cache / 'fonts/sources.json').read_text()),
        'sourceFilesSha256': {str(p.relative_to(cache / 'source')): hashlib.sha256(p.read_bytes()).hexdigest()
            for base in ('src', 'dist') for p in sorted((cache / 'source' / base).rglob('*')) if p.is_file()}}
(cache / 'build.json').write_text(json.dumps(info, indent=2) + '\n')
PY
