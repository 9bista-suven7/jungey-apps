#!/usr/bin/env python3
"""Give apps their Jungey icon in the menu, bar and Launchpad, and hide duplicates.
Apps keep their own names (only Jungey's own apps are called "Jungey ...").

    python3 menu.py          apply
    python3 menu.py --undo   put everything back

System apps get an override in ~/.local/share/applications (same file name wins
over /usr/share/applications). Apps you installed yourself are edited in place.
Either way the original name and icon are kept in X-Jungey-Original-* keys, and
panel launchers keep a .jungey-orig copy, so --undo can restore them exactly.
Apps downloaded from outside Ubuntu (Chrome, VS Code, DBeaver, MongoDB Compass, Claude)
keep their own names and icons; only DBeaver's entry is tidied from "dbeaver-ce" to "DBeaver".
"""
import subprocess
import sys
from pathlib import Path

HOME = Path.home()
USER_APPS = HOME / ".local/share/applications"
SYSTEM_APPS = [Path("/usr/local/share/applications"), Path("/usr/share/applications")]
PANEL = HOME / ".config/xfce4/panel"

HIDE = None
KEEP = ""  # keep the app's own name, only change the icon
# desktop id -> (name or KEEP, icon or None to keep the app's own); (HIDE, None) hides a duplicate
APPS = {
    "scrcpy.desktop":                   (KEEP, "jungey-mirror"),
    "org.xfce.Parole.desktop":          (KEEP, "jungey-player"),
    "org.gnome.Cheese.desktop":         (KEEP, "jungey-camera"),
    "org.xfce.ristretto.desktop":       (KEEP, "jungey-photos"),
    "atril.desktop":                    (KEEP, "jungey-pdf"),
    "thunar.desktop":                   (KEEP, "jungey-files"),
    "thunar-bulk-rename.desktop":       (KEEP, "jungey-rename"),
    "xarchiver.desktop":                (KEEP, "jungey-archive"),
    "xfce4-terminal.desktop":           (KEEP, "jungey-terminal"),
    "xfce4-taskmanager.desktop":        (HIDE, None),   # replaced by Jungey Tasks
    "synaptic.desktop":                 (KEEP, "jungey-packages"),
    "blueman-manager.desktop":          (KEEP, "jungey-bluetooth"),
    "pavucontrol.desktop":              (KEEP, "jungey-sound"),
    "nm-connection-editor.desktop":     (KEEP, "jungey-network"),
    "xfce-settings-manager.desktop":    (KEEP, "jungey-settings"),
    "xfce4-appfinder.desktop":          (KEEP, "jungey-find"),
    "jungey-snip.desktop":              ("Jungey Snip", None),
    # duplicates and terminal-only tools: still installed, just not in the menu
    "xfce4-web-browser.desktop":        (HIDE, None),
    "xfce4-file-manager.desktop":       (HIDE, None),
    "xfce4-terminal-emulator.desktop":  (HIDE, None),
    "xfce4-mail-reader.desktop":        (HIDE, None),   # no mail app installed
    "xfce4-about.desktop":              (HIDE, None),
    "scrcpy-console.desktop":           (HIDE, None),
    "nvim.desktop":                     (HIDE, None),
    "vim.desktop":                      (HIDE, None),
    "htop.desktop":                     (HIDE, None),
}

# panel launchers / favourites that point at removed apps -> their Jungey replacement
REPLACED = {
    "org.gnome.Epiphany.desktop": "google-chrome.desktop",
    "geany.desktop": "com.microsoft.VSCode.desktop",
    "org.xfce.mousepad.desktop": "jungey-notepad.desktop",
    "xfce4-screenshooter.desktop": "jungey-snip.desktop",
    "xfce4-taskmanager.desktop": "jungey-tasks.desktop",
}


def system_file(desktop_id):
    for d in SYSTEM_APPS:
        if (d / desktop_id).exists():
            return d / desktop_id
    return None


def rewrite(text, name, icon):
    """Set Name/Icon (or hide) in the [Desktop Entry] section, remembering the originals."""
    lines = text.splitlines()
    main = [i for i, l in enumerate(lines) if l.strip() == "[Desktop Entry]"][0]
    end = next((i for i in range(main + 1, len(lines)) if lines[i].startswith("[")), len(lines))
    entry = lines[main + 1:end]
    keys = {l.split("=", 1)[0]: l.split("=", 1)[1] for l in entry if "=" in l}

    orig_name = keys.get("X-Jungey-Original-Name", keys.get("Name", ""))
    orig_icon = keys.get("X-Jungey-Original-Icon", keys.get("Icon", ""))
    out = [l for l in entry if not l.startswith(("Name[", "X-Jungey-"))]
    if name is HIDE:
        out = [l for l in out if not l.startswith("NoDisplay=")] + ["NoDisplay=true", "X-Jungey-Hidden=true"]
    else:
        out = [f"Name={name or orig_name}" if l.startswith("Name=") else l for l in out]
        # downloaded apps (icon None) keep their own icon
        icon = icon or orig_icon
        if icon:
            out = [f"Icon={icon}" if l.startswith("Icon=") else l for l in out]
        # keep the old name searchable in the menu
        words = [w for w in keys.get("Keywords", "").split(";") if w]
        if orig_name not in words:
            words.append(orig_name)
        out = [l for l in out if not l.startswith("Keywords=")] + ["Keywords=" + ";".join(words) + ";"]
    out += [f"X-Jungey-Original-Name={orig_name}", f"X-Jungey-Original-Icon={orig_icon}"]
    return "\n".join(lines[:main + 1] + out + lines[end:]) + "\n"


