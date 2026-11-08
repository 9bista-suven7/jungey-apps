#!/usr/bin/env python3
"""Build the Jungey GTK + window theme into ~/.local/share/themes/Jungey.

Starts from vendor/jungey-base (Orchis-Dark with rounder corners, navy and cyan)
and moves every colour onto palette.py: navy surfaces become the base colour,
cyan becomes the accent, status colours follow the palette. PNG/SVG assets are
recoloured the same way. Then it adds macOS-style window buttons, desktop.css
(bottom bar, menus, title buttons) and notify.css, and writes the palette for
the Jungey apps (jungey-ui/src/palette.css).

    uv run --with pillow python3 theme.py
"""
import colorsys
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from string import Template

from PIL import Image

import palette as P

HERE = Path(__file__).parent
SRC = HERE / "vendor/jungey-base"
DEST = Path.home() / ".local/share/themes/Jungey"
UI_PALETTE = HERE.parent / "jungey-ui/src/palette.css"

SURF_H = P.hls(P.BASE)[0]
ACC_H, ACC_L, ACC_S = P.hls(P.ACCENT_BG)
NEU_H, _, NEU_S = P.hls(P.SMOKE)
EXACT = {  # Orchis status colours
    (0xF4, 0x43, 0x36): P.rgb(P.ERROR),
    (0xFB, 0xC0, 0x2D): P.rgb(P.WARNING),
    (0x81, 0xC9, 0x95): P.rgb(P.SUCCESS),
    (0xBA, 0x68, 0xC8): P.rgb(P.ACCENT_HI),
}
_cache = {}


def clamp(x, lo, hi):
    return max(lo, min(hi, x))


def remap(r, g, b):
    """Map one source (navy/cyan) colour onto the palette."""
    key = (r, g, b)
    if key in _cache:
        return _cache[key]
    out = EXACT.get(key, key)
    if key not in EXACT:
        h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
        if s >= 0.06 and 170 <= h * 360 <= 232:
            if s > 0.55 and l > 0.38:      # the accent family
                nh, nl, ns = ACC_H, clamp(ACC_L + (l - 0.57) * 0.9, 0.05, 0.95), min(0.6, ACC_S + 0.06)
            elif l < 0.36:                 # surfaces
                nl = l * 0.8
                nh, ns = SURF_H, clamp(0.75 - (nl - 0.08) * 2, 0.25, 0.85)
            else:                          # quiet text
                nh, nl, ns = NEU_H, l, NEU_S + 0.03
            out = tuple(round(c * 255) for c in colorsys.hls_to_rgb(nh, nl, ns))
    _cache[key] = out
    return out


def recolor_text(text):
    def six(m):
        r, g, b = (int(m.group(1)[i:i + 2], 16) for i in (0, 2, 4))
        return "#%02x%02x%02x" % remap(r, g, b)

    def func(m):
        parts = [p.strip() for p in m.group(2).split(",")]
        try:
            r, g, b = (int(float(x)) for x in parts[:3])
        except ValueError:
            return m.group(0)
        nr, ng, nb = remap(r, g, b)
        rest = ", " + ", ".join(parts[3:]) if len(parts) > 3 else ""
        return f"{m.group(1)}({nr}, {ng}, {nb}{rest})"

    text = re.sub(r"#([0-9a-fA-F]{6})\b", six, text)
    return re.sub(r"\b(rgba?)\(([^)]*)\)", func, text)


def recolor_png(path):
    img = Image.open(path).convert("RGBA")
    px = bytearray(img.tobytes())
    for i in range(0, len(px), 4):
        if px[i + 3]:
            px[i:i + 3] = bytes(remap(px[i], px[i + 1], px[i + 2]))
    img.frombytes(bytes(px))
    img.save(path)


def template(name):
    return Template((HERE / name).read_text()).substitute(P.css_vars())


# --- macOS-style window buttons ------------------------------------------------

