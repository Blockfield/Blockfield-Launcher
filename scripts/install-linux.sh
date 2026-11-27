#!/usr/bin/env bash
# Build the native Slint launcher (release) and install it for the current user:
# ~/.local/lib/blockfield-launcher, a .desktop entry and an icon. No root, no system packages.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export VITE_BLOCKFIELD_PACK_URL="${VITE_BLOCKFIELD_PACK_URL:-https://blockfield.pro}"
export VITE_BLOCKFIELD_WORKSHOP_PACK_URL="${VITE_BLOCKFIELD_WORKSHOP_PACK_URL:-https://blockfield.pro/workshop-pack}"

if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != "--native" ]; }; then
    echo "Usage: $0 [--native]" >&2
    exit 2
fi

APP="$HOME/.local/lib/blockfield-launcher"
mkdir -p "$APP" "$HOME/.local/share/applications"

cd "$ROOT"
NATIVE_TARGET="${CARGO_TARGET_DIR:-$ROOT/native/app/target}"
cargo build --manifest-path "$ROOT/native/app/Cargo.toml" --locked --release --bin blockfield-native
built_binary="$NATIVE_TARGET/release/blockfield-native"
installed_name="blockfield-launcher"
desktop_file="$HOME/.local/share/applications/blockfield-launcher.desktop"

if [ -f "$APP/$installed_name" ]; then
    cp -p "$APP/$installed_name" "$APP/$installed_name.previous"
fi
staged_binary=$(mktemp "$APP/.blockfield-install.XXXXXX")
trap 'rm -f "$staged_binary"' EXIT
install -m 755 "$built_binary" "$staged_binary"
mv -f "$staged_binary" "$APP/$installed_name"
printf '1\n' >"$APP/.blockfield-native-install"
install -m 644 native/packaging/assets/256x256.png "$APP/icon.png"

discord_id="${BLOCKFIELD_DISCORD_APPLICATION_ID:-1548300877125779456}"
cat >"$desktop_file" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Blockfield Launcher
Comment=Blockfield game launcher
Exec="$APP/$installed_name" %U
Icon=$APP/icon.png
Terminal=false
Categories=Game;
StartupWMClass=blockfield-native
MimeType=x-scheme-handler/blockfield;x-scheme-handler/discord-$discord_id;
DESKTOP
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "Installed: $APP/$installed_name (menu entry 'Blockfield Launcher')"
