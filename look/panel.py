#!/usr/bin/env python3
"""Lay out the Jungey desktop: one floating bar, centred at the bottom, always visible.

    python3 panel.py                 apply
    python3 panel.py --restore DIR   put back a backup (backup/before-dusk-*)

    [J] | Files Browser Terminal Code Claude Jungey-AI Snip  (open apps) | tray sound alerts battery  (dial) time/date  power

With the docklike plugin installed (setup.sh does it), pinned and open apps are
merged: one icon per app, with a dot per open window. Without it the bar shows
open apps only, so nothing appears twice.
"""
import shutil
import subprocess
import sys
from pathlib import Path

import palette as P

CH = "xfce4-panel"
HOME = Path.home()
APPS = HOME / ".local/share/applications"
PLUGINS = Path("/usr/lib/x86_64-linux-gnu/xfce4/panel/plugins")
DOCKLIKE = PLUGINS / "libdocklike.so"
BATTERY_PLUGIN = PLUGINS / "libxfce4powermanager.so"

# plugin ids already on this machine
MENU, CLOCK, TRAY, SOUND, ALERTS, POWER = 1, 19, 15, 16, 18, 20
FILES, BROWSER, TERMINAL, CODE, CLAUDE, JUNGEY, SNIP = 4, 5, 3, 6, 14, 7, 21
DIVIDER, RUNNING = 8, 11
DIV_A, DIV_B, DOCK, DIAL, BATTERY, KEY, CENTER = 23, 24, 25, 26, 27, 28, 29
LAUNCHPAD = HOME / ".local/bin/jungey-launchpad"
JUNGEY_CENTER = HOME / ".local/bin/jungey-center"
RETIRED = [2, 12, 13, 22]  # old separators, spacers and the cpu/memory meter

PINNED = [  # for docklike, in order
    APPS / ("jungey-files.desktop" if (APPS / "jungey-files.desktop").exists() else "thunar.desktop"),
    APPS / "google-chrome.desktop",
    APPS / "xfce4-terminal.desktop",
    APPS / "com.microsoft.VSCode.desktop",
    Path("/usr/share/applications/com.anthropic.Claude.desktop"),
    APPS / "jungey.desktop",
    APPS / "jungey-snip.desktop",
]


def q(*args, check=True):
    return subprocess.run(["xfconf-query", "-c", CH, *args], check=check, capture_output=True, text=True)


def kind(v):
    return {bool: "bool", int: "int", float: "double", str: "string"}[type(v)]


def fmt(v):
    return str(v).lower() if isinstance(v, bool) else str(v)


def put(path, value):
    if isinstance(value, list):
        args = ["-p", path, "-n", "--force-array"]
        for v in value:
            args += ["-t", kind(v), "-s", fmt(v)]
    else:
        args = ["-p", path, "-n", "-t", kind(value), "-s", fmt(value)]
    q(*args)


def drop(path):
    q("-p", path, "-r", "-R", check=False)


def stop_panel():
    """The panel writes its own layout back when it quits, so stop it before changing anything."""
    subprocess.run(["xfce4-panel", "-q"], check=False)
    for _ in range(50):
        if subprocess.run(["pgrep", "-x", "xfce4-panel"], capture_output=True).returncode != 0:
            return
        subprocess.run(["sleep", "0.1"])


def start_panel():
    subprocess.run(["sh", "-c", "setsid -f xfce4-panel >/dev/null 2>&1 </dev/null"], check=False)


def glass(hex_color, alpha):
    r, g, b = P.rgb(hex_color)
    return [round(r / 255, 3), round(g / 255, 3), round(b / 255, 3), alpha]


def write_docklike():
    pinned = ";".join(str(p) for p in PINNED if p.exists())
    r, g, b = P.rgb(P.ACCENT)
    (HOME / f".config/xfce4/panel/docklike-{DOCK}.rc").write_text(
        "[user]\n"
        f"pinned={pinned}\n"
        "indicatorStyle=1\n"           # dots under the icon
        "indicatorOrientation=0\n"
        f"indicatorColor=rgb({r},{g},{b})\n"
        "inactiveIndicatorStyle=1\n"
        f"inactiveColor=rgba({r},{g},{b},0.45)\n"
        "showPreviews=true\n"
        "previewScale=0.22\n"
        "forceIconSize=true\n"
        "iconSize=30\n"
        "noWindowsListIfSingle=true\n"
        "onlyDisplayVisible=false\n"
        "onlyDisplayScreen=false\n"
    )


def launcher(pid, name, exec_, icon):
    """A bar button that runs one command."""
    d = HOME / f".config/xfce4/panel/launcher-{pid}"
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{name}.desktop").write_text(
        f"[Desktop Entry]\nType=Application\nName={name.replace('-', ' ').title()}\n"
        f"Exec={exec_}\nIcon={icon}\nTerminal=false\n")
    put(f"/plugins/plugin-{pid}", "launcher")
    put(f"/plugins/plugin-{pid}/items", [f"{name}.desktop"])


