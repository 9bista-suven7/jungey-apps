#!/usr/bin/env python3
"""Switch the desktop effects over to picom (built by setup.sh).

    python3 look/effects.py          turn on: picom replaces Xfce's own compositor
    python3 look/effects.py --off    back to Xfce's compositor

picom starts with every login from ~/.config/autostart/jungey-effects.desktop.
"""
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HOME = Path.home()
CONF = HOME / ".config/picom/picom.conf"
AUTOSTART = HOME / ".config/autostart/jungey-effects.desktop"


def xfwm_compositing(on):
    subprocess.run(["xfconf-query", "-c", "xfwm4", "-p", "/general/use_compositing", "-s", str(on).lower()], check=True)


def on():
    picom = shutil.which("picom")
    if not picom:
        sys.exit("picom isn't installed yet: run  sudo bash setup.sh  first.")
    CONF.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(Path(__file__).parent / "picom.conf", CONF)
    check = subprocess.run([picom, "--config", str(CONF), "--diagnostics"], capture_output=True, text=True)
    if check.returncode != 0:
        sys.exit("picom doesn't accept the config:\n" + check.stderr[-2000:])

    xfwm_compositing(False)
    subprocess.run(["pkill", "-x", "picom"], check=False)
    time.sleep(0.5)
    subprocess.run(["sh", "-c", f"setsid -f {picom} --config {CONF} >/dev/null 2>&1 </dev/null"], check=True)
    AUTOSTART.parent.mkdir(parents=True, exist_ok=True)
    AUTOSTART.write_text(
        "[Desktop Entry]\nType=Application\nName=Jungey Effects\n"
        "Comment=Blur, rounded corners, shadows and animations (picom)\n"
        f"Exec={picom} --config {CONF}\nOnlyShowIn=XFCE;\nX-GNOME-Autostart-enabled=true\n"
    )
    time.sleep(1.5)
    running = subprocess.run(["pgrep", "-x", "picom"], capture_output=True).returncode == 0
    if not running:
        xfwm_compositing(True)
        sys.exit("picom stopped right away; switched back to Xfce's compositor.")
    gestures()
    print("Effects on: blur, rounded corners, shadows and animations; touchpad gestures ready.")


def gestures():
    """3 fingers left/right switch apps, up opens the menu, down shows the desktop; 4 fingers switch tabs."""
    conf = HOME / ".config/touchegg/touchegg.conf"
    conf.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(Path(__file__).parent / "touchegg.conf", conf)
    if shutil.which("touchegg"):
        subprocess.run(["pkill", "-x", "-u", str(os.getuid()), "touchegg"], check=False)  # your session client, not the daemon
        subprocess.run(["sh", "-c", "setsid -f touchegg >/dev/null 2>&1 </dev/null"], check=False)


def off():
    subprocess.run(["pkill", "-x", "picom"], check=False)
    AUTOSTART.unlink(missing_ok=True)
    xfwm_compositing(True)
    print("Effects off: back to Xfce's own compositor.")


if __name__ == "__main__":
    off() if "--off" in sys.argv else on()
