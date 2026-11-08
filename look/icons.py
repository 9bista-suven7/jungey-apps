#!/usr/bin/env python3
"""Build the Jungey icon theme into ~/.local/share/icons/Jungey.

Apps downloaded from outside Ubuntu (Chrome, VS Code, MongoDB Compass, DBeaver)
keep their own icons. Every other app icon is a glossy gradient tile in its own colour, with a soft-shadowed
white glyph and a tint for the second lines (Big Sur style). Icons are also installed under the original apps' icon
names, so menus, the dock and window lists all pick them up. Folders are
Papirus-Dark's, recoloured to amber gold. Anything not covered falls back to
Papirus-Dark.
"""
import os
import re
import shutil
import subprocess
from pathlib import Path

import palette as P

DEST = Path.home() / ".local/share/icons/Jungey"
PAPIRUS = Path("/usr/share/icons/Papirus-Dark")

C = "#ffffff"      # main line colour: white
V = "__TINT__"     # second colour: each icon's own tint (see TINTS)
M = "#bbf7d0"
A = "#fff1a8"      # small suns and dots
FILL = "__FILL__"  # behind overlapping shapes: the tile colour

TILE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <!-- {comment} -->
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0.35" y2="1">
      {stops}
    </linearGradient>
    <radialGradient id="shine" cx="0.28" cy="0.08" r="0.95">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.34"/>
      <stop offset="0.55" stop-color="#ffffff" stop-opacity="0.04"/>
      <stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="shade" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0.6" stop-color="#000000" stop-opacity="0"/>
      <stop offset="1" stop-color="#000000" stop-opacity="0.22"/>
    </linearGradient>
    <linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.45"/>
      <stop offset="0.4" stop-color="#ffffff" stop-opacity="0.06"/>
      <stop offset="1" stop-color="#000000" stop-opacity="0.18"/>
    </linearGradient>
    <filter id="lift" x="-30%" y="-30%" width="160%" height="160%">
      <feDropShadow dx="0" dy="5" stdDeviation="6" flood-color="#000000" flood-opacity="0.28"/>
    </filter>
  </defs>
  <rect x="8" y="8" width="240" height="240" rx="56" fill="url(#bg)"/>
  <rect x="8" y="8" width="240" height="240" rx="56" fill="url(#shine)"/>
  <rect x="8" y="8" width="240" height="240" rx="56" fill="url(#shade)"/>
  <rect x="8.75" y="8.75" width="238.5" height="238.5" rx="55.25" fill="none" stroke="url(#rim)" stroke-width="1.5"/>
  <g filter="url(#lift)" fill="none" stroke-width="12" stroke-linecap="round" stroke-linejoin="round">
{body}
  </g>
