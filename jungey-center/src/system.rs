//! Reading and changing system state through the standard tools (nmcli, bluetoothctl,
//! pactl, brightnessctl, rfkill, xfconf). Everything here blocks: call it from
//! `gio::spawn_blocking`.

use std::fs;
use std::process::Command;

/// Run a tool and return its output; anything slower than 2 seconds counts as failed,
/// so one stuck tool never leaves the card empty.
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    run_for(2, cmd, args)
}

fn run_for(seconds: u32, cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("timeout").arg(seconds.to_string()).arg(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).status().map(|s| s.success()).unwrap_or(false)
}

/// Start something and don't wait for it (power actions, settings, screenshot).
pub fn launch(cmd: &str, args: &[&str]) {
    let _ = Command::new(cmd).args(args).spawn();
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub wifi_on: bool,
    pub wifi_name: Option<String>,
    pub bt_on: bool,
    pub bt_devices: Vec<String>,
    pub airplane: bool,
    pub dnd: bool,
    pub volume: f64,
    pub muted: bool,
    pub brightness: Option<f64>,
    pub battery: Option<Battery>,
}

#[derive(Clone, Debug)]
pub struct Battery {
    pub percent: u32,
    pub charging: bool,
    pub full: bool,
    pub hours_left: Option<f64>,
}

pub fn read() -> State {
    let wifi_on = run("nmcli", &["-t", "-f", "WIFI", "general"]).map(|s| s.trim() == "enabled").unwrap_or(false);
    let wifi_name = run("nmcli", &["-t", "-f", "ACTIVE,SSID", "device", "wifi", "list", "--rescan", "no"]).and_then(|s| {
        s.lines().find_map(|l| l.strip_prefix("yes:").map(unescape).filter(|n| !n.is_empty()))
    });
    let bt_show = run("bluetoothctl", &["show"]).unwrap_or_default();
    let bt_on = bt_show.lines().any(|l| l.trim() == "Powered: yes");
    let bt_devices = if bt_on {
        run("bluetoothctl", &["devices", "Connected"])
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.splitn(3, ' ').nth(2).map(str::to_string))
            .collect()
    } else {
        Vec::new()
    };
    let radios = run("nmcli", &["-t", "radio", "all"]).unwrap_or_default();
    let airplane = !radios.is_empty() && radios.trim().split(':').all(|r| r == "disabled");
    let dnd = run("xfconf-query", &["-c", "xfce4-notifyd", "-p", "/do-not-disturb"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false);
    let volume = run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])
        .and_then(|s| s.split('/').nth(1).and_then(|p| p.trim().trim_end_matches('%').parse::<f64>().ok()))
        .unwrap_or(0.0);
    let muted = run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).map(|s| s.contains("yes")).unwrap_or(false);
    State { wifi_on, wifi_name, bt_on, bt_devices, airplane, dnd, volume, muted, brightness: brightness(), battery: battery() }
}

/// nmcli -t escapes ':' as '\:'.
fn unescape(s: &str) -> String {
    s.replace("\\:", ":").replace("\\\\", "\\")
}

fn brightness() -> Option<f64> {
    let dir = fs::read_dir("/sys/class/backlight").ok()?.flatten().next()?.path();
    let now: f64 = fs::read_to_string(dir.join("brightness")).ok()?.trim().parse().ok()?;
    let max: f64 = fs::read_to_string(dir.join("max_brightness")).ok()?.trim().parse().ok()?;
    (max > 0.0).then(|| now / max * 100.0)
}

fn battery() -> Option<Battery> {
    let dir = fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("BAT")).unwrap_or(false))?;
    let read = |f: &str| fs::read_to_string(dir.join(f)).ok().map(|s| s.trim().to_string());
    let percent: u32 = read("capacity")?.parse().ok()?;
    let status = read("status").unwrap_or_default();
    let num = |f: &str| read(f).and_then(|v| v.parse::<f64>().ok());
    let hours_left = match (num("energy_now").or(num("charge_now")), num("power_now").or(num("current_now"))) {
        (Some(now), Some(rate)) if rate > 0.0 && status == "Discharging" => Some(now / rate),
        _ => None,
    };
    Some(Battery { percent, charging: status == "Charging", full: status == "Full", hours_left })
}

pub fn set_wifi(on: bool) -> bool {
    ok("nmcli", &["radio", "wifi", if on { "on" } else { "off" }])
}

pub fn set_bluetooth(on: bool) -> bool {
    if on {
        let _ = ok("rfkill", &["unblock", "bluetooth"]);
    }
    ok("bluetoothctl", &["power", if on { "on" } else { "off" }])
}

pub fn set_airplane(on: bool) -> bool {
    ok("rfkill", &[if on { "block" } else { "unblock" }, "all"])
}

pub fn set_dnd(on: bool) -> bool {
    ok("xfconf-query", &["-c", "xfce4-notifyd", "-p", "/do-not-disturb", "-n", "-t", "bool", "-s", if on { "true" } else { "false" }])
}

pub fn set_volume(percent: f64) {
    let _ = ok("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &format!("{}%", percent.round() as i64)]);
    let _ = ok("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "0"]);
}

pub fn toggle_mute() {
    let _ = ok("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "toggle"]);
}

pub fn set_brightness(percent: f64) {
    // never all the way to black
    let _ = ok("brightnessctl", &["-q", "set", &format!("{}%", percent.round().max(3.0) as i64)]);
}

#[derive(Clone, Debug)]
pub struct Network {
    pub name: String,
    pub signal: u32,
    pub secure: bool,
    pub active: bool,
    pub known: bool,
}

pub fn networks() -> Vec<Network> {
    let known: Vec<String> = run("nmcli", &["-t", "-f", "NAME,TYPE", "connection", "show"])
        .unwrap_or_default()
        .lines()
        .filter(|l| l.ends_with(":802-11-wireless"))
        .map(|l| unescape(l.trim_end_matches(":802-11-wireless")))
        .collect();
    // a fresh scan can take several seconds
    let mut nets: Vec<Network> = run_for(10, "nmcli", &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "auto"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            // split on ':' that isn't escaped
            let mut parts = Vec::new();
            let mut cur = String::new();
            let mut esc = false;
            for c in l.chars() {
                match (esc, c) {
                    (true, _) => { cur.push(c); esc = false; }
                    (false, '\\') => esc = true,
                    (false, ':') => parts.push(std::mem::take(&mut cur)),
                    _ => cur.push(c),
                }
            }
            parts.push(cur);
            let [in_use, name, signal, security] = <[String; 4]>::try_from(parts).ok()?;
            (!name.is_empty()).then(|| Network {
                known: known.contains(&name),
                active: in_use == "*",
                signal: signal.parse().unwrap_or(0),
                secure: !security.is_empty() && security != "--",
                name,
            })
        })
        .collect();
    nets.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    nets.dedup_by(|a, b| a.name == b.name);
    nets
}

/// Join a network. Known ones reconnect with their saved password.
pub fn connect(name: &str, password: Option<&str>) -> Result<(), String> {
    let mut args = vec!["device", "wifi", "connect", name];
    if let Some(p) = password {
        args.extend(["password", p]);
    }
    let out = Command::new("nmcli").args(&args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().trim_start_matches("Error: ").to_string())
    }
}
