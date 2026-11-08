#!/usr/bin/env python3
"""Build and switch on the whole Jungey look in one go.

    uv run --with pillow python3 look/apply.py

Theme, window buttons, icons, wallpaper, the bottom bar, Alt+Tab, terminal and
VS Code colours, all from palette.py. Safe to run again after changing the palette.
"""
import subprocess
import time
from pathlib import Path

import icons
import palette as P
import panel
import theme
import wallpaper

HOME = Path.home()


def xf(channel, prop, value):
    t = {bool: "bool", int: "int", str: "string"}[type(value)]
    v = str(value).lower() if isinstance(value, bool) else str(value)
    subprocess.run(["xfconf-query", "-c", channel, "-p", prop, "-n", "-t", t, "-s", v], check=True)


def main():
    theme.build()
    subprocess.run(["python3", str(Path(icons.__file__))], check=True)
    subprocess.run(["python3", str(Path(wallpaper.__file__))], check=True)

    # switch away and back so running apps reload theme, borders and icons
    xf("xsettings", "/Net/ThemeName", "Adwaita-dark")
    xf("xfwm4", "/general/theme", "Default")
    xf("xsettings", "/Net/IconThemeName", "Papirus-Dark")
    time.sleep(1)
    xf("xsettings", "/Net/ThemeName", "Jungey")
    xf("xfwm4", "/general/theme", "Jungey")
    xf("xsettings", "/Net/IconThemeName", "Jungey")
    if Path("/usr/share/icons/breeze_cursors").exists():  # dark arrow, white edge, like macOS
        xf("xsettings", "/Gtk/CursorThemeName", "breeze_cursors")
        xf("xsettings", "/Gtk/CursorThemeSize", 24)

    # macOS-style windows: buttons on the left, title in the middle, soft shadows
    xf("xsettings", "/Gtk/DecorationLayout", "close,minimize,maximize:")
    xf("xfwm4", "/general/button_layout", "CHM|")
    xf("xfwm4", "/general/title_alignment", "center")
    xf("xfwm4", "/general/title_font", "Inter Semi-Bold 10")
    xf("xfwm4", "/general/show_popup_shadow", True)
    xf("xfwm4", "/general/show_frame_shadow", True)

    # simple app switching: one desktop, Alt+Tab shows big previews of every window
    xf("xfwm4", "/general/workspace_count", 1)
    xf("xfwm4", "/general/cycle_preview", True)
    xf("xfwm4", "/general/cycle_tabwin_mode", 0)
    xf("xfwm4", "/general/cycle_minimum", True)
    xf("xfwm4", "/general/cycle_hidden", True)
    xf("xfwm4", "/general/cycle_draw_frame", False)
    xf("xfwm4", "/general/cycle_workspaces", False)

    # wallpaper on every monitor
    image = str(HOME / ".local/share/backgrounds/jungey.png")
    keys = subprocess.run(["xfconf-query", "-c", "xfce4-desktop", "-l"], capture_output=True, text=True).stdout.split()
    for k in keys:
        if k.endswith("/last-image"):
            xf("xfce4-desktop", k, image)
            xf("xfce4-desktop", k.replace("last-image", "image-style"), 5)

    # terminal colours
    palette16 = [P.VIEW, "#d0473a", "#62b040", "#e0b84f", "#4f9fb3", "#a77fb8", "#8ec9d6", "#d8dad3",
                 P.SMOKE, "#e86a5c", "#86cf63", "#ecd27a", "#7cc0d1", "#c7a2d6", "#b5e0e9", "#f7f8f3"]
    xf("xfce4-terminal", "/color-background", P.VIEW)
    xf("xfce4-terminal", "/color-foreground", P.TEXT)
    xf("xfce4-terminal", "/color-cursor", P.ACCENT)
    xf("xfce4-terminal", "/color-palette", ";".join(palette16))

    panel.apply()
    subprocess.run(["python3", str(Path(__file__).parent / "vscode.py")], check=False, capture_output=True)
    print("Jungey look applied.")


if __name__ == "__main__":
    main()
