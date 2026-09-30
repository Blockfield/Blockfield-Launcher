set quiet
set dotenv-load := false
set positional-arguments
set default-script
set script-interpreter := ["python", "-I", "-X", "utf8"]

default:
    import subprocess
    raise SystemExit(subprocess.call(["just", "--list"]))

setup *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "setup", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

lint *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "lint", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

format-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "format-check", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

format *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "format", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

test *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "test", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "check", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

compile-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "compile", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

build *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "build", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

dev *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "dev", *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

python *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'python', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

tauri *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'pnpm', 'run', 'tauri', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

tauri-dev *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'pnpm', 'run', 'tauri:dev', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

version-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'pnpm', 'run', 'version:check', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

config-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'pnpm', 'run', 'config:check', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

config-test *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'pnpm', 'run', 'config:test', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

audit *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'pnpm', 'audit', '--audit-level', 'high', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

cargo-audit *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'cargo', 'audit', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")
