#!/usr/bin/env bash
# Second round of speed-ups for this laptop (hard drive + i7-1065G7).
#   sudo bash faster.sh
#
#   1. hard drive: the BFQ scheduler, so apps and the desktop get the disk first and
#      background work (Docker, updates, builds) no longer makes things stutter
#   2. processor: full speed while plugged in, balanced on battery
#   3. preload: learns which apps you use and keeps them ready in spare memory,
#      so they open faster from the hard drive
#   4. startup: skip e2scrub_reap, a 7-second boot step for LVM snapshots this laptop doesn't have
#
# Everything is undone by the lines at the bottom.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo:  sudo bash $0"
  exit 1
fi
step() { printf '\n\033[1;33m== %s\033[0m\n' "$*"; }

step "1/4 Hard drive: BFQ scheduler"
echo bfq > /etc/modules-load.d/jungey-bfq.conf
cat > /etc/udev/rules.d/60-jungey-ioscheduler.rules <<'EOF'
# Spinning disks: BFQ keeps the desktop responsive while something else reads or writes.
ACTION=="add|change", KERNEL=="sd[a-z]", ATTR{queue/rotational}=="1", ATTR{queue/scheduler}="bfq"
EOF
modprobe bfq
for d in /sys/block/sd?; do
  [ "$(cat "$d/queue/rotational")" = 1 ] && echo bfq > "$d/queue/scheduler"
done
echo "   now: $(cat /sys/block/sda/queue/scheduler)"

step "2/4 Processor: full speed when plugged in"
cat > /usr/local/bin/jungey-power-mode <<'EOF'
#!/bin/sh
# Full speed when plugged in, balanced on battery.
if [ "$(cat /sys/class/power_supply/AC/online 2>/dev/null)" = 1 ]; then epp=performance; else epp=balance_performance; fi
for f in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do
  echo "$epp" > "$f" 2>/dev/null || true
done
EOF
chmod 755 /usr/local/bin/jungey-power-mode
cat > /etc/systemd/system/jungey-power-mode.service <<'EOF'
[Unit]
Description=Jungey: full CPU speed on AC power, balanced on battery

[Service]
Type=oneshot
ExecStart=/usr/local/bin/jungey-power-mode

[Install]
WantedBy=multi-user.target
EOF
cat > /etc/udev/rules.d/61-jungey-power-mode.rules <<'EOF'
# Plugging in or unplugging the charger switches the CPU mode.
SUBSYSTEM=="power_supply", ATTR{type}=="Mains", RUN+="/usr/bin/systemctl --no-block start jungey-power-mode.service"
EOF
systemctl daemon-reload
udevadm control --reload
systemctl enable --now jungey-power-mode.service
echo "   now: $(cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference)"

step "3/4 preload: faster app launches"
apt-get install -y -q preload
systemctl enable --now preload

step "4/4 Startup: skip e2scrub_reap"
systemctl disable e2scrub_reap.service

printf '\n\033[1;32mAll done.\033[0m Restart once to get the faster startup.\n'

# Undo:
#   sudo rm /etc/udev/rules.d/60-jungey-ioscheduler.rules /etc/modules-load.d/jungey-bfq.conf
#   sudo systemctl disable --now jungey-power-mode.service && sudo rm /etc/udev/rules.d/61-jungey-power-mode.rules \
#        /etc/systemd/system/jungey-power-mode.service /usr/local/bin/jungey-power-mode
#   sudo apt purge preload
#   sudo systemctl enable e2scrub_reap.service
