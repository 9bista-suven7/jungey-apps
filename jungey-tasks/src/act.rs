//! Stopping and restarting things, switching start-at-boot, and the details behind a row.
//! Blocking: call from `gio::spawn_blocking`. System services and other people's processes
//! go through polkit, which shows its own password window.

use std::process::Command;

use crate::collect::{Kind, Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Stop,
    ForceStop,
    Restart,
    Boot(bool),
}

/// What an action needs to know about a task (a `Task` without the live numbers).
#[derive(Clone, Debug)]
pub struct Target {
    pub kind: Kind,
    pub unit: Option<String>,
    pub pids: Vec<u32>,
    pub mine: bool,
    pub name: String,
}

impl From<&Task> for Target {
    fn from(t: &Task) -> Self {
        Target { kind: t.kind, unit: t.unit.clone(), pids: t.procs.iter().map(|p| p.pid).collect(), mine: t.mine, name: t.name.clone() }
    }
}

pub fn run(t: &Target, action: Action) -> Result<String, String> {
    let unit = t.unit.clone().unwrap_or_default();
    match (t.kind, action) {
        (Kind::Service | Kind::UserService, _) => {
            let verb = match action {
                Action::Stop | Action::ForceStop => "stop",
                Action::Restart => "restart",
                Action::Boot(true) => "enable",
                Action::Boot(false) => "disable",
            };
            let mut args = Vec::new();
            if t.kind == Kind::UserService {
                args.push("--user");
            }
            args.extend([verb, unit.as_str()]);
            exec("systemctl", &args)?;
            Ok(done(&t.name, action))
        }
        (Kind::Container, _) => {
            let policy;
            let args: Vec<&str> = match action {
                Action::Stop => vec!["stop", unit.as_str()],
                Action::ForceStop => vec!["kill", unit.as_str()],
                Action::Restart => vec!["restart", unit.as_str()],
                Action::Boot(on) => {
                    policy = format!("--restart={}", if on { remembered_policy(&unit) } else { remember_policy(&unit).to_string() });
                    vec!["update", policy.as_str(), unit.as_str()]
                }
            };
            exec("docker", &args)?;
            Ok(done(&t.name, action))
        }
        (_, Action::Stop | Action::ForceStop) => {
            let signal = if action == Action::Stop { "-TERM" } else { "-KILL" };
            let pids: Vec<String> = t.pids.iter().map(|p| p.to_string()).collect();
            let mut args = vec![signal];
            args.extend(pids.iter().map(String::as_str));
            if t.mine {
                exec("kill", &args)?;
            } else {
                let mut with_kill = vec!["kill"];
                with_kill.extend(args);
                exec("pkexec", &with_kill)?;
            }
            Ok(done(&t.name, action))
        }
        _ => Err("That can't be done to this one.".into()),
    }
}

/// Where a container's own restart setting is kept while start-at-boot is switched off, so
/// switching it back on restores exactly what it had (always, unless-stopped, on-failure...).
fn policy_file(id: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(&std::env::var("HOME").unwrap_or_default()).join(".cache/jungey-tasks/restart");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(id)
}

/// Save the current setting and return "no" (the value that switches start-at-boot off).
fn remember_policy(id: &str) -> &'static str {
    let out = Command::new("docker").args(["inspect", "-f", "{{.HostConfig.RestartPolicy.Name}}", id]).output();
    if let Ok(o) = out {
        let policy = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !policy.is_empty() && policy != "no" {
            let _ = std::fs::write(policy_file(id), policy);
        }
    }
    "no"
}

