#!/usr/bin/env bash
# Everything the Jungey setup needs your password for, in one go:
#   sudo bash setup.sh
#
#   1. apps     DBeaver from its official .deb; remove duplicate apps; keep snapd out
#   2. look     Breeze cursor; battery indicator; touchpad gestures; the Jungey theme for every account; remove the 54 Orchis variants
#   3. effects  build picom v13: blur, rounded corners, soft shadows, open/close animations
#   4. bar      build the docklike plugin: one icon per app, live window previews on hover
#   5. apps     GTK4 / libadwaita / WebKit libraries for building the Jungey apps
#
# Builds run as you (in ~/.cache/jungey-build); only installing runs as root.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo:  sudo bash $0"
  exit 1
fi
USER_NAME=${SUDO_USER:?run this with sudo from your own account}
USER_HOME=$(getent passwd "$USER_NAME" | cut -d: -f6)
BUILD=$USER_HOME/.cache/jungey-build
as_user() { sudo -u "$USER_NAME" -H "$@"; }
step() { printf '\n\033[1;33m== %s\033[0m\n' "$*"; }
trap 'echo; echo "Stopped at: $BASH_COMMAND"; echo "Nothing after this step ran. Send the output above to Claude."' ERR

step "1/6 Packages"
apt-get update -q
apt-get install -y -q \
  build-essential pkg-config git curl meson ninja-build cmake autoconf automake libtool gettext intltool \
  libgtk-4-dev libadwaita-1-dev libpoppler-glib-dev libwebkitgtk-6.0-dev \
  libgtk-4-media-gstreamer gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav \
  breeze-cursor-theme xfce4-power-manager-plugins \
  xfce4-dev-tools libxfce4panel-2.0-dev libxfce4ui-2-dev libxfce4util-dev libwnck-3-dev \
  libconfig-dev libdbus-1-dev libegl-dev libev-dev libgl-dev libepoxy-dev libpcre2-dev libpixman-1-dev \
  libx11-xcb-dev libxcb1-dev libxcb-composite0-dev libxcb-damage0-dev libxcb-glx0-dev libxcb-image0-dev \
  libxcb-present-dev libxcb-randr0-dev libxcb-render0-dev libxcb-render-util0-dev libxcb-shape0-dev \
  libxcb-util-dev libxcb-xfixes0-dev uthash-dev


# touchpad gestures: Ubuntu only packages the old Touchegg 1.x (no daemon, no pinch);
# Touchegg 2 comes from the project's official release (.deb, about 130 KB)
gestures() {
  local v
  v=$(dpkg-query -W -f='${Version}' touchegg 2>/dev/null || true)
  if [[ "$v" != 2.* ]]; then
    curl -fL --retry 2 -o /tmp/touchegg.deb \
      https://github.com/JoseExposito/touchegg/releases/download/2.0.18/touchegg_2.0.18_amd64.deb
    apt-get install -y /tmp/touchegg.deb
    rm -f /tmp/touchegg.deb
  fi
  systemctl enable --now touchegg.service
}
gestures || echo "(Touchpad gestures skipped: tell Claude.)"

step "2/6 DBeaver (official .deb; your settings are already in ~/.local/share/DBeaverData)"
if ! dpkg -s dbeaver-ce >/dev/null 2>&1; then
  curl -fL --retry 2 -o /tmp/dbeaver-ce.deb https://dbeaver.io/files/dbeaver-ce_latest_amd64.deb
  apt-get install -y /tmp/dbeaver-ce.deb
  rm -f /tmp/dbeaver-ce.deb
fi

step "3/6 Removing duplicates and clutter"
# GNOME Web -> Jungey Browser, Mousepad -> Jungey Notepad, Geany -> Jungey Code,
# Screenshot -> Jungey Snip (Print keys already moved), and the 54 Orchis theme variants
# (Jungey keeps the one it was built from). Shared libraries stay: the Jungey apps use them.
apt-get purge -y epiphany-browser mousepad geany geany-common xfce4-screenshooter orchis-gtk-theme 2>/dev/null || true
apt-get purge -y snapd >/dev/null 2>&1 || true
cat > /etc/apt/preferences.d/no-snapd.pref <<'EOF'
# snapd was removed on purpose; don't let other packages reinstall it.
Package: snapd
Pin: release a=*
Pin-Priority: -10
EOF

step "4/6 Jungey theme for every account (login screen and dialogs too)"
rm -rf /usr/share/themes/Jungey
cp -a "$USER_HOME/.local/share/themes/Jungey" /usr/share/themes/Jungey

step "5/6 picom v13 (blur, rounded corners, shadows, animations)"
as_user mkdir -p "$BUILD"
[ -d "$BUILD/picom" ] || as_user git clone -q --depth 1 --branch v13 https://github.com/yshui/picom "$BUILD/picom"
as_user bash -c "cd '$BUILD/picom' && rm -rf build && meson setup --buildtype=release -Dwith_docs=false build && ninja -C build"
ninja -C "$BUILD/picom/build" install

step "6/6 docklike panel plugin (one icon per app, window previews)"
[ -d "$BUILD/docklike" ] || as_user git clone -q --depth 1 --branch xfce4-docklike-plugin-0.4.2 \
  https://gitlab.xfce.org/panel-plugins/xfce4-docklike-plugin.git "$BUILD/docklike"
as_user bash -c "cd '$BUILD/docklike' && ./autogen.sh --prefix=/usr --libdir=/usr/lib/x86_64-linux-gnu && make -j\$(nproc)"
make -C "$BUILD/docklike" install

trap - ERR
printf '\n\033[1;32mAll done.\033[0m Tell Claude "done" to switch on the effects and the new taskbar.\n'

# Undo, if ever needed:
#   sudo apt install epiphany-browser mousepad geany xfce4-screenshooter orchis-gtk-theme
#   sudo rm /etc/apt/preferences.d/no-snapd.pref /usr/local/bin/picom
#   sudo make -C ~/.cache/jungey-build/docklike uninstall
