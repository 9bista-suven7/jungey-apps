# Jungey Apps

The desktop apps and look of Jungey OS — a lightweight Ubuntu 24.04 + Xfce setup
re-skinned dark and premium, with native GTK4 apps in place of the stock ones.

## Apps (Rust + GTK4 / libadwaita)

| Crate | What it is |
|-------|------------|
| `jungey-ui` | Shared theme and widgets used by every app below |
| `jungey-files` | File manager (browse, organise, rename, find) |
| `jungey-launchpad` | Full-screen app grid, macOS-style |
| `jungey-center` | Control-centre: Wi-Fi, Bluetooth, sound, brightness, battery, power |
| `jungey-tasks` | Task manager: what's running in plain words, with memory, CPU and a way to stop it |

## `look/`

Python scripts that paint the desktop: the palette, GTK/Xfce theme, colourful app
icons, the single centered floating bottom bar, picom compositing, touchpad gestures,
and the menu rules that give downloaded apps a Jungey icon while keeping their names.

## Build & install

```bash
sudo bash setup.sh     # one-time: packages, theme, compositor, gestures
./install.sh           # build the apps and switch them in (no sudo)
```

`faster.sh` tunes the system for speed; `security-tools.sh` and `install-ghidra.sh`
install a small set of standard security tools.

## Layout

Cargo workspace at the root. The build output (`target/`), an upstream model clone
(`ace-step/`) and local experiments (`music/`) are git-ignored.