def apply_menu():
    USER_APPS.mkdir(parents=True, exist_ok=True)
    done = []
    for desktop_id, (name, icon) in APPS.items():
        dest = USER_APPS / desktop_id
        src = system_file(desktop_id)
        if dest.exists():
            text = dest.read_text()            # yours, or our override from a previous run
        elif src:
            text = src.read_text()
        else:
            continue
        dest.write_text(rewrite(text, name, icon))
        done.append(name or f"(hidden) {desktop_id}")
    return done


def entry_keys(text):
    return dict(l.split("=", 1) for l in text.split("\n[", 1)[0].splitlines() if "=" in l)


def apply_panel():
    """Panel launchers hold their own copy of a menu entry; match them by original name."""
    targets = {}
    for desktop_id in list(APPS) + list(REPLACED):
        src = system_file(desktop_id) or USER_APPS / desktop_id
        if src.exists():
            keys = entry_keys(src.read_text())
            targets[keys.get("X-Jungey-Original-Name") or keys.get("Name")] = REPLACED.get(desktop_id, desktop_id)

    changed = []
    for f in sorted(PANEL.glob("launcher-*/*.desktop")):
        orig = f.with_suffix(".desktop.jungey-orig")
        if not orig.exists():
            orig.write_text(f.read_text())
        name = entry_keys(orig.read_text()).get("Name")
        target = targets.get(name)
        if target and (USER_APPS / target).exists():
            f.write_text((USER_APPS / target).read_text())
            changed.append(f"{name} -> {entry_keys(f.read_text())['Name']}")
    return changed


def favourites(mapping):
    prop = "/plugins/plugin-1/favorites"
    cur = subprocess.run(["xfconf-query", "-c", "xfce4-panel", "-p", prop], capture_output=True, text=True).stdout
    items = [l for l in cur.splitlines()[2:] if l.strip()]
    new = [mapping.get(i, i) for i in items]
    if new != items:
        args = ["xfconf-query", "-c", "xfce4-panel", "-p", prop, "--force-array"]
        for i in new:
            args += ["-t", "string", "-s", i]
        subprocess.run(args, check=True)
    return new


def undo():
    for f in USER_APPS.glob("*.desktop"):
        text = f.read_text()
        if "X-Jungey-Original-Name=" not in text:
            continue
        if system_file(f.name):
            f.unlink()                          # our override: drop it
            continue
        keys = dict(l.split("=", 1) for l in text.splitlines() if l.startswith("X-Jungey-Original-"))
        out = []
        for l in text.splitlines():
            if l.startswith("X-Jungey-") or (l == "NoDisplay=true" and "X-Jungey-Hidden=true" in text):
                continue
            if l.startswith("Name=") and keys.get("X-Jungey-Original-Name"):
                l = "Name=" + keys["X-Jungey-Original-Name"]
            if l.startswith("Icon=") and keys.get("X-Jungey-Original-Icon"):
                l = "Icon=" + keys["X-Jungey-Original-Icon"]
            out.append(l)
        f.write_text("\n".join(out) + "\n")
    for orig in PANEL.glob("launcher-*/*.desktop.jungey-orig"):
        orig.replace(orig.with_suffix(""))


def refresh():
    subprocess.run(["update-desktop-database", "-q", str(USER_APPS)], check=False)
    # `xfce4-panel -r` can quit without coming back, so stop it and start a fresh one
    subprocess.run(["xfce4-panel", "-q"], check=False)
    subprocess.run(["sh", "-c", "sleep 1; setsid -f xfce4-panel >/dev/null 2>&1 </dev/null"], check=False)


if __name__ == "__main__":
    if "--undo" in sys.argv:
        undo()
        refresh()
        print("Menu, panel launchers restored. (Favourites keep their current apps.)")
        sys.exit()
    renamed = apply_menu()
    launchers = apply_panel()
    favs = favourites(REPLACED)
    refresh()
    print(f"{sum(1 for n in renamed if not n.startswith('(hidden)'))} apps renamed, "
          f"{sum(1 for n in renamed if n.startswith('(hidden)'))} duplicates hidden")
    print("panel launchers:", ", ".join(launchers) or "none")
    print("favourites:", ", ".join(favs))
