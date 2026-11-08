#!/usr/bin/env bash
# Build the Jungey apps and switch them in. No sudo needed (run setup.sh once first).
#   ./install.sh
set -euo pipefail
cd "$(dirname "$0")"
if ! pkg-config --exists gtk4 libadwaita-1 2>/dev/null; then
  echo "The GTK4 libraries aren't installed yet: run  sudo bash setup.sh  first."
  exit 1
fi
BIN=$HOME/.local/bin
APPS=$HOME/.local/share/applications

cargo build --release -p jungey-files -p jungey-launchpad -p jungey-center -p jungey-tasks

for app in jungey-files jungey-launchpad jungey-center jungey-tasks; do
  install -Dm755 "target/release/$app" "$BIN/$app"
  sed "s|@BIN@|$BIN|g" "$app/data/$app.desktop" > "$APPS/$app.desktop"
done
install -Dm755 bin/jungey-menu "$BIN/jungey-menu"

# Launchpad and Center: loaded at login so they open instantly
mkdir -p "$HOME/.config/autostart"
for app in jungey-launchpad jungey-center; do
  sed "s|@BIN@|$BIN|g" "$app/data/autostart.desktop" > "$HOME/.config/autostart/$app.desktop"
  pkill -x "$app" 2>/dev/null || true
  setsid -f "$BIN/$app" --background >/dev/null 2>&1 </dev/null
done

# Files: opens folders everywhere (desktop, downloads, other apps); Thunar leaves the menu
xdg-mime default jungey-files.desktop inode/directory
mkdir -p "$HOME/.local/share/xfce4/helpers"
cat > "$HOME/.local/share/xfce4/helpers/jungey-files.desktop" <<HELPER
[Desktop Entry]
NoDisplay=true
Version=1.0
Type=X-XFCE-Helper
X-XFCE-Category=FileManager
X-XFCE-Commands=$BIN/jungey-files
X-XFCE-CommandsWithParameter=$BIN/jungey-files "%s"
Icon=jungey-files
Name=Jungey Files
HELPER
touch "$HOME/.config/xfce4/helpers.rc"
sed -i '/^FileManager=/d' "$HOME/.config/xfce4/helpers.rc"
echo "FileManager=jungey-files" >> "$HOME/.config/xfce4/helpers.rc"
python3 - <<'PY'
from pathlib import Path
for name in ("thunar.desktop", "thunar-bulk-rename.desktop"):
    f = Path.home() / ".local/share/applications" / name
    if f.exists() and "NoDisplay=true" not in f.read_text():
        f.write_text(f.read_text().replace("[Desktop Entry]\n", "[Desktop Entry]\nNoDisplay=true\n", 1))
PY
update-desktop-database -q "$APPS" || true

# bar: the Jungey key opens the Launchpad, Files is pinned
python3 look/panel.py
echo "Jungey Files, Launchpad, Center and Tasks installed."
