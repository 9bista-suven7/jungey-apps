#!/usr/bin/env bash
# One best-in-class security tool per section. Standard tools for authorized testing,
# CTF and learning on your own machine.   Run:  sudo bash security-tools.sh
#
# Sources: Ubuntu repos (most), pipx (Frida), Rapid7's signed apt repo (Metasploit),
# and Maltego's official .deb. The two big downloads are ~450 MB (Metasploit) and
# ~214 MB (Maltego); everything else is small.
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Run with sudo:  sudo bash $0"
  exit 1
fi
USER_NAME="${SUDO_USER:-$(logname 2>/dev/null || echo root)}"

echo "==> Updating package lists"
apt-get update -qq

# Wireshark asks whether non-root users may capture packets: answer yes up front.
echo "wireshark-common wireshark-common/install-setuid boolean true" | debconf-set-selections

APT_TOOLS=(
  nmap            # 1  information gathering
  nikto           # 2  vulnerability analysis
  sqlmap          # 3  web application testing
  john            # 4  password / authentication auditing
  aircrack-ng     # 5  wireless security
  wireshark       # 6  network analysis
  tshark          #    Wireshark on the command line
  sleuthkit       # 8  digital forensics
  radare2         # 9  reverse engineering
  mitmproxy       # 11 sniffing / spoofing
)
echo "==> Installing from Ubuntu repos: ${APT_TOOLS[*]}"
DEBIAN_FRONTEND=noninteractive apt-get install -y "${APT_TOOLS[@]}"

# Let your user capture packets in Wireshark without root.
if getent group wireshark >/dev/null && [ "$USER_NAME" != root ]; then
  usermod -aG wireshark "$USER_NAME"
  echo "==> Added $USER_NAME to the 'wireshark' group (log out and back in for it to take effect)."
fi

# 12  Frida (Android) isn't packaged for Ubuntu; install it for your user with pipx.
echo "==> Installing Frida via pipx (for $USER_NAME)"
apt-get install -y pipx
sudo -u "$USER_NAME" -H bash -lc 'pipx install frida-tools >/dev/null && pipx ensurepath >/dev/null' || \
  echo "   (Frida hiccupped — rerun as your user:  pipx install frida-tools )"

# 7  Metasploit — official Rapid7 signed apt repo + package (~450 MB).
echo "==> Installing Metasploit Framework (Rapid7 official installer)"
if ! command -v msfconsole >/dev/null; then
  if curl -fsSL https://raw.githubusercontent.com/rapid7/metasploit-omnibus/master/config/templates/metasploit-framework-wrappers/msfupdate.erb -o /tmp/msfinstall; then
    chmod +x /tmp/msfinstall && /tmp/msfinstall || echo "   (Metasploit install failed — you can rerun: sudo /tmp/msfinstall )"
  else
    echo "   (Couldn't fetch the Metasploit installer; skipping.)"
  fi
else
  echo "   Already installed."
fi

# 10  Maltego — official .deb (~214 MB). Needs a free account (create it in-app) to activate.
echo "==> Installing Maltego (official .deb)"
if ! command -v maltego >/dev/null && [ ! -d /opt/maltego ]; then
  if curl -fSL https://downloads.maltego.com/maltego-v4/linux/Maltego.v4.9.0.deb -o /tmp/maltego.deb; then
    apt-get install -y /tmp/maltego.deb || echo "   (Maltego install failed — try: sudo apt-get install -f )"
  else
    echo "   (Couldn't fetch Maltego; grab it from https://www.maltego.com/downloads/ )"
  fi
else
  echo "   Already installed."
fi

echo
echo "================================================================"
echo "Done. Checking what's now installed:"
for t in nmap nikto sqlmap john aircrack-ng tshark mmls r2 mitmproxy msfconsole maltego; do
  if command -v "$t" >/dev/null; then printf '  [ok]  %s\n' "$t"; else printf '  [--]  %s (not found)\n' "$t"; fi
done
echo "  Frida: ~/.local/bin/frida  (open a NEW terminal first)"
echo
echo "First-time notes:"
echo "  * Metasploit: run  msfconsole  once; it sets up its database."
echo "  * Maltego: launch it, then create a free account in the app to activate."
echo "  * Wireshark: log out and back in so non-root capture works."
