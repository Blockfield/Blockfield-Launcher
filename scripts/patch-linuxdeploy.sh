#!/usr/bin/env bash
set -euo pipefail

mkdir -p ~/.cache/tauri
curl -sSL -o ~/.cache/tauri/linuxdeploy-plugin-gtk.sh https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh
APP_VER=$(node -p "require('./package.json').version")
cat <<EOF >>~/.cache/tauri/linuxdeploy-plugin-gtk.sh

# Custom patch: exclude bundled Wayland client libraries so host Mesa/Wayland graphics stack is used instead.
# This prevents "Could not create default EGL display: EGL_BAD_PARAMETER" white screen crash on modern Wayland environments.
rm -f "\$APPDIR"/usr/lib/libwayland*.so* "\$APPDIR"/usr/lib/*/libwayland*.so* || true

# Custom patch: inject X-AppImage-Version into .desktop files so Gear Lever and AppImage integrators display version
for d in "\$APPDIR"/*.desktop "\$APPDIR"/usr/share/applications/*.desktop; do
  if [ -f "\$d" ] && ! grep -q "X-AppImage-Version" "\$d"; then
    echo "X-AppImage-Version=$APP_VER" >> "\$d"
  fi
done

# Custom patch: undo the plugin's GDK_BACKEND=x11 in the hook that ships inside the AppImage.
# x11 was a workaround for the Wayland backend crashing on the bundled libwayland, which we
# no longer ship (see patch above). Under XWayland mutter scales the window twice (1280x720
# arrives as 2560x1514) and claims a 74px decoration frame the window never asked for, so
# clicks land next to the UI instead of on it.
# Fail the build rather than ship a silent no-op if the plugin moves its hook.
gtk_hook="\$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh"
[ -f "\$gtk_hook" ] || { echo "GTK AppRun hook missing at \$gtk_hook" >&2; exit 1; }
echo 'unset GDK_BACKEND' >> "\$gtk_hook"
EOF
chmod +x ~/.cache/tauri/linuxdeploy-plugin-gtk.sh