</svg>
"""

# each app's tile: (top colour, bottom colour, tint for second lines[, middle colour])
TINTS = {
    "jungey-files": ("#ffc94d", "#e0791a", "#fff1c2"),
    "jungey-browser": ("#3ddbc9", "#1560b0", "#b8f5ee"),
    "jungey-mail": ("#61c9ff", "#2459d6", "#cdeeff"),
    "jungey-code": ("#4fa3ff", "#1a3fa8", "#bcdcff"),
    "jungey-data": ("#8a95ff", "#3d2fb0", "#d6dbff"),
    "jungey-mongo": ("#5fd38a", "#11804a", "#c9f6d9"),
    "jungey-mirror": ("#7a86ff", "#2c2a96", "#d4d8ff"),
    "jungey-player": ("#ff6fb5", "#7c2ae0", "#ffd2ec"),
    "jungey-camera": ("#ff9a6b", "#d8344a", "#ffe0d2"),
    "jungey-photos": ("#ffd36e", "#ff4f7b", "#fff4cf", "#ff9a5c"),
    "jungey-pdf": ("#ff6f5e", "#9b1c24", "#ffd6cf"),
    "jungey-rename": ("#b996ff", "#5b32c4", "#e7dcff"),
    "jungey-archive": ("#d6bf72", "#7a6420", "#f6eccb"),
    "jungey-terminal": ("#3b4452", "#11151b", "#7dffa0"),
    "jungey-monitor": ("#4ade80", "#0b6e5a", "#d2fbe2"),
    "jungey-packages": ("#f5ad63", "#a5501b", "#ffe6c9"),
    "jungey-bluetooth": ("#4c9dff", "#1640c2", "#d2e6ff"),
    "jungey-sound": ("#ffab45", "#e2452f", "#ffe6c4"),
    "jungey-network": ("#2ee6d6", "#0a6f9e", "#c9fbf6"),
    "jungey-settings": ("#8e98a8", "#353c49", "#ffd36e"),
    "jungey-find": ("#62bcff", "#3a46d6", "#d6edff"),
    "jungey-snip": ("#3fc5ff", "#4a2fd0", "#ffd1ea"),
    "jungey-tv": ("#ff5f6d", "#6d2ff0", "#ffd9de", "#c23aa8"),
    "jungey-control": ("#5b6cff", "#14b3d6", "#d4f6ff"),
    "jungey-notepad": ("#ffd65c", "#f0841a", "#fff3c4"),
    "jungey": ("#2dd4bf", "#d946ef", "#e0fbff", "#6366f1"),
}


# name -> (comment, line art, original icon names this replaces)
ICONS = {
    "jungey-browser": ("Jungey Browser: a globe", f"""
    <circle cx="128" cy="128" r="76" stroke="{C}"/>
    <ellipse cx="128" cy="128" rx="32" ry="76" stroke="{V}" stroke-width="10"/>
    <path d="M54 128 H202" stroke="{C}" stroke-width="10"/>
    <path d="M68 90 H188 M68 166 H188" stroke="{V}" stroke-width="9"/>""",
        ["org.xfce.webbrowser", "web-browser"]),

    "jungey-mail": ("Jungey Mail: an envelope", f"""
    <rect x="40" y="68" width="176" height="120" rx="18" stroke="{C}"/>
    <path d="M54 84 L128 138 L202 84" stroke="{V}"/>""",
        ["org.xfce.mailreader"]),

    "jungey-code": ("Jungey Code: angle brackets and a slash", f"""
    <path d="M94 82 L50 128 L94 174" stroke="{C}" stroke-width="14"/>
    <path d="M162 82 L206 128 L162 174" stroke="{C}" stroke-width="14"/>
    <path d="M144 64 L112 192" stroke="{V}" stroke-width="12"/>""",
        []),

    "jungey-data": ("Jungey Data: a database cylinder", f"""
    <ellipse cx="128" cy="74" rx="70" ry="24" stroke="{C}"/>
    <path d="M58 74 V182 A70 24 0 0 0 198 182 V74" stroke="{C}"/>
    <path d="M58 128 A70 24 0 0 0 198 128" stroke="{V}" stroke-width="10"/>""",
        []),

    "jungey-mongo": ("Jungey Mongo: a leaf", f"""
    <path d="M128 40 C182 92 182 164 128 206 C74 164 74 92 128 40 Z" stroke="{M}"/>
    <path d="M128 88 V222" stroke="{C}" stroke-width="10"/>""",
        []),

    "jungey-mirror": ("Jungey Mirror: a phone shown on a screen", f"""
    <rect x="34" y="52" width="188" height="126" rx="16" stroke="{C}" stroke-width="11"/>
    <path d="M100 210 H156 M128 180 V208" stroke="{C}" stroke-width="10"/>
    <rect x="102" y="72" width="52" height="88" rx="10" fill="{FILL}" stroke="{V}"/>
    <path d="M120 146 H136" stroke="{V}" stroke-width="8"/>""",
        ["scrcpy"]),

    "jungey-player": ("Jungey Player: a ring with the play mark", f"""
    <circle cx="128" cy="128" r="80" stroke="{C}"/>
    <path d="M110 92 L168 128 L110 164 Z" fill="{V}" stroke="{V}" stroke-width="8"/>""",
        ["org.xfce.parole", "parole"]),

    "jungey-camera": ("Jungey Camera: a camera body and lens", f"""
    <path d="M92 84 L106 62 H150 L164 84" stroke="{C}"/>
    <rect x="38" y="84" width="180" height="118" rx="20" stroke="{C}"/>
    <circle cx="128" cy="142" r="34" stroke="{V}"/>
    <circle cx="188" cy="110" r="7" fill="{A}" stroke="none"/>""",
        ["org.gnome.Cheese", "cheese"]),

    "jungey-photos": ("Jungey Photos: a frame with hills and a sun", f"""
    <rect x="40" y="56" width="176" height="144" rx="18" stroke="{C}"/>
    <path d="M58 182 L106 126 L138 162 L160 140 L198 182" stroke="{V}" stroke-width="11"/>
    <circle cx="168" cy="96" r="15" fill="{A}" stroke="none"/>""",
        ["org.xfce.ristretto", "ristretto"]),

    "jungey-pdf": ("Jungey PDF: a page with a bookmark ribbon", f"""
    <path d="M80 38 H150 L194 82 V200 Q194 218 176 218 H80 Q62 218 62 200 V56 Q62 38 80 38 Z" stroke="{C}"/>
    <path d="M150 38 V82 H194" stroke="{C}" stroke-width="10"/>
    <path d="M94 38 V104 L112 90 L130 104 V38" fill="{V}" stroke="{V}" stroke-width="6"/>
    <path d="M92 146 H164 M92 178 H144" stroke="{C}" stroke-width="10"/>""",
        ["atril", "org.mate.atril"]),

    "jungey-files": ("Jungey Files: a folder", f"""
    <path d="M38 78 Q38 58 58 58 H100 L120 78 H198 Q218 78 218 98 V182 Q218 202 198 202 H58 Q38 202 38 182 Z" stroke="{C}"/>
    <path d="M38 106 H218" stroke="{V}" stroke-width="10"/>""",
        ["org.xfce.thunar", "thunar", "system-file-manager", "org.xfce.filemanager", "file-manager"]),

    "jungey-rename": ("Jungey Rename: a text field with a cursor", f"""
    <rect x="34" y="88" width="188" height="80" rx="18" stroke="{C}"/>
    <path d="M62 128 H122" stroke="{C}" stroke-width="10"/>
    <path d="M160 60 V196 M142 60 H178 M142 196 H178" stroke="{V}" stroke-width="10"/>""",
        []),

    "jungey-archive": ("Jungey Archive: a box with a zip", f"""
    <rect x="38" y="54" width="180" height="42" rx="12" stroke="{C}"/>
    <path d="M52 96 V192 Q52 208 68 208 H188 Q204 208 204 192 V96" stroke="{C}"/>
    <path d="M128 104 V196" stroke="{V}" stroke-width="12" stroke-dasharray="1 20"/>""",
        ["xarchiver", "org.xfce.xarchiver"]),

    "jungey-terminal": ("Jungey Terminal: a prompt", f"""
    <rect x="34" y="50" width="188" height="156" rx="20" stroke="{C}"/>
    <path d="M70 104 L104 128 L70 152" stroke="{V}"/>
    <path d="M122 158 H180" stroke="{C}"/>""",
        ["org.xfce.terminal", "xfce4-terminal", "org.xfce.terminalemulator",
         "utilities-terminal", "terminal"]),

    "jungey-monitor": ("Jungey Monitor: a pulse line", f"""
    <rect x="34" y="54" width="188" height="148" rx="20" stroke="{C}"/>
    <path d="M56 138 H90 L108 100 L132 170 L152 116 L166 138 H200" stroke="{V}" stroke-width="11"/>""",
        ["org.xfce.taskmanager", "xfce4-taskmanager", "utilities-system-monitor"]),

    "jungey-packages": ("Jungey Packages: a box seen from the corner", f"""
    <path d="M128 38 L206 82 V172 L128 216 L50 172 V82 Z" stroke="{C}"/>
    <path d="M50 82 L128 126 L206 82 M128 126 V216" stroke="{C}" stroke-width="10"/>
    <path d="M89 60 L167 104 V138" stroke="{V}" stroke-width="10"/>""",
        ["synaptic"]),

    "jungey-bluetooth": ("Jungey Bluetooth: the rune", f"""
    <path d="M90 90 L166 160 L128 198 V58 L166 96 L90 166" stroke="{C}"/>
    <circle cx="60" cy="128" r="8" fill="{V}" stroke="none"/>
    <circle cx="196" cy="128" r="8" fill="{V}" stroke="none"/>""",
        ["blueman"]),

    "jungey-sound": ("Jungey Sound: a speaker with waves", f"""
    <path d="M48 104 H82 L124 66 V190 L82 152 H48 Z" stroke="{C}"/>
    <path d="M152 100 A40 40 0 0 1 152 156" stroke="{V}"/>
    <path d="M176 76 A72 72 0 0 1 176 180" stroke="{V}"/>""",
        ["multimedia-volume-control", "pavucontrol"]),

    "jungey-network": ("Jungey Network: three linked nodes", f"""
    <path d="M118 84 L76 166 M138 84 L180 166 M86 188 H170" stroke="{V}" stroke-width="10"/>
    <circle cx="128" cy="64" r="22" fill="{FILL}" stroke="{C}"/>
    <circle cx="64" cy="188" r="22" fill="{FILL}" stroke="{C}"/>
    <circle cx="192" cy="188" r="22" fill="{FILL}" stroke="{C}"/>""",
        []),

    "jungey-settings": ("Jungey Settings: three sliders", f"""
    <path d="M52 78 H204 M52 128 H204 M52 178 H204" stroke="{C}" stroke-width="10"/>
    <circle cx="96" cy="78" r="17" fill="{FILL}" stroke="{V}" stroke-width="10"/>
    <circle cx="164" cy="128" r="17" fill="{FILL}" stroke="{V}" stroke-width="10"/>
    <circle cx="118" cy="178" r="17" fill="{FILL}" stroke="{V}" stroke-width="10"/>""",
        ["org.xfce.settings.manager", "xfce4-settings-manager"]),

    "jungey-find": ("Jungey Find: a magnifier", f"""
    <circle cx="112" cy="112" r="58" stroke="{C}"/>
    <path d="M155 155 L202 202" stroke="{V}" stroke-width="18"/>""",
        ["org.xfce.appfinder", "xfce4-appfinder"]),

    "jungey-snip": ("Jungey Snip: capture corners and a pointer", f"""
    <path d="M52 104 V76 Q52 52 76 52 H104 M152 52 H180 Q204 52 204 76 V104 M52 152 V180 Q52 204 76 204 H104" stroke="{C}"/>
    <path d="M146 146 L212 172 L184 182 L172 210 Z" fill="{V}" stroke="{V}" stroke-width="8"/>""",
        []),

    # your own Jungey apps, redrawn in the same style
    "jungey-tv": ("Jungey TV: a screen on a stand with the play mark", f"""
    <rect x="44" y="62" width="168" height="112" rx="16" stroke="{C}"/>
    <path d="M98 206 H158 M128 174 V204" stroke="{C}"/>
    <path d="M114 94 L152 118 L114 142 Z" fill="{V}" stroke="{V}" stroke-width="6"/>""",
        []),

    "jungey-control": ("Jungey Control: a tablet and a phone with a signal", f"""
    <rect x="40" y="58" width="112" height="146" rx="16" stroke="{C}" stroke-width="11"/>
    <path d="M84 186 H108" stroke="{C}" stroke-width="10"/>
    <rect x="132" y="92" width="76" height="126" rx="15" fill="{FILL}" stroke="{V}"/>
    <path d="M160 200 H180" stroke="{V}" stroke-width="10"/>
    <path d="M178 40 A34 34 0 0 1 212 74 M178 58 A16 16 0 0 1 194 74" stroke="{V}" stroke-width="10"/>""",
        []),

    "jungey-notepad": ("Jungey Notepad: a page with lines", f"""
    <path d="M80 38 H150 L194 82 V200 Q194 218 176 218 H80 Q62 218 62 200 V56 Q62 38 80 38 Z" stroke="{C}"/>
    <path d="M150 38 V82 H194" stroke="{C}" stroke-width="10"/>
    <path d="M92 124 H164 M92 152 H164 M92 180 H140" stroke="{V}" stroke-width="10"/>""",
        []),

    "jungey": ("Jungey AI: a friendly robot whose mouth is a voice wave, with a spark", f"""
    <path d="M124 82 V64" stroke="{C}" stroke-width="10"/>
    <circle cx="124" cy="56" r="11" fill="{C}" stroke="none"/>
    <rect x="36" y="124" width="18" height="44" rx="9" fill="{C}" stroke="none"/>
    <rect x="194" y="124" width="18" height="44" rx="9" fill="{C}" stroke="none"/>
    <rect x="60" y="88" width="128" height="108" rx="34" stroke="{C}" fill="{C}" fill-opacity="0.14"/>
    <circle cx="100" cy="132" r="13" fill="{C}" stroke="none"/>
    <circle cx="148" cy="132" r="13" fill="{C}" stroke="none"/>
    <path d="M104 164 V170 M114 159 V175 M124 154 V180 M134 159 V175 M144 164 V170" stroke="{V}" stroke-width="7"/>
    <path d="M200 30 L207 50 L227 57 L207 64 L200 84 L193 64 L173 57 L193 50 Z" fill="#fbfbef" stroke="none"/>""",
        []),
}

# Plain glyphs (no tile) for small controls in the bar and menus.
GLYPH = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <g fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</g>
</svg>
"""
GLYPHS = {
    # Jungey Center on the bar: two toggles, like Control Center
    "jungey-center": ('<rect x="2.5" y="4.5" width="19" height="6.5" rx="3.25"/><circle cx="6.75" cy="7.75" r="1.6" fill="{color}" stroke="none"/>'
                      '<rect x="2.5" y="13" width="19" height="6.5" rx="3.25"/><circle cx="17.25" cy="16.25" r="1.6" fill="{color}" stroke="none"/>',
                      []),
    "jungey-power": ('<path d="M12 3.5 V11"/><path d="M7 6.5 A7.5 7.5 0 1 0 17 6.5"/>',
                     ["system-log-out", "system-shutdown", "xfsm-logout", "system-shutdown-symbolic"]),
    "jungey-lock": ('<rect x="5.5" y="10.5" width="13" height="9.5" rx="2.5"/><path d="M8.5 10.5 V8 A3.5 3.5 0 0 1 15.5 8 V10.5"/>',
                    ["system-lock-screen", "xfsm-lock"]),
    "jungey-restart": ('<path d="M19 12 A7 7 0 1 1 16.5 6.6"/><path d="M17 3 V7 H13"/>',
                       ["system-reboot", "xfsm-reboot"]),
    "jungey-sleep": ('<path d="M19 14.5 A7.5 7.5 0 1 1 9.5 5 A6 6 0 0 0 19 14.5 Z"/>',
                     ["system-suspend", "xfsm-suspend", "system-hibernate"]),
}


