"""The Jungey palette. Every colour in the Jungey look comes from here: the GTK and
window theme, icons, wallpaper, the bottom bar, terminal, VS Code and the Jungey apps.

Built from five chosen colours:
    #032431 deep teal   -> the base everything sits on
    #899041 olive gold  -> the accent
    #636067 smoke       -> quiet text and lines
    #781810 oxblood, #899041 olive, #297910 forest -> window buttons (close, minimize, maximize)
"""

# the five source colours
DEEP_TEAL = "#032431"
OLIVE = "#899041"
SMOKE = "#636067"
OXBLOOD = "#781810"
FOREST = "#297910"

# surfaces, darkest to lightest
VOID = "#01141b"       # the bottom bar: the deepest layer
VIEW = "#021d27"       # content: file grids, editors, lists
BASE = DEEP_TEAL       # window background
SURFACE = "#062c3a"    # sidebars, cards
RAISED = "#0a3444"     # menus, popovers, dialogs
LINE = "#16404f"       # hairlines and borders

TEXT = "#eef0ea"
MUTED = "#a9a6ae"
DIM = SMOKE

ACCENT = "#c2ca6e"     # olive gold, brightened so it reads (and glows) on teal
ACCENT_BG = OLIVE      # fills: buttons, selection (dark text on it)
ACCENT_FG = "#0b1a0f"
ACCENT_HI = "#e1e6a6"
SECOND = "#8ec9d6"     # pale teal, the quiet second colour in icons and charts

SUCCESS = "#79d45a"    # forest, lifted so it reads on dark teal
WARNING = "#e0b84f"
ERROR = "#d0473a"      # oxblood, lifted for dark backgrounds
ERROR_BG = OXBLOOD

# window buttons, macOS style: close, minimize, maximize
CLOSE, MINIMIZE, MAXIMIZE = OXBLOOD, OLIVE, FOREST


def rgb(hex_color):
    h = hex_color.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def hls(hex_color):
    import colorsys
    return colorsys.rgb_to_hls(*(c / 255 for c in rgb(hex_color)))


def css_vars():
    """Every name above, for $NAME substitution in the CSS templates."""
    return {k: v for k, v in globals().items() if k.isupper() and isinstance(v, str)}