GLYPH = {
    "close": "M9 13 L15 19 M15 13 L9 19",
    "hide": "M8.5 16 H15.5",
    "maximize": "M12 12.5 V19.5 M8.5 16 H15.5",
    "maximize-toggled": "M9.5 13.5 L14.5 18.5 M14.5 13.5 V18.5 H9.5",
}
COLOR = {"close": P.CLOSE, "hide": P.MINIMIZE, "maximize": P.MAXIMIZE, "maximize-toggled": P.MAXIMIZE}


def button_svg(kind, state):
    fill = COLOR[kind] if state != "inactive" else "#24414d"
    glyph = state in ("prelight", "pressed")
    shade = 0.28 if state == "pressed" else 0.0
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="24" height="34" viewBox="0 0 24 34">
  <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff" stop-opacity="{0 if state == 'inactive' else 0.28}"/>
    <stop offset="0.6" stop-color="#ffffff" stop-opacity="0"/>
  </linearGradient></defs>
  <circle cx="12" cy="16" r="7" fill="{fill}"/>
  <circle cx="12" cy="16" r="7" fill="url(#g)"/>
  <circle cx="12" cy="16" r="7" fill="#000000" fill-opacity="{shade}"/>
  <circle cx="12" cy="16" r="6.5" fill="none" stroke="#000000" stroke-opacity="0.35"/>
  {f'<path d="{GLYPH[kind]}" stroke="#000000" stroke-opacity="0.62" stroke-width="1.5" stroke-linecap="round" fill="none"/>' if glyph else ''}
</svg>"""


def build_buttons(xfwm):
    with tempfile.TemporaryDirectory() as tmp:
        for kind in GLYPH:
            for state in ("active", "inactive", "prelight", "pressed"):
                svg = Path(tmp) / f"{kind}-{state}.svg"
                svg.write_text(button_svg(kind, state))
                subprocess.run(["rsvg-convert", str(svg), "-o", str(xfwm / f"{kind}-{state}.png")], check=True)
    rc = xfwm / "themerc"
    text = re.sub(r"^(button_offset|button_spacing|title_alignment|title_horizontal_offset)=.*\n", "", rc.read_text(), flags=re.M)
    rc.write_text(text + "button_offset=8\nbutton_spacing=0\ntitle_alignment=center\n")


def build():
    if DEST.exists():
        shutil.rmtree(DEST)
    shutil.copytree(SRC, DEST)
    for f in DEST.rglob("*"):
        if f.suffix in (".css", ".svg", ".xml") or f.name == "themerc":
            f.write_text(recolor_text(f.read_text()))
        elif f.suffix == ".png":
            recolor_png(f)
    build_buttons(DEST / "xfwm4")

    extra = template("desktop.css")
    for name in ("gtk.css", "gtk-dark.css"):
        css = DEST / "gtk-3.0" / name
        css.write_text(css.read_text() + "\n/* ---- Jungey desktop ---- */\n" + extra)
    (DEST / "xfce-notify-4.0/gtk.css").write_text(template("notify.css"))
    UI_PALETTE.write_text(template("ui-palette.css"))
    user_css = Path.home() / ".config/gtk-3.0/gtk.css"
    user_css.parent.mkdir(parents=True, exist_ok=True)
    user_css.write_text(template("user.css"))

    (DEST / "index.theme").write_text(
        "[Desktop Entry]\nType=X-GNOME-Metatheme\nName=Jungey\n"
        "Comment=Jungey OS: deep teal and olive gold\nEncoding=UTF-8\n\n"
        "[X-GNOME-Metatheme]\nGtkTheme=Jungey\nMetacityTheme=Jungey\nIconTheme=Jungey\n"
        "CursorTheme=breeze_cursors\nButtonLayout=close,minimize,maximize:\n"
    )
    print(f"Jungey theme built in {DEST}")


if __name__ == "__main__":
    build()
