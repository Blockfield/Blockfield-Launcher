#!/usr/bin/env bash
# Run a cargo command when a C compiler is available on the host.
# Usage: scripts/rust-env.sh cargo clippy --workspace -- -D warnings
set -euo pipefail

if [ "$(uname -s)" != Linux ] || command -v "${CC:-cc}" >/dev/null; then
    exec "$@"
fi
echo "Missing C compiler (${CC:-cc}) for native Rust build: $*" >&2
exit 1