fn remembered_policy(id: &str) -> String {
    let saved = std::fs::read_to_string(policy_file(id)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    saved.unwrap_or_else(|| "unless-stopped".into())
}

fn done(name: &str, action: Action) -> String {
    match action {
        Action::Stop | Action::ForceStop => format!("Stopped {name}"),
        Action::Restart => format!("Restarted {name}"),
        Action::Boot(true) => format!("{name} will start with the computer"),
        Action::Boot(false) => format!("{name} won't start with the computer any more"),
    }
}

fn exec(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd).args(args).output().map_err(|e| format!("Couldn't run {cmd}: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    // polkit: the password window was closed, or the password was wrong
    if out.status.code() == Some(126) || err.contains("Interactive authentication required") || err.contains("Not authorized") {
        return Err("Not allowed: the password wasn't given.".into());
    }
    if err.contains("Operation not permitted") {
        return Err("Not allowed: it belongs to the system.".into());
    }
    Err(err.lines().last().unwrap_or("It didn't work.").to_string())
}

// --- details -----------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Details {
    /// (label, value) pairs for the "More details" list.
    pub rows: Vec<(String, String)>,
    /// Starts with the computer: Some(on) when it can be changed here.
    pub boot: Option<bool>,
    /// A sentence about start-up when it can't be changed (e.g. started on demand).
    pub boot_note: Option<String>,
}

pub fn details(t: &Task) -> Details {
    let mut d = Details::default();
    let unit = t.unit.clone().unwrap_or_default();
    match t.kind {
        Kind::Service | Kind::UserService => {
            let mut cmd = Command::new("systemctl");
            if t.kind == Kind::UserService {
                cmd.arg("--user");
            }
            let props = "Id,Description,UnitFileState,FragmentPath,MainPID,Restart,Type,User,ExecMainStartTimestamp,Documentation,TasksCurrent";
            let text = cmd.args(["show", &unit, "-p", props]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
            d.rows.push(("Service name".into(), get("Id")));
            d.rows.push(("System description".into(), get("Description")));
            match get("UnitFileState").as_str() {
                "enabled" => d.boot = Some(true),
                "disabled" => d.boot = Some(false),
                "static" | "indirect" => d.boot_note = Some("Starts when another part of the system asks for it.".into()),
                "generated" | "transient" => d.boot_note = Some("Created automatically by the system.".into()),
                other if !other.is_empty() => d.boot_note = Some(format!("Start-up setting: {other}")),
                _ => {}
            }
            let restart = get("Restart");
            d.rows.push((
                "If it crashes".into(),
                match restart.as_str() {
                    "no" | "" => "Stays stopped".into(),
                    "always" => "Always starts again by itself".into(),
                    "on-failure" | "on-abnormal" | "on-abort" => "Starts again after a crash".into(),
                    other => other.to_string(),
                },
            ));
            let user = get("User");
            d.rows.push((
                "Runs as".into(),
                if !user.is_empty() {
                    user
                } else if t.kind == Kind::UserService {
                    "You".into()
                } else {
                    "root (administrator)".into()
                },
            ));
            let pid = get("MainPID");
            if pid != "0" && !pid.is_empty() {
                d.rows.push(("Main process".into(), format!("PID {pid}")));
            }
            push(&mut d, "Started", get("ExecMainStartTimestamp"));
            push(&mut d, "Kind", get("Type"));
            push(&mut d, "Set up in", get("FragmentPath"));
            push(&mut d, "Help", get("Documentation"));
        }
        Kind::Container => {
            let fmt = "{{.Name}}|{{.Config.Image}}|{{.State.StartedAt}}|{{.HostConfig.RestartPolicy.Name}}|{{index .Config.Labels \"com.docker.compose.project\"}}|{{index .Config.Labels \"com.docker.compose.project.working_dir\"}}|{{.Path}} {{join .Args \" \"}}";
            let text = Command::new("docker").args(["inspect", "-f", fmt, &unit]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            let f: Vec<&str> = text.trim().split('|').collect();
            let at = |i: usize| f.get(i).copied().unwrap_or("").trim().to_string();
            d.boot = Some(!matches!(at(3).as_str(), "no" | ""));
            d.rows.push(("Container".into(), at(0).trim_start_matches('/').to_string()));
            d.rows.push(("Image".into(), at(1)));
            push(&mut d, "Project", at(4));
            push(&mut d, "Project folder", at(5));
            push(&mut d, "Started", at(2).split('.').next().unwrap_or("").replace('T', " "));
            d.rows.push(("Container ID".into(), unit.chars().take(12).collect()));
            let ports = Command::new("docker").args(["port", &unit]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            let ports: Vec<String> = ports.lines().map(|l| l.replace(" -> ", " → ")).collect();
            push(&mut d, "Ports", ports.join("\n"));
            push(&mut d, "Command", at(6));
        }
        Kind::App => {
            if let Some(p) = t.procs.iter().min_by_key(|p| p.started) {
                push(&mut d, "Program", p.exe.clone().unwrap_or_default());
                d.rows.push(("Command".into(), p.cmd.clone()));
                d.rows.push(("Main process".into(), format!("PID {}", p.pid)));
            }
            push(&mut d, "Menu entry", unit);
        }
        Kind::Process => {
            if let Some(p) = t.procs.first() {
                d.rows.push(("Process".into(), format!("PID {}", p.pid)));
                d.rows.push(("Runs as".into(), if p.user == "root" { "root (administrator)".into() } else { p.user.clone() }));
                push(&mut d, "Program", p.exe.clone().unwrap_or_default());
                d.rows.push(("Command".into(), p.cmd.clone()));
            }
        }
    }
    d
}

fn push(d: &mut Details, label: &str, value: String) {
    if !value.trim().is_empty() && value != "n/a" {
        d.rows.push((label.into(), value));
    }
}

/// The last messages it wrote, newest last.
pub fn log(t: &Task) -> String {
    let unit = t.unit.clone().unwrap_or_default();
    let out = match t.kind {
        Kind::Service => Command::new("journalctl").args(["-u", &unit, "-n", "40", "--no-pager", "-o", "short"]).output(),
        Kind::UserService => Command::new("journalctl").args(["--user", "-u", &unit, "-n", "40", "--no-pager", "-o", "short"]).output(),
        Kind::Container => Command::new("docker").args(["logs", "--tail", "40", &unit]).output(),
        _ => return String::new(),
    };
    match out {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&o.stderr));
            let text = text.trim().to_string();
            if text.is_empty() || text == "-- No entries --" {
                "Nothing written recently.".into()
            } else {
                text
            }
        }
        Err(e) => format!("Couldn't read the messages: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn alive(pid: u32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).map(|s| !s.contains(") Z ")).unwrap_or(false)
    }

    #[test]
    fn stops_own_process() {
        let mut child = Command::new("sleep").arg("600").spawn().unwrap();
        let pid = child.id();
        let t = Target { kind: Kind::Process, unit: None, pids: vec![pid], mine: true, name: "sleep".into() };
        assert_eq!(run(&t, Action::Stop).unwrap(), "Stopped sleep");
        let _ = child.wait();
        assert!(!alive(pid));
    }

    #[test]
    fn stops_and_restarts_user_service() {
        let unit = "jungey-tasks-test.service";
        let _ = Command::new("systemctl").args(["--user", "stop", unit]).status();
        assert!(Command::new("systemd-run").args(["--user", "--unit", unit, "sleep", "600"]).status().unwrap().success());
        sleep(Duration::from_millis(300));
        let t = Target { kind: Kind::UserService, unit: Some(unit.into()), pids: vec![], mine: true, name: "Test".into() };
        assert_eq!(run(&t, Action::Restart).unwrap(), "Restarted Test");
        assert_eq!(run(&t, Action::Stop).unwrap(), "Stopped Test");
        let state = Command::new("systemctl").args(["--user", "is-active", unit]).output().unwrap();
        assert_ne!(String::from_utf8_lossy(&state.stdout).trim(), "active");
    }
}
