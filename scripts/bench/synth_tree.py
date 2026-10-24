#!/usr/bin/env python3
"""Synthetic process tree for bench smoke tests: parent + 2 children.

Child A allocates ~20 MiB and sleeps; child B prints READY then exits early
(to exercise appearing/disappearing processes). Parent prints READY and sleeps.
Usage: synth_tree.py <sleep_seconds>
"""

import subprocess
import sys
import time

ALLOC = """import time
import sys
blob = bytearray(20 * 1024 * 1024)
print("READY-" + sys.argv[1], flush=True)
time.sleep(float(sys.argv[2]))
"""

EXIT_EARLY = """import time
print("READY-quick", flush=True)
time.sleep(0.3)
"""

LEAF_LIFE = 30.0


def main():
    life = float(sys.argv[1]) if len(sys.argv) > 1 else LEAF_LIFE
    kids = [
        subprocess.Popen([sys.executable, "-c", ALLOC, "slow", str(life)]),
        subprocess.Popen([sys.executable, "-c", EXIT_EARLY]),
    ]
    print("READY-parent", flush=True)
    deadline = time.monotonic() + life
    try:
        while time.monotonic() < deadline and any(k.poll() is None for k in kids):
            time.sleep(0.1)
    finally:
        for kid in kids:
            if kid.poll() is None:
                kid.terminate()
    print("DONE-parent", flush=True)


if __name__ == "__main__":
    main()
