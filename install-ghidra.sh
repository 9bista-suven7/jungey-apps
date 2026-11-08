#!/usr/bin/env bash
# Ghidra — the NSA's free reverse-engineering suite (GUI decompiler). Section 9, alongside
# Radare2.   Run AFTER security-tools.sh has finished:   sudo bash install-ghidra.sh
#
# Downloads ~570 MB from the official NSA GitHub release and unpacks to /opt/ghidra.
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Run with sudo:  sudo bash $0"
  exit 1
fi
USER_NAME="${SUDO_USER:-$(logname 2>/dev/null || echo root)}"

VERSION="12.2"
ZIP="ghidra_12.2_PUBLIC_20261016.zip"
URL="https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_12.2_build/$ZIP"
DEST="/opt/ghidra"

echo "==> Java (OpenJDK 21) and unzip"
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y default-jdk unzip curl

if [ -x "$DEST/ghidraRun" ]; then
  echo "==> Ghidra already installed at $DEST"
else
  echo "==> Downloading Ghidra $VERSION (~570 MB) from github.com/NationalSecurityAgency"
  curl -fL -C - "$URL" -o /tmp/ghidra.zip
  echo "==> Unpacking to $DEST"
  rm -rf /opt/ghidra_* "$DEST"
  unzip -q /tmp/ghidra.zip -d /opt
  mv /opt/ghidra_${VERSION}_PUBLIC "$DEST"
  chown -R "$USER_NAME":"$USER_NAME" "$DEST"
  rm -f /tmp/ghidra.zip
fi

echo "==> Launcher and menu entry"
cat > /usr/local/bin/ghidra <<EOF
#!/usr/bin/env bash
exec "$DEST/ghidraRun" "\$@"
EOF
chmod +x /usr/local/bin/ghidra

cat > /usr/share/applications/ghidra.desktop <<EOF
[Desktop Entry]
Type=Application
Name=Ghidra
GenericName=Reverse Engineering
Comment=Decompile and analyse binaries
Exec=/usr/local/bin/ghidra
Icon=applications-engineering
Terminal=false
Categories=Development;Security;
Keywords=reverse;decompile;disassemble;ghidra;
EOF

echo
echo "Done. Launch from the app menu (Ghidra) or run:  ghidra"
echo "First start takes a minute while it unpacks its modules."
