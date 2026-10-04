#!/usr/bin/env bash
# Run a cargo command natively when the Tauri Linux deps are present, otherwise inside the
# `blockfield-tauri-builder` podman image (immutable distros like Bluefin have no gtk -dev packages).
# Usage: scripts/rust-env.sh cargo clippy --workspace -- -D warnings
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE=blockfield-tauri-builder

if [ "$(uname -s)" != Linux ] ||
    { pkg-config --exists gdk-3.0 webkit2gtk-4.1 2>/dev/null && command -v "${CC:-cc}" >/dev/null; }; then
    exec "$@"
fi
if ! command -v podman >/dev/null; then
    echo "Neither a native Tauri build environment nor podman found: $*" >&2
    exit 1
fi
if ! podman image exists "$IMAGE"; then
    echo "Building $IMAGE (one-time, a few minutes)..." >&2
    podman build -q -t "$IMAGE" -f "$ROOT/scripts/tauri-builder.Containerfile" "$ROOT/scripts"
fi
mkdir -p "$HOME/.cargo/registry" "${BLOCKFIELD_CARGO_TARGET:=$HOME/.cache/bf-cargo-target}"
TTY=()
[ -t 0 ] && TTY=(-it)
exec podman run --rm "${TTY[@]}" --tmpfs /tmp:rw,size=4g \
    -v "$ROOT:/work:z" \
    -v "$HOME/.cargo/registry:/usr/local/cargo/registry:z" \
    -v "$BLOCKFIELD_CARGO_TARGET:/work/target:z" \
    -w /work -e CARGO_TERM_COLOR=always \
    -e "BLOCKFIELD_DISCORD_APPLICATION_ID=${BLOCKFIELD_DISCORD_APPLICATION_ID:-}" \
    -e "VITE_BLOCKFIELD_PACK_URL=${VITE_BLOCKFIELD_PACK_URL:-}" \
    "$IMAGE" "$@"
