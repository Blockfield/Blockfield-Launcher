"""Runner-only process accounting. Requires psutil; raw process identities are retained."""
import sys

import psutil


def identity(process):
    return process.pid, process.create_time()


def webkit(process):
    return 'com.apple.WebKit.' in process.name() or any(
        'com.apple.WebKit.' in arg for arg in process.cmdline()
    )


def tree(root, baseline):
    processes = {root.pid: root}
    for process in root.children(recursive=True):
        processes[process.pid] = process
    if sys.platform == 'darwin':
        # XPC WebKit services are launchd children, not descendants of the Tauri PID.
        for process in psutil.process_iter():
            try:
                if identity(process) not in baseline and webkit(process):
                    processes[process.pid] = process
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                continue
    return list(processes.values())


def sample(root, baseline):
    rows = []
    for process in tree(root, baseline):
        try:
            cpu = process.cpu_times()
            if sys.platform == 'win32':
                # psutil USS uses QueryWorkingSet: resident private pages, not commit size.
                memory = process.memory_full_info().uss
                metric = 'private_working_set'
            elif sys.platform == 'darwin':
                memory = process.memory_info().rss
                metric = 'rss_fallback'
            else:
                memory = process.memory_full_info().pss
                metric = 'pss'
            rows.append(dict(pid=process.pid, born=process.create_time(), name=process.name(),
                             memory_bytes=memory, cpu_s=cpu.user + cpu.system))
        except psutil.NoSuchProcess:
            continue
    if not rows:
        raise RuntimeError('Empty process tree')
    return dict(metric=metric, memory_bytes=sum(p['memory_bytes'] for p in rows), processes=rows)


def cpu_delta(before, after):
    previous = {(p['pid'], p['born']): p['cpu_s'] for p in before['processes']}
    return sum(max(0, p['cpu_s'] - previous.get((p['pid'], p['born']), 0))
               for p in after['processes'])