# Papirus folder colours -> amber gold
FOLDER_COLOURS = {
    "#5294e2": "#f2b33d",  # folder front
    "#4877b1": "#c27f17",  # folder back
    "#1d344f": "#5a3a05",  # the little picture on special folders
}


def build_apps():
    apps = DEST / "scalable/apps"
    apps.mkdir(parents=True, exist_ok=True)
    for name, (comment, body, aliases) in ICONS.items():
        top, bottom, tint, *mid = TINTS[name]
        stops = [f'<stop offset="0" stop-color="{top}"/>']
        if mid:
            stops.append(f'<stop offset="0.5" stop-color="{mid[0]}"/>')
        stops.append(f'<stop offset="1" stop-color="{bottom}"/>')
        art = body.strip("\n").replace("__TINT__", tint).replace("__FILL__", mid[0] if mid else bottom)
        svg = TILE.format(comment=comment, body=art, stops="\n      ".join(stops))
        (apps / f"{name}.svg").write_text(svg)
        for alias in aliases:
            link = apps / f"{alias}.svg"
            if link.is_symlink() or link.exists():
                link.unlink()
            link.symlink_to(f"{name}.svg")


ORBIT_J = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
  <!-- Jungey: a J crossed by an orbit, on a dark glass key with a red edge -->
  <defs>
    <radialGradient id="glow" cx="0.5" cy="1.25" r="0.85">
      <stop offset="0" stop-color="#d0473a" stop-opacity="0.75"/>
      <stop offset="1" stop-color="#d0473a" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="j" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="1" stop-color="#ffd9d2"/>
    </linearGradient>
    <linearGradient id="o" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#ff5a45" stop-opacity="0.15"/>
      <stop offset="0.55" stop-color="#ff5a45"/>
      <stop offset="1" stop-color="#ffb3a1"/>
    </linearGradient>
    <clipPath id="front"><rect x="-10" y="25" width="68" height="40" transform="rotate(-24 24 25)"/></clipPath>
  </defs>
  <rect x="1.5" y="1.5" width="45" height="45" rx="12" fill="#04161d"/>
  <rect x="1.5" y="1.5" width="45" height="45" rx="12" fill="url(#glow)"/>
  <rect x="2.25" y="2.25" width="43.5" height="43.5" rx="11.25" fill="none" stroke="#e0533f" stroke-width="1.5"/>
  <g transform="translate(24 24) scale(0.74) translate(-24 -24)">
    <g fill="none" stroke-linecap="round">
      <ellipse cx="24" cy="25" rx="21" ry="7.5" transform="rotate(-24 24 25)" stroke="url(#o)" stroke-width="2.4" opacity="0.55"/>
      <path d="M28.5 8.5 V27.5 A8.5 8.5 0 0 1 11.5 27.5" stroke="url(#j)" stroke-width="6"/>
      <ellipse cx="24" cy="25" rx="21" ry="7.5" transform="rotate(-24 24 25)" stroke="url(#o)" stroke-width="2.6" clip-path="url(#front)"/>
    </g>
    <circle cx="42.6" cy="17.2" r="2.6" fill="#ffc2b5"/>
  </g>