def apply():
    stop_panel()
    for pid in RETIRED:
        drop(f"/plugins/plugin-{pid}")
    drop("/panels/panel-2")

    for pid in (DIVIDER, DIV_A, DIV_B):
        put(f"/plugins/plugin-{pid}", "separator")
        put(f"/plugins/plugin-{pid}/style", 1)
        put(f"/plugins/plugin-{pid}/expand", False)

    # one power button: opens the dialog with shut down, restart, sleep and log out
    put(f"/plugins/plugin-{POWER}/appearance", 0)
    put(f"/plugins/plugin-{POWER}/items", ["+logout-dialog", "-lock-screen", "-switch-user", "-separator",
                                           "-suspend", "-hibernate", "-hybrid-sleep", "-restart", "-shutdown", "-logout"])
    put(f"/plugins/plugin-{MENU}/show-button-title", False)
    put(f"/plugins/plugin-{MENU}/button-icon", "jungey-os")
    # the Jungey key: Jungey Launchpad when it's installed, otherwise the Start menu
    menu = MENU
    if LAUNCHPAD.exists():
        launcher(KEY, "jungey-menu", f"{HOME}/.local/bin/jungey-menu", "jungey-os")
        menu = KEY

    # clock: a small analog face, then the time above the date
    put(f"/plugins/plugin-{DIAL}", "clock")
    put(f"/plugins/plugin-{DIAL}/mode", 0)
    put(f"/plugins/plugin-{DIAL}/show-seconds", False)
    put(f"/plugins/plugin-{CLOCK}/mode", 2)
    put(f"/plugins/plugin-{CLOCK}/digital-layout", 1)  # time above date
    put(f"/plugins/plugin-{CLOCK}/digital-time-format", "%H:%M")
    put(f"/plugins/plugin-{CLOCK}/digital-date-format", "%a %d %b")
    put(f"/plugins/plugin-{CLOCK}/digital-time-font", "Inter Semi-Bold 10")
    put(f"/plugins/plugin-{CLOCK}/digital-date-font", "Inter 8")

    # tray: compact icons, close together (20px so the battery reads clearly)
    put(f"/plugins/plugin-{TRAY}/icon-size", 20)
    put(f"/plugins/plugin-{TRAY}/square-icons", False)
    put(f"/plugins/plugin-{TRAY}/single-row", True)

    # battery: the power manager plugin if installed (icon + %), else its tray icon
    status = [TRAY, SOUND, ALERTS]
    if BATTERY_PLUGIN.exists():
        put(f"/plugins/plugin-{BATTERY}", "power-manager-plugin")
        put(f"/plugins/plugin-{BATTERY}/show-panel-label", 1)
        subprocess.run(["xfconf-query", "-c", "xfce4-power-manager", "-p", "/xfce4-power-manager/show-tray-icon",
                        "-n", "-t", "bool", "-s", "false"], check=False)
        status.append(BATTERY)
    else:
        drop(f"/plugins/plugin-{BATTERY}")
        subprocess.run(["xfconf-query", "-c", "xfce4-power-manager", "-p", "/xfce4-power-manager/show-tray-icon",
                        "-n", "-t", "bool", "-s", "true"], check=False)

    # Jungey Center: one button for Wi-Fi, Bluetooth, sound, Do Not Disturb and power,
    # so those leave the bar (their background helpers keep running, hidden from the tray)
    ends = [POWER]
    if JUNGEY_CENTER.exists():
        launcher(CENTER, "jungey-center", str(JUNGEY_CENTER), "jungey-center")
        status = [s for s in status if s not in (SOUND, ALERTS)] + [CENTER]
        ends = []
        put(f"/plugins/plugin-{TRAY}/hidden-items", ["blueman", "nm-applet"])
        hidden_legacy = ["network"] + (["xfce4-power-manager"] if BATTERY_PLUGIN.exists() else [])
        put(f"/plugins/plugin-{TRAY}/hidden-legacy-items", hidden_legacy)

    if DOCKLIKE.exists():
        put(f"/plugins/plugin-{DOCK}", "docklike")
        write_docklike()
        apps = [DOCK]
    else:
        put(f"/plugins/plugin-{RUNNING}/show-labels", False)
        put(f"/plugins/plugin-{RUNNING}/flat-buttons", True)
        put(f"/plugins/plugin-{RUNNING}/show-handle", False)
        put(f"/plugins/plugin-{RUNNING}/grouping", 1)
        put(f"/plugins/plugin-{RUNNING}/include-all-workspaces", True)
        # without docklike, pinned launchers and open windows would show the same app twice:
        # show open apps only (launch from the J menu) until docklike is installed
        apps = [RUNNING]

    bar = "/panels/panel-1"
    put(f"{bar}/position", "p=10;x=0;y=0")
    put(f"{bar}/position-locked", True)
    put(f"{bar}/mode", 0)
    put(f"{bar}/length", 1)
    put(f"{bar}/length-adjust", True)
    put(f"{bar}/size", 58)
    # app icons: docklike sizes its own (30), so the panel size only sets the small status icons
    put(f"{bar}/icon-size", 20 if DOCKLIKE.exists() else 32)  # 32+: the task list uses full-size app icons
    put(f"{bar}/autohide-behavior", 0)  # always there: open apps never disappear
    put(f"{bar}/background-style", 1)
    put(f"{bar}/background-rgba", glass(P.RAISED, 0.84))
    put(f"{bar}/plugin-ids", [menu, DIV_A, *apps, DIV_B, *status, DIAL, CLOCK, *ends])

    put("/panels", [1])
    put("/panels/dark-mode", True)
    start_panel()
    print("Jungey bar applied: one bar, centred at the bottom.")


def restore(backup):
    backup = Path(backup)
    stop_panel()
    subprocess.run(["pkill", "-x", "xfconfd"], check=False)  # it would write its cached copy back
    shutil.copy2(backup / "xfce4-panel.xml", HOME / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml")
    if (backup / "panel-dir").is_dir():
        shutil.rmtree(HOME / ".config/xfce4/panel", ignore_errors=True)
        shutil.copytree(backup / "panel-dir", HOME / ".config/xfce4/panel")
    start_panel()
    print(f"Panel restored from {backup}")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--restore":
        restore(sys.argv[2])
    else:
        apply()
