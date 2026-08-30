#!/usr/bin/env bash
# Build the launcher (release, via scripts/rust-env.sh) and install it for the current user:
# ~/.local/lib/blockfield-launcher, a .desktop entry and an icon. No root, no system packages.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export VITE_BLOCKFIELD_PACK_URL="${VITE_BLOCKFIELD_PACK_URL:-https://modpack.dev.nether.pp.ua}"
TARGET="${BLOCKFIELD_CARGO_TARGET:-$HOME/.cache/bf-cargo-target}"

cd "$ROOT"
CI=true pnpm install --frozen-lockfile
pnpm build
# tauri/custom-protocol = serve the embedded frontend; without it a release binary still loads devUrl (localhost:5173)
scripts/rust-env.sh cargo build --release -p blockfield-launcher --features tauri/custom-protocol

APP="$HOME/.local/lib/blockfield-launcher"
mkdir -p "$APP" "$HOME/.local/share/applications"
install -m 755 "$TARGET/release/blockfield-launcher" "$APP/blockfield-launcher"
install -m 644 src-tauri/icons/256x256.png "$APP/icon.png"
cat > "$HOME/.local/share/applications/blockfield-launcher.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Blockfield Launcher
Comment=Blockfield modpack launcher
# WEBKIT_DISABLE_DMABUF_RENDERER: transparent Tauri windows render invisible with WebKitGTK's DMA-BUF path on some Wayland/GPU combos
Exec=env WEBKIT_DISABLE_DMABUF_RENDERER=1 $APP/blockfield-launcher
Icon=$APP/icon.png
Terminal=false
Categories=Game;
StartupWMClass=blockfield-launcher
DESKTOP
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "Installed: $APP/blockfield-launcher (menu entry 'Blockfield Launcher')"