</svg>
"""


def build_glyphs():
    apps = DEST / "scalable/apps"
    (apps / "jungey-os.svg").write_text(ORBIT_J)
    for name, (body, aliases) in GLYPHS.items():
        color = P.TEXT if name == "jungey-center" else P.ACCENT
        (apps / f"{name}.svg").write_text(GLYPH.format(color=color, body=body.replace("{color}", color if name == "jungey-center" else P.ACCENT_HI)))
        for alias in aliases:
            link = apps / f"{alias}.svg"
            if link.is_symlink() or link.exists():
                link.unlink()
            link.symlink_to(f"{name}.svg")


def battery_svg(level, charging=False, charged=False):
    """A big horizontal battery like macOS: outline, a fill that grows with the charge, a bolt
    when charging. Symbolic icons get recoloured by GTK, so colour comes from classes: success
    (green) while charging, error (red) when low, otherwise the text colour."""
    width = max(2.0, 15.5 * level / 100)
    cls = ' class="success"' if (charging or charged) else (' class="error"' if level <= 20 else "")
    bolt = ('<path d="M11.8 7.2 L7.6 12.6 H10.9 L9.8 16.8 L14.4 11 H11.1 Z" class="warning"/>'
            if charging else "")
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
  <path fill-rule="evenodd" d="M4 5 H17 A3.5 3.5 0 0 1 20.5 8.5 V15.5 A3.5 3.5 0 0 1 17 19 H4 A3.5 3.5 0 0 1 0.5 15.5 V8.5 A3.5 3.5 0 0 1 4 5 Z M4 6.8 A1.7 1.7 0 0 0 2.3 8.5 V15.5 A1.7 1.7 0 0 0 4 17.2 H17 A1.7 1.7 0 0 0 18.7 15.5 V8.5 A1.7 1.7 0 0 0 17 6.8 Z"/>
  <rect x="21.2" y="9.5" width="2.3" height="5" rx="1.1"/>
  <rect x="2.9" y="7.6" width="{width:.1f}" height="8.8" rx="1.2"{cls}/>
  {bolt}
</svg>
"""


