//! Everything that's running, read from /proc, the cgroup tree, systemctl and docker, and
//! grouped the way a person thinks about it: apps you opened, services, and other processes.
//! Blocking: call from `gio::spawn_blocking`.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::know;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    Service,
    UserService,
    Container,
    Process,
}

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
    pub exe: Option<String>,
    pub user: String,
    pub memory: u64,
    pub cpu: f64,
    pub started: u64,
}

#[derive(Clone, Debug)]
pub struct Task {
    /// Stable key: `app:<desktop id>`, `svc:<unit>`, `user:<unit>`, `box:<container id>`, `pid:<pid>`.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    /// One plain line under the name.
    pub about: String,
    pub icon: String,
    pub memory: u64,
    /// Percent of the whole processor (all cores together).
    pub cpu: f64,
    /// Unix time the oldest process started.
    pub started: Option<u64>,
    pub procs: Vec<Proc>,
    /// The systemd unit, container id or desktop file behind it.
    pub unit: Option<String>,
    pub mine: bool,
    pub safety: know::Safety,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub cpu: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub disk_free: u64,
    pub disk_total: u64,
    pub uptime: u64,
    pub processes: usize,
}

pub struct Snapshot {
    pub summary: Summary,
    pub tasks: Vec<Task>,
}

/// Remembers the previous reading so processor use can be measured between two refreshes.
pub struct Collector {
    me: u32,
    users: HashMap<u32, String>,
    desktop: Desktop,
    page: u64,
    clk: u64,
    btime: u64,
    prev_cpu: Option<(u64, u64)>,
    prev_jiffies: HashMap<(u32, u64), u64>,
    docker: bool,
}

/// Where a process sits in the cgroup tree.
enum Place {
    Service(String, String),
    UserService(String, String),
    Container(String, String),
    AppScope(String),
    Loose,
}

impl Collector {
    pub fn new() -> Self {
        let me = fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(1000);
        let btime = read("/proc/stat")
            .lines()
            .find_map(|l| l.strip_prefix("btime ").and_then(|v| v.trim().parse().ok()))
            .unwrap_or(0);
        let docker = Command::new("docker").args(["ps", "-q"]).output().map(|o| o.status.success()).unwrap_or(false);
        Collector {
            me,
            users: users(),
            desktop: Desktop::load(),
            page: 4096,
            clk: 100,
            btime,
            prev_cpu: None,
            prev_jiffies: HashMap::new(),
            docker,
        }
    }

