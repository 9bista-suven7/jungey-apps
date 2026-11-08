#!/usr/bin/env python3
"""Render the Jungey wallpaper: stepped teal rings, a glowing gold orbit,
and the Jungey "J" in the middle. Output: ~/.local/share/backgrounds/jungey.png
"""
import math
import subprocess
from pathlib import Path

import palette as P

W, H = 2560, 1440
CX, CY = W / 2, H * 0.53
OUT = Path.home() / ".local/share/backgrounds/jungey.png"


def arc(r, start, sweep):
    a0, a1 = math.radians(start), math.radians(start + sweep)
    x0, y0 = CX + r * math.cos(a0), CY + r * math.sin(a0)
    x1, y1 = CX + r * math.cos(a1), CY + r * math.sin(a1)
    return f"M{x0:.1f} {y0:.1f} A{r} {r} 0 {1 if sweep > 180 else 0} 1 {x1:.1f} {y1:.1f}", (x1, y1)


def svg():
    rings = [1180, 940, 740, 570, 430, 310, 205]
    shades = ["#021a23", "#03202b", "#042633", "#062c3a", "#083241", "#0b3948", "#0e4050"]
    discs = "\n".join(
        f'<circle cx="{CX}" cy="{CY}" r="{r}" fill="{c}"/>'
        f'<circle cx="{CX}" cy="{CY}" r="{r}" fill="none" stroke="#ffffff" stroke-opacity="0.035" stroke-width="1.5"/>'
        for r, c in zip(rings, shades)
    )
    orbit, (ox, oy) = arc(570, 200, 105)
    orbit2, (ix, iy) = arc(940, -35, 60)
    orbit3, _ = arc(310, 40, 70)
    s = 1.9  # J scale
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
  <defs>
    <radialGradient id="sky" cx="{CX / W}" cy="{CY / H}" r="0.75">
      <stop offset="0" stop-color="#0c3d4c"/>
      <stop offset="0.55" stop-color="{P.BASE}"/>
      <stop offset="1" stop-color="#010e13"/>
    </radialGradient>
    <radialGradient id="fade" cx="0.5" cy="0.53" r="0.6">
      <stop offset="0" stop-color="#fff" stop-opacity="1"/>
      <stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </radialGradient>
    <mask id="center"><rect width="{W}" height="{H}" fill="url(#fade)"/></mask>
    <pattern id="dots" width="34" height="34" patternUnits="userSpaceOnUse">
      <circle cx="17" cy="17" r="1.1" fill="#ffffff" fill-opacity="0.07"/>
    </pattern>
    <linearGradient id="beam" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{P.ACCENT_HI}"/>
      <stop offset="0.6" stop-color="{P.ACCENT}"/>
      <stop offset="1" stop-color="{P.ACCENT_BG}"/>
    </linearGradient>
    <linearGradient id="jfill" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{P.ACCENT_HI}"/>
      <stop offset="1" stop-color="{P.ACCENT}"/>
    </linearGradient>
    <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
      <feGaussianBlur stdDeviation="9" result="b"/>
      <feMerge><feMergeNode in="b"/><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <filter id="dotglow" filterUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}">
      <feGaussianBlur stdDeviation="6" result="b"/>
      <feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <filter id="haze" x="-50%" y="-50%" width="200%" height="200%">
      <feGaussianBlur stdDeviation="60"/>
    </filter>
  </defs>
  <rect width="{W}" height="{H}" fill="url(#sky)"/>
  {discs}
  <rect width="{W}" height="{H}" fill="url(#dots)" mask="url(#center)"/>
  <circle cx="{CX}" cy="{CY}" r="230" fill="{P.ACCENT_BG}" fill-opacity="0.16" filter="url(#haze)"/>

  <g fill="none" stroke-linecap="round" filter="url(#glow)">
    <path d="{orbit}" stroke="url(#beam)" stroke-width="3"/>
    <path d="{orbit2}" stroke="{P.SECOND}" stroke-opacity="0.55" stroke-width="2"/>
    <path d="{orbit3}" stroke="{P.ACCENT}" stroke-opacity="0.5" stroke-width="2"/>
  </g>
  <circle cx="{ox:.1f}" cy="{oy:.1f}" r="7" fill="{P.ACCENT_HI}" filter="url(#dotglow)"/>
  <circle cx="{ix:.1f}" cy="{iy:.1f}" r="5" fill="{P.SECOND}" filter="url(#dotglow)"/>

  <g transform="translate({CX} {CY}) scale({s}) translate(-64 -64)" filter="url(#glow)">
    <path d="M78 26 V78 A26 26 0 0 1 26 78" fill="none" stroke="url(#jfill)" stroke-width="15" stroke-linecap="round"/>
    <path d="M104 14 L108 26 L120 30 L108 34 L104 46 L100 34 L88 30 L100 26 Z" fill="#fbfbef"/>
  </g>
</svg>
"""


def main():
    OUT.parent.mkdir(parents=True, exist_ok=True)
    src = OUT.with_suffix(".svg")
    src.write_text(svg())
    subprocess.run(["rsvg-convert", "-w", str(W), "-h", str(H), str(src), "-o", str(OUT)], check=True)
    print(OUT)
    # a soft, darker copy for behind Jungey Launchpad (needs pillow: run under uv)
    try:
        from PIL import Image, ImageEnhance, ImageFilter
    except ImportError:
        return
    blur = Image.open(OUT).convert("RGB").resize((W // 2, H // 2)).filter(ImageFilter.GaussianBlur(28))
    ImageEnhance.Brightness(blur).enhance(0.78).save(OUT.with_name("jungey-blur.jpg"), quality=90)


if __name__ == "__main__":
    main()
