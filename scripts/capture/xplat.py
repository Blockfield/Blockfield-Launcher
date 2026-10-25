#!/usr/bin/env python3
"""Capture/benchmark only on disposable GitHub runners, never the owner's desktop."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/bench'))
import psutil
from xplat_metrics import identity, sample, cpu_delta, tree

STATES = ('main', 'main-launching', 'offline', 'error', 'settings-skin',
          'settings-general', 'settings-runtime', 'settings-commands', 'settings-launcher')


def wait(check, process, seconds=45):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f'App exited {process.returncode} before readiness')
        value = check()
        if value:
            return value
        time.sleep(.1)
    raise TimeoutError('45s app readiness deadline')


def config_path():
    if sys.platform == 'win32':
        base = Path(os.environ['APPDATA'])
    elif sys.platform == 'darwin':
        base = Path.home() / 'Library/Application Support'
    else:
        base = Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share'))
    return base / 'com.blockfield.capture/launcher-config.json'


def validate(evidence, state, motion):
    assert evidence['state'] == state and evidence['motion'] == motion, evidence
    assert evidence['viewport']['width'] == 1280 and evidence['viewport']['height'] == 720, evidence
    assert evidence['viewport']['dpr'] == 1, evidence
    assert evidence['fonts']['inter'] and any(
        f['family'].strip('\"\'') == 'Inter' and f['status'] == 'loaded'
        for f in evidence['fonts']['faces']), evidence
    assert not evidence['errors'] or state == 'error', evidence


@contextmanager
def launch(app, state, motion, args, directory):
    directory.mkdir(parents=True, exist_ok=True)
    baseline = set()
    for p in psutil.process_iter():
        try:
            baseline.add(identity(p))
        except psutil.NoSuchProcess:
            pass
    env = dict(os.environ, SLINT_BACKEND='winit-software', SLINT_SCALE_FACTOR='1',
               LIBGL_ALWAYS_SOFTWARE='1', GDK_SCALE='1', GDK_DPI_SCALE='1',
               WEBKIT_DISABLE_DMABUF_RENDERER='1', TZ='UTC')
    config = config_path()
    with tempfile.TemporaryDirectory(prefix='blockfield-xplat-') as temporary:
        if app == 'tauri':
            config.parent.mkdir(parents=True, exist_ok=True)
            config.write_text(json.dumps(dict(
                gameDir=str(Path(temporary) / 'game'), workshopGameDir=str(Path(temporary) / 'workshop'),
                javaPath='', ramMb=4096, autoUpdate=False, discordPresence=False,
                username='CaptureOperator', lang='capture-selection:' + json.dumps(dict(
                    state=state, lang='ru', scale=1, motion=motion)))))
            command = [str(args.tauri.resolve())]
        else:
            command = [str(args.slint.resolve()), '--state', state, '--idle-seconds', '100']
        with (directory / 'app.log').open('w') as log:
            process = subprocess.Popen(command, env=env, cwd=temporary, stdout=log, stderr=subprocess.STDOUT)
            root = psutil.Process(process.pid)
            tracked = {}
            try:
                def ready():
                    if app == 'slint':
                        return 'READY pid=' in (directory / 'app.log').read_text(errors='replace')
                    try:
                        lang = json.loads(config.read_text()).get('lang', '')
                    except (ValueError, FileNotFoundError):
                        return None
                    if lang.startswith('capture-failed:'):
                        raise RuntimeError(lang)
                    if lang.startswith('capture-ready:'):
                        return json.loads(lang[len('capture-ready:'):])
                    return None
                evidence = wait(ready, process)
                if app == 'tauri':
                    validate(evidence, state, motion)
                    (directory / 'proof.json').write_text(json.dumps(evidence, indent=2))
                yield root, baseline, tracked
            finally:
                if process.poll() is None:
                    for child in tree(root, baseline):
                        tracked[identity(child)] = child
                # Only launch-owned identities, including newly spawned WebKit XPC helpers.
                for key, child in reversed(list(tracked.items())):
                    try:
                        if identity(child) == key:
                            child.terminate()
                    except psutil.NoSuchProcess:
                        pass
                _, alive = psutil.wait_procs(list(tracked.values()), timeout=5)
                for child in alive:
                    try:
                        child.kill()
                    except psutil.NoSuchProcess:
                        pass
                psutil.wait_procs(alive, timeout=5)
                process.wait(timeout=5)


def screenshot(root, destination):
    from PIL import Image, ImageGrab
    if sys.platform == 'win32':
        import ctypes
        from ctypes import wintypes
        user = ctypes.windll.user32
        rectangles = []
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            pid = wintypes.DWORD()
            user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == root.pid and user.IsWindowVisible(hwnd):
                rect = wintypes.RECT()
                point = wintypes.POINT()
                user.GetClientRect(hwnd, ctypes.byref(rect))
                user.ClientToScreen(hwnd, ctypes.byref(point))
                if rect.right >= 1280 and rect.bottom >= 720:
                    rectangles.append((point.x, point.y, point.x + rect.right, point.y + rect.bottom))
            return True
        user.EnumWindows(callback_type(visit), 0)
        assert len(rectangles) == 1, rectangles
        ImageGrab.grab(bbox=rectangles[0], all_screens=True).save(destination)
    elif sys.platform == 'darwin':
        import Quartz
        windows = [w for w in Quartz.CGWindowListCopyWindowInfo(
            Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID)
            if w.get('kCGWindowOwnerPID') == root.pid and w.get('kCGWindowLayer') == 0]
        assert len(windows) == 1, windows
        subprocess.run(['screencapture', '-x', '-o', '-l', str(windows[0]['kCGWindowNumber']), str(destination)],
                       check=True, timeout=15)
    else:
        window = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--pid', str(root.pid)], text=True).splitlines()[0]
        geometry = dict(line.split('=', 1) for line in subprocess.check_output(
            ['xdotool', 'getwindowgeometry', '--shell', window], text=True).splitlines())
        x, y, w, h = (int(geometry[k]) for k in ('X', 'Y', 'WIDTH', 'HEIGHT'))
        ImageGrab.grab(bbox=(x, y, x+w, y+h)).save(destination)
    with Image.open(destination) as image:
        assert image.size == (1280, 720), image.size


def benchmark(root, baseline, tracked, duration):
    before = sample(root, baseline)
    start = last = time.monotonic()
    samples = []
    while time.monotonic() - start < duration:
        time.sleep(min(1, max(0, duration - (time.monotonic() - start))))
        after = sample(root, baseline)
        now = time.monotonic()
        after['elapsed_s'] = now - start
        after['cpu_pct'] = cpu_delta(before, after) / (now - last) * 100
        samples.append(after)
        for p in tree(root, baseline):
            tracked[identity(p)] = p
        before, last = after, now
    return dict(memory_bytes=statistics.mean(s['memory_bytes'] for s in samples),
                cpu_pct=statistics.mean(s['cpu_pct'] for s in samples), samples=samples)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--tauri', type=Path, required=True)
    parser.add_argument('--slint', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--mode', choices=['capture', 'bench'], required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise RuntimeError('Runner-only harness: refuses access to a local desktop')
    args.out.mkdir(parents=True, exist_ok=True)
    if sys.platform == 'win32':
        import ctypes
        ctypes.windll.user32.SetProcessDPIAware()
    if args.mode == 'capture':
        failures = []
        for state in STATES:
            directory = args.out / state
            try:
                with launch('tauri', state, False, args, directory) as (root, _, __):
                    screenshot(root, directory / 'capture.png')
            except Exception as error:
                failures.append(dict(state=state, error=str(error)))
        (args.out / 'failures.json').write_text(json.dumps(failures, indent=2))
        if failures:
            raise RuntimeError(failures)
    else:
        result = dict(platform=platform.platform(), sha=os.environ.get('GITHUB_SHA'),
                      warmup_s=5, duration_s=30, repeats=3, cpu='sum % of one core',
                      mac_attribution='descendants + WebKit XPC identities absent before app launch; isolated sequential spawn window',
                      limits='macOS RSS fallback, not phys_footprint; exited helpers CPU between samples can be missed; no GPU memory',
                      renderers=dict(slint='winit-software; skin wgpu adapter in app.log',
                                     tauri='WebView2/WKWebView/WebKitGTK; WebGL renderer in skin proof.json'), runs=[])
        for state in ('main', 'settings-skin'):
            for repeat in range(3):
                for app in ('tauri', 'slint'):
                    directory = args.out / state / str(repeat) / app
                    with launch(app, state, True, args, directory) as (root, baseline, tracked):
                        time.sleep(5)
                        run = benchmark(root, baseline, tracked, 30)
                    result['runs'].append(dict(app=app, state=state, repeat=repeat, **run))
                    (args.out / 'result.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