    pub fn collect(&mut self) -> Snapshot {
        // the processor as a whole
        let (busy, total) = cpu_times();
        let (cpu, span) = match self.prev_cpu {
            Some((pb, pt)) if total > pt => ((busy - pb.min(busy)) as f64 / (total - pt) as f64 * 100.0, total - pt),
            _ => (0.0, 0),
        };
        self.prev_cpu = Some((busy, total));

        // every process, placed in its group
        let mut groups: HashMap<String, Vec<(Proc, u32)>> = HashMap::new();
        let mut cgroups: HashMap<String, String> = HashMap::new();
        let mut scope_votes: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut loose: Vec<(Proc, u32, Option<String>, Option<String>)> = Vec::new();
        let mut jiffies = HashMap::new();
        let mut count = 0;

        for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
            let Some(raw) = read_opt(&format!("/proc/{pid}/stat")) else { continue };
            let (Some(open), Some(close)) = (raw.find('('), raw.rfind(')')) else { continue };
            let comm = raw[open + 1..close].to_string();
            let f: Vec<&str> = raw[close + 2..].split_whitespace().collect();
            if f.len() < 22 {
                continue;
            }
            let ppid: u32 = f[1].parse().unwrap_or(0);
            if pid == 2 || ppid == 2 {
                continue; // kernel threads
            }
            count += 1;
            let used: u64 = f[11].parse::<u64>().unwrap_or(0) + f[12].parse::<u64>().unwrap_or(0);
            let start: u64 = f[19].parse().unwrap_or(0);
            jiffies.insert((pid, start), used);
            let pcpu = match self.prev_jiffies.get(&(pid, start)) {
                Some(&before) if span > 0 => (used.saturating_sub(before)) as f64 / span as f64 * 100.0,
                _ => 0.0,
            };
            let uid = fs::metadata(format!("/proc/{pid}")).map(|m| m.uid()).unwrap_or(0);
            let mine = uid == self.me;
            let memory = private_memory(pid, self.page);
            let cmd = read_opt(&format!("/proc/{pid}/cmdline"))
                .map(|c| c.replace('\0', " ").trim().to_string())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| format!("[{comm}]"));
            let exe = fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.to_string_lossy().into_owned());
            let proc = Proc {
                pid,
                name: comm,
                cmd,
                exe,
                user: self.users.get(&uid).cloned().unwrap_or_else(|| uid.to_string()),
                memory,
                cpu: pcpu,
                started: self.btime + start / self.clk,
            };
            let cg = read_opt(&format!("/proc/{pid}/cgroup"))
                .and_then(|c| c.lines().find_map(|l| l.strip_prefix("0::").map(str::to_string)))
                .unwrap_or_default();
            match place(&cg) {
                Place::Service(unit, path) => {
                    cgroups.insert(format!("svc:{unit}"), path);
                    groups.entry(format!("svc:{unit}")).or_default().push((proc, uid));
                }
                Place::UserService(unit, path) => {
                    cgroups.insert(format!("user:{unit}"), path);
                    groups.entry(format!("user:{unit}")).or_default().push((proc, uid));
                }
                Place::Container(id, path) => {
                    cgroups.insert(format!("box:{id}"), path);
                    groups.entry(format!("box:{id}")).or_default().push((proc, uid));
                }
                Place::AppScope(scope) if mine => {
                    let hit = if know::process(&proc.name).is_none() { self.desktop.find(&proc) } else { None };
                    if let Some(id) = &hit {
                        *scope_votes.entry(scope.clone()).or_default().entry(id.clone()).or_default() += 1;
                    }
                    loose.push((proc, uid, Some(scope), hit));
                }
                _ => {
                    let hit = if mine && know::process(&proc.name).is_none() { self.desktop.find(&proc) } else { None };
                    loose.push((proc, uid, None, hit));
                }
            }
        }
        self.prev_jiffies = jiffies;

        // processes in an app's own scope belong to that app, even its helpers and terminals
        let scope_app: HashMap<String, String> = scope_votes
            .iter()
            .filter_map(|(scope, votes)| {
                self.desktop
                    .by_scope(scope)
                    .or_else(|| votes.iter().max_by_key(|(_, n)| **n).map(|(id, _)| id.clone()))
                    .map(|id| (scope.clone(), id))
            })
            .collect();
        let windows = windows();
        let mut by_app: HashMap<String, Vec<(Proc, u32)>> = HashMap::new();
        // an app registered under two menu entries (Chrome) is still one app: key by its name
        let mut by_name: HashMap<String, String> = HashMap::new();
        for (proc, uid, scope, hit) in loose {
            let app = scope.and_then(|s| scope_app.get(&s).cloned()).or(hit).map(|id| {
                let name = self.desktop.entry(&id).map(|e| e.name.clone()).unwrap_or_else(|| id.clone());
                by_name.entry(name).or_insert(id).clone()
            });
            match app {
                Some(id) => by_app.entry(id).or_default().push((proc, uid)),
                None if windows.contains_key(&proc.pid) => groups.entry(format!("win:{}", proc.name)).or_default().push((proc, uid)),
                None => groups.entry(format!("pid:{}", proc.pid)).or_default().push((proc, uid)),
            }
        }
        for (id, procs) in by_app {
            let shown = procs.iter().any(|(p, _)| windows.contains_key(&p.pid));
            groups.insert(format!("{}:{id}", if shown { "app" } else { "bg" }), procs);
        }

        // turn the groups into tasks, named from systemd, docker and the app menu
        let services = list_units(false);
        let user_services = list_units(true);
        let containers = if self.docker { containers() } else { HashMap::new() };
        let mut tasks = Vec::new();
        for (key, procs) in groups {
            let mine = procs.iter().all(|(_, uid)| *uid == self.me);
            let procs: Vec<Proc> = procs.into_iter().map(|(p, _)| p).collect();
            let (kind, rest) = key.split_once(':').unwrap();
            let rest = rest.to_string();
            let mut task = Task {
                id: key.clone(),
                kind: Kind::Process,
                name: String::new(),
                about: String::new(),
                icon: "application-x-executable".into(),
                memory: procs.iter().map(|p| p.memory).sum(),
                cpu: procs.iter().map(|p| p.cpu).sum(),
                started: procs.iter().map(|p| p.started).min(),
                procs,
                unit: Some(rest.clone()),
                mine,
                safety: know::Safety::Unknown,
            };
            match kind {
                "svc" | "user" => {
                    let user = kind == "user";
                    let Some(desc) = (if user { &user_services } else { &services }).get(&rest) else { continue };
                    task.kind = if user { Kind::UserService } else { Kind::Service };
                    let k = know::service(&rest);
                    let (name, about) = split_description(desc, &rest);
                    task.name = k.map(|k| k.name.to_string()).unwrap_or(name);
                    task.about = k.map(|k| k.about.to_string()).unwrap_or(about);
                    task.safety = k.map(|k| k.safety).unwrap_or(know::Safety::Unknown);
                    task.icon = k.map(|k| k.icon).unwrap_or("system-run-symbolic").into();
                    if let Some(mem) = cgroups.get(&key).and_then(|p| working_set(p)) {
                        task.memory = mem;
                    }
                }
                "box" => {
                    task.kind = Kind::Container;
                    let c = containers.get(&rest);
                    task.name = c.map(|c| c.name.clone()).unwrap_or_else(|| rest[..12.min(rest.len())].to_string());
                    task.about = c
                        .map(|c| match know::image(&c.image) {
                            Some(what) => format!("{what} · Docker container"),
                            None => format!("Docker container · {}", c.image),
                        })
                        .unwrap_or_else(|| "Docker container".into());
                    task.icon = "package-x-generic-symbolic".into();
                    task.safety = know::Safety::Optional;
                    if let Some(mem) = cgroups.get(&key).and_then(|p| working_set(p)) {
                        task.memory = mem;
                    }
                }
                "app" | "bg" => {
                    task.kind = if kind == "app" { Kind::App } else { Kind::Process };
                    let e = self.desktop.entry(&rest);
                    task.name = e.map(|e| e.name.clone()).unwrap_or_else(|| rest.clone());
                    let n = task.procs.len();
                    let what = e.map(|e| e.comment.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "App".into());
                    let what = if kind == "bg" { format!("{what} · in the background") } else { what };
                    task.about = if n == 1 { what } else { format!("{what} · {n} processes") };
                    task.icon = e.map(|e| e.icon.clone()).filter(|i| !i.is_empty()).unwrap_or(task.icon);
                    task.safety = know::app(&rest);
                }
                "win" => {
                    task.kind = Kind::App;
                    task.name = rest.clone();
                    task.about = short_cmd(&task.procs[0].cmd);
                    task.safety = know::Safety::Optional;
                    task.unit = None;
                }
                _ => {
                    let p = &task.procs[0];
                    let k = know::process(&p.name);
                    task.name = k.map(|k| k.name.to_string()).unwrap_or_else(|| p.name.clone());
                    task.about = k.map(|k| k.about.to_string()).unwrap_or_else(|| short_cmd(&p.cmd));
                    task.safety = k.map(|k| k.safety).unwrap_or(if p.pid == 1 { know::Safety::Needed } else { know::Safety::Unknown });
                    if let Some((name, about)) = know::bar_plugin(&p.name, &p.cmd) {
                        (task.name, task.about, task.safety) = (name, about, know::Safety::Needed);
                    }
                    if p.name == "systemd" && mine {
                        task.name = "Your services manager".into();
                        task.about = "Starts and watches your background services.".into();
                    }
                    task.icon = if mine { "utilities-terminal-symbolic" } else { "emblem-system-symbolic" }.into();
                    task.unit = None;
                }
            }
            tasks.push(task);
        }
        // services with nothing running in them (their work happens elsewhere) still count as running
        for (user, list) in [(false, &services), (true, &user_services)] {
            for (unit, desc) in list.iter() {
                let id = format!("{}:{unit}", if user { "user" } else { "svc" });
                if tasks.iter().any(|t| t.id == id) {
                    continue;
                }
                let k = know::service(unit);
                let (name, about) = split_description(desc, unit);
                tasks.push(Task {
                    id,
                    kind: if user { Kind::UserService } else { Kind::Service },
                    name: k.map(|k| k.name.to_string()).unwrap_or(name),
                    about: k.map(|k| k.about.to_string()).unwrap_or(about),
                    icon: k.map(|k| k.icon).unwrap_or("system-run-symbolic").into(),
                    memory: 0,
                    cpu: 0.0,
                    started: None,
                    procs: Vec::new(),
                    unit: Some(unit.clone()),
                    mine: user,
                    safety: k.map(|k| k.safety).unwrap_or(know::Safety::Unknown),
                });
            }
        }

        let mut merged: Vec<Task> = Vec::new();
        for t in tasks {
            match merged.iter_mut().find(|m| m.kind == Kind::App && t.kind == Kind::App && m.name == t.name) {
                Some(m) => {
                    m.memory += t.memory;
                    m.cpu += t.cpu;
                    m.started = m.started.min(t.started).or(m.started).or(t.started);
                    m.procs.extend(t.procs);
                    if let Some(i) = m.about.find(" · ") {
                        m.about.truncate(i);
                    }
                    m.about = format!("{} · {} processes", m.about, m.procs.len());
                }
                None => merged.push(t),
            }
        }
        let tasks = merged;

        let mem = meminfo();
        let (disk_total, disk_free) = disk();
        Snapshot {
            summary: Summary {
                cpu,
                mem_total: mem.get("MemTotal").copied().unwrap_or(0),
                mem_used: mem.get("MemTotal").copied().unwrap_or(0).saturating_sub(mem.get("MemAvailable").copied().unwrap_or(0)),
                swap_total: mem.get("SwapTotal").copied().unwrap_or(0),
                swap_used: mem.get("SwapTotal").copied().unwrap_or(0).saturating_sub(mem.get("SwapFree").copied().unwrap_or(0)),
                disk_free,
                disk_total,
                uptime: read("/proc/uptime").split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0),
                processes: count,
            },
            tasks,
        }
    }
}

