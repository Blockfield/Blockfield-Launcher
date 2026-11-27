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

format-staged *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "format-staged", *sys.argv[1:]]
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
    sys.argv = ["scripts/quality.py", "run", 'cargo', 'build', '--manifest-path', 'native/app/Cargo.toml', '--locked', '--release', '--bin', 'blockfield-native', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

dev *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'cargo', 'run', '--manifest-path', 'native/app/Cargo.toml', '--bin', 'blockfield-native', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

python *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'python', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

version-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'python', 'scripts/check-version.py', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

rollback *args:
    import runpy, sys
    sys.argv = ["scripts/rollback-release.py", *sys.argv[1:]]
    runpy.run_path("scripts/rollback-release.py", run_name="__main__")

config-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'python', 'scripts/validate-production-config.py', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

config-test *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'python', '-m', 'unittest', 'discover', '-s', 'tests', '-p', 'test_*.py', '-v', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

legacy-check *args:
    import runpy, sys
    sys.argv = ["scripts/quality.py", "run", 'python', 'scripts/check-legacy-cleanup.py', *sys.argv[1:]]
    runpy.run_path("scripts/quality.py", run_name="__main__")

cargo-audit *args:
    import subprocess, sys
    audit_args = list(sys.argv[1:])
    workspace = subprocess.run([sys.executable, "-I", "-X", "utf8", "scripts/quality.py", "run", "cargo", "audit", *audit_args]).returncode
    native = subprocess.run([sys.executable, "-I", "-X", "utf8", "scripts/quality.py", "run", "cargo", "audit", "--file", "native/app/Cargo.lock", *audit_args]).returncode
    raise SystemExit(workspace or native)
