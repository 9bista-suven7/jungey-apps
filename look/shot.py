#!/usr/bin/env python3
"""Screenshot the whole screen to a PNG (for checking the look): python3 shot.py OUT.png"""
import sys

import gi

gi.require_version("Gdk", "3.0")
from gi.repository import Gdk  # noqa: E402

root = Gdk.get_default_root_window()
w, h = root.get_width(), root.get_height()
Gdk.pixbuf_get_from_window(root, 0, 0, w, h).savev(sys.argv[1], "png", [], [])
print(f"{sys.argv[1]} ({w}x{h})")