fn place(cg: &str) -> Place {
    let parts: Vec<&str> = cg.split('/').filter(|s| !s.is_empty()).collect();
    if parts.first() == Some(&"system.slice") {
        if let Some(last) = parts.last() {
            if let Some(id) = last.strip_prefix("docker-").and_then(|s| s.strip_suffix(".scope")) {
                return Place::Container(id.to_string(), cg.to_string());
            }
        }
        if let Some(i) = parts.iter().position(|p| p.ends_with(".service")) {
            return Place::Service(parts[i].to_string(), format!("/{}", parts[..=i].join("/")));
        }
    }
    if let Some(at) = parts.iter().position(|p| p.starts_with("user@") && p.ends_with(".service")) {
        for (i, p) in parts.iter().enumerate().skip(at + 1) {
            if p.ends_with(".service") {
                return Place::UserService(p.to_string(), format!("/{}", parts[..=i].join("/")));
            }
            if p.starts_with("app-") && p.ends_with(".scope") {
                return Place::AppScope(p.to_string());
            }
        }
    }
    Place::Loose
}

/// Memory a service really holds: everything charged to it minus file cache it could drop.
fn working_set(cgroup: &str) -> Option<u64> {
    let dir = Path::new("/sys/fs/cgroup").join(cgroup.trim_start_matches('/'));
    let current: u64 = read_opt(dir.join("memory.current").to_str()?)?.trim().parse().ok()?;
    let inactive: u64 = read_opt(dir.join("memory.stat").to_str()?)?
        .lines()
        .find_map(|l| l.strip_prefix("inactive_file ").and_then(|v| v.trim().parse().ok()))
        .unwrap_or(0);
    Some(current.saturating_sub(inactive))
}

