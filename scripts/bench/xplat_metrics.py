"""Runner-only process accounting. Requires psutil; raw process identities are retained."""

import sys


def identity(process):
    return process.pid, process.create_time()


def webkit(process):
    return process.name().startswith("com.apple.WebKit.")


def tree(root, baseline):
    import psutil

    processes = {root.pid: root}
    try:
        for process in root.children(recursive=True):
            processes[process.pid] = process
    except psutil.NoSuchProcess:
        pass
    if sys.platform == "darwin":
        # shortcut: launch-window matching can include unrelated WebKit apps; use responsible PID if runners run those concurrently.
        for process in psutil.process_iter():
            try:
                if identity(process) not in baseline and webkit(process):
                    processes[process.pid] = process
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                continue
    return list(processes.values())


def sample(root, baseline):
    import psutil

    if not root.is_running() or root.status() == psutil.STATUS_ZOMBIE:
        raise RuntimeError("App exited during benchmark")
    rows = []
    for process in tree(root, baseline):
        try:
            cpu = process.cpu_times()
            if sys.platform == "win32":
                # psutil USS uses QueryWorkingSet: resident private pages, not commit size.
                memory = process.memory_full_info().uss
                metric = "private_working_set"
            elif sys.platform == "darwin":
                # shortcut: RSS can double-count shared pages; use phys_footprint when a reliable runner probe is available.
                memory = process.memory_info().rss
                metric = "rss_fallback"
            else:
                memory = process.memory_full_info().pss
                metric = "pss"
            rows.append(
                dict(
                    pid=process.pid,
                    born=process.create_time(),
                    name=process.name(),
                    memory_bytes=memory,
                    cpu_s=cpu.user + cpu.system,
                )
            )
        except psutil.NoSuchProcess:
            continue
    if not rows:
        raise RuntimeError("Empty process tree")
    return dict(
        metric=metric, memory_bytes=sum(p["memory_bytes"] for p in rows), processes=rows
    )


def cpu_delta(before, after):
    previous = {(p["pid"], p["born"]): p["cpu_s"] for p in before["processes"]}
    return sum(
        max(0, p["cpu_s"] - previous.get((p["pid"], p["born"]), 0))
        for p in after["processes"]
    )