def build_battery():
    """The names xfce4-power-manager asks for: battery-level-NN[-charging|-charged]-symbolic."""
    apps = DEST / "scalable/apps"
    for level in range(0, 101, 10):
        (apps / f"battery-level-{level}-symbolic.svg").write_text(battery_svg(level))
        (apps / f"battery-level-{level}-charging-symbolic.svg").write_text(battery_svg(level, charging=True))
    (apps / "battery-level-100-charged-symbolic.svg").write_text(battery_svg(100, charged=True))
    (apps / "battery-missing-symbolic.svg").write_text(battery_svg(0))


def build_folders():
    """Copy every place icon Papirus points at its blue folders, recoloured."""
    dirs = []
    blue = re.compile(r"^(folder|user)-blue([-.])")
    for src_dir in sorted(PAPIRUS.glob("*/places")):
        if src_dir.parent.name == "symbolic":
            continue
        rel = src_dir.relative_to(PAPIRUS)
        out = DEST / rel
        count = 0
        for entry in src_dir.iterdir():
            if not entry.is_symlink() or not blue.match(Path(os.readlink(entry)).name):
                continue
            svg = entry.resolve().read_text()
            for old, new in FOLDER_COLOURS.items():
                svg = re.sub(old, new, svg, flags=re.I)
            out.mkdir(parents=True, exist_ok=True)
            (out / entry.name).write_text(svg)
            count += 1
        if count:
            dirs.append(str(rel))
    return dirs