/// Memory a process holds for itself: resident pages minus those shared with others (libraries,
/// mapped files). The same number GNOME System Monitor shows, and cheap to read for every process.
fn private_memory(pid: u32, page: u64) -> u64 {
    let statm = read(&format!("/proc/{pid}/statm"));
    let v: Vec<u64> = statm.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    match (v.get(1), v.get(2)) {
        (Some(resident), Some(shared)) => resident.saturating_sub(*shared) * page,
        _ => 0,
    }
}

/// Processes that own a window you can see (not the bar or the desktop), with its title.
fn windows() -> HashMap<u32, String> {
    let out = Command::new("wmctrl").arg("-lp").output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let desktop: i32 = f.get(1)?.parse().ok()?;
            let pid: u32 = f.get(2)?.parse().ok()?;
            (desktop >= 0 && pid > 0).then(|| (pid, f.get(4..).map(|t| t.join(" ")).unwrap_or_default()))
        })
        .collect()
}

fn cpu_times() -> (u64, u64) {
    let stat = read("/proc/stat");
    let line = stat.lines().next().unwrap_or("");
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 5 {
        return (0, 0);
    }
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v[4];
    (total - idle, total)
}

fn meminfo() -> HashMap<String, u64> {
    read("/proc/meminfo")
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let kb: u64 = v.trim().trim_end_matches("kB").trim().parse().ok()?;
            Some((k.to_string(), kb * 1024))
        })
        .collect()
}

fn disk() -> (u64, u64) {
    let out = Command::new("df").args(["-B1", "--output=size,avail", "/"]).output().ok();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let v: Vec<u64> = text.lines().nth(1).unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0))
}

/// Running services: unit -> description.
fn list_units(user: bool) -> HashMap<String, String> {
    let mut cmd = Command::new("systemctl");
    if user {
        cmd.arg("--user");
    }
    let out = cmd
        .args(["list-units", "--type=service", "--state=running", "--no-legend", "--plain", "--no-pager"])
        .output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let unit = it.next()?.to_string();
            let desc: Vec<&str> = it.skip(3).collect();
            Some((unit, desc.join(" ")))
        })
        .collect()
}

pub struct Container {
    pub name: String,
    pub image: String,
}

fn containers() -> HashMap<String, Container> {
    let out = Command::new("docker")
        .args(["ps", "--no-trunc", "--format", "{{.ID}}|{{.Names}}|{{.Image}}"])
        .output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut it = l.split('|');
            let id = it.next()?.to_string();
            Some((id, Container { name: it.next()?.to_string(), image: it.next().unwrap_or("").to_string() }))
        })
        .collect()
}

fn users() -> HashMap<u32, String> {
    read("/etc/passwd")
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            Some((f.get(2)?.parse().ok()?, f.first()?.to_string()))
        })
        .collect()
}

/// "Jungey Control - the Bluetooth link to..." -> ("Jungey Control", "The Bluetooth link to...");
/// a description with no dash keeps the service's file name as its second line.
fn split_description(desc: &str, unit: &str) -> (String, String) {
    for dash in [" - ", " — ", " – ", ": "] {
        if let Some((name, about)) = desc.split_once(dash) {
            if !name.trim().is_empty() && !about.trim().is_empty() {
                return (tidy(name.trim()), tidy(about.trim()));
            }
        }
    }
    (tidy(desc), format!("Background service ({})", unit.trim_end_matches(".service")))
}