def papirus_section(rel):
    """Reuse Papirus-Dark's index.theme entry for a directory."""
    text = (PAPIRUS / "index.theme").read_text()
    m = re.search(r"^\[" + re.escape(rel) + r"\]\n(.*?)(?=^\[|\Z)", text, re.S | re.M)
    return f"[{rel}]\n{m.group(1).strip()}\n"


def write_index(folder_dirs):
    dirs = ["scalable/apps"] + folder_dirs
    sections = ["[scalable/apps]\nSize=256\nMinSize=16\nMaxSize=512\nContext=Applications\nType=Scalable\n"]
    sections += [papirus_section(d) for d in folder_dirs]
    (DEST / "index.theme").write_text(
        "[Icon Theme]\n"
        "Name=Jungey\n"
        "Comment=Jungey OS icons: glossy colour tiles and amber folders, built on Papirus-Dark\n"
        "Inherits=Papirus-Dark,hicolor\n"
        "Example=folder\n"
        f"Directories={','.join(dirs)}\n\n" + "\n".join(sections)
    )


if __name__ == "__main__":
    if DEST.exists():
        shutil.rmtree(DEST)
    build_apps()
    build_glyphs()
    build_battery()
    folder_dirs = build_folders()
    write_index(folder_dirs)
    subprocess.run(["gtk-update-icon-cache", "-f", "-t", "-q", str(DEST)], check=False)
    n_apps = len(list((DEST / "scalable/apps").iterdir()))
    n_folders = sum(len(list((DEST / d).iterdir())) for d in folder_dirs)
    print(f"Jungey icons: {len(ICONS)} app icons ({n_apps} names), {n_folders} folder icons in {len(folder_dirs)} sizes")