/// "Avahi mDNS/DNS-SD Stack" stays; "foo-bar daemon" becomes "Foo-bar daemon".
fn tidy(desc: &str) -> String {
    let mut c = desc.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The program and its first arguments, without long paths.
pub fn short_cmd(cmd: &str) -> String {
    let mut words = cmd.split_whitespace();
    let first = words.next().unwrap_or("");
    let prog = first.rsplit('/').next().unwrap_or(first);
    let rest: Vec<&str> = words.take(3).map(|w| w.rsplit('/').next().unwrap_or(w)).collect();
    let s = std::iter::once(prog).chain(rest).collect::<Vec<_>>().join(" ");
    if s.chars().count() > 70 {
        s.chars().take(69).collect::<String>() + "…"
    } else {
        s
    }
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn read_opt(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

// --- the app menu, to name and draw apps the way the menu does --------------------------------

pub struct Entry {
    pub name: String,
    pub icon: String,
    pub comment: String,
}

struct Desktop {
    entries: HashMap<String, Entry>,
    keys: HashMap<String, String>,
}

/// Program names too general to tell apps apart.
const GENERIC: &[&str] = &[
    "sh", "bash", "zsh", "env", "python", "python3", "perl", "ruby", "node", "java", "electron", "flatpak", "snap",
    "gjs", "pkexec", "sudo", "xdg-open", "exo-open", "bin", "sbin", "usr", "local", "lib", "libexec", "share", "opt",
    "games", "app", "apps",
];

impl Desktop {
    fn load() -> Self {
        let mut d = Desktop { entries: HashMap::new(), keys: HashMap::new() };
        let home = std::env::var("HOME").unwrap_or_default();
        let dirs = [
            format!("{home}/.local/share/applications"),
            "/usr/local/share/applications".into(),
            "/usr/share/applications".into(),
            "/var/lib/snapd/desktop/applications".into(),
        ];
        for dir in dirs {
            for f in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let id = path.file_stem().unwrap().to_string_lossy().into_owned();
                if d.entries.contains_key(&id) {
                    continue; // the user's own copy wins
                }
                if let Some((entry, keys)) = parse_desktop(&id, &path) {
                    for k in keys {
                        d.keys.entry(k).or_insert_with(|| id.clone());
                    }
                    d.entries.insert(id, entry);
                }
            }
        }
        for id in d.entries.keys() {
            d.keys.insert(id.to_lowercase(), id.clone());
        }
        d
    }

    fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// Which app a process belongs to, if it's one from the menu.
    fn find(&self, p: &Proc) -> Option<String> {
        let mut tries = Vec::new();
        if let Some(exe) = &p.exe {
            let path = Path::new(exe);
            tries.push(base(path));
            if let Some(parent) = path.parent() {
                tries.push(base(parent));
            }
        }
        tries.push(p.name.to_lowercase());
        if let Some(first) = p.cmd.split_whitespace().next() {
            tries.push(base(Path::new(first)));
        }
        tries
            .into_iter()
            .filter(|k| !generic(k))
            .find_map(|k| self.keys.get(&k).cloned())
    }

    /// `app-com.google.Chrome-1488.scope` -> the desktop file named after it, if there is one.
    fn by_scope(&self, scope: &str) -> Option<String> {
        let inner = scope.strip_prefix("app-")?.strip_suffix(".scope")?.replace("\\x2d", "-");
        let inner = match inner.rsplit_once('-') {
            Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => head.to_string(),
            _ => inner,
        };
        let inner = ["gnome-", "xfce-", "kde-", "flatpak-"]
            .iter()
            .find_map(|p| inner.strip_prefix(p))
            .unwrap_or(&inner)
            .to_string();
        if self.entries.contains_key(&inner) {
            return Some(inner);
        }
        self.keys.get(&inner.to_lowercase()).cloned()
    }
}

fn generic(key: &str) -> bool {
    key.is_empty()
        || GENERIC.contains(&key)
        || ["python", "perl", "ruby", "node", "java", "php", "lua", "electron"].iter().any(|p| key.starts_with(p))
}

fn base(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn parse_desktop(id: &str, path: &PathBuf) -> Option<(Entry, Vec<String>)> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_main = false;
    let mut f: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            f.entry(k.trim()).or_insert(v.trim());
        }
    }
    if f.get("Type") != Some(&"Application") || f.get("Hidden") == Some(&"true") {
        return None;
    }
    let mut keys = vec![id.to_lowercase()];
    if let Some(last) = id.rsplit('.').next() {
        keys.push(last.to_lowercase());
    }
    if let Some(wm) = f.get("StartupWMClass") {
        keys.push(wm.to_lowercase());
    }
    if let Some(exec) = f.get("Exec") {
        let prog = exec
            .split_whitespace()
            .map(|w| w.trim_matches('"'))
            .find(|w| *w != "env" && !w.contains('='))
            .unwrap_or("");
        if !prog.is_empty() {
            let p = Path::new(prog);
            keys.push(base(p));
            let full = if p.is_absolute() { Some(p.to_path_buf()) } else { which(prog) };
            if let Some(real) = full.and_then(|f| fs::canonicalize(f).ok()) {
                keys.push(base(&real));
                if let Some(parent) = real.parent() {
                    keys.push(base(parent));
                }
            }
        }
    }
    keys.retain(|k| !generic(k));
    let entry = Entry {
        name: f.get("Name").unwrap_or(&id).to_string(),
        icon: f.get("Icon").unwrap_or(&"").to_string(),
        comment: f.get("GenericName").or(f.get("Comment")).unwrap_or(&"").to_string(),
    };
    Some((entry, keys))
}

fn which(prog: &str) -> Option<PathBuf> {
    std::env::var("PATH").ok()?.split(':').map(|d| Path::new(d).join(prog)).find(|p| p.exists())
}
