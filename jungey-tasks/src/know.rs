//! Plain words for what's running: a friendly name, one line on what it does, and whether
//! stopping it is harmless or will break something.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Safety {
    /// The computer or the desktop needs it; ask before stopping.
    Needed,
    /// Fine to stop; it only matters if you use what it does.
    Optional,
    Unknown,
}

pub struct Known {
    pub name: &'static str,
    pub about: &'static str,
    pub safety: Safety,
    pub icon: &'static str,
}

const fn k(name: &'static str, about: &'static str, safety: Safety, icon: &'static str) -> Known {
    Known { name, about, safety, icon }
}

use Safety::{Needed, Optional};

/// Services, by unit name without ".service" (a trailing "@" matches every instance).
const SERVICES: &[(&str, Known)] = &[
    // system
    ("accounts-daemon", k("User accounts", "Knows who can log in to this computer.", Needed, "system-users-symbolic")),
    ("avahi-daemon", k("Network device finder", "Finds printers and other devices on your network. Usually safe to stop.", Optional, "network-workgroup-symbolic")),
    ("bluetooth", k("Bluetooth", "Lets you use Bluetooth headphones, mice and phones.", Optional, "bluetooth-active-symbolic")),
    ("colord", k("Screen colours", "Manages colour profiles for screens and printers.", Optional, "preferences-color-symbolic")),
    ("containerd", k("Container engine", "Runs the containers behind Docker. Stopping it stops your containers.", Optional, "package-x-generic-symbolic")),
    ("cron", k("Scheduled jobs", "Runs tasks at set times, like nightly clean-ups.", Optional, "alarm-symbolic")),
    ("cups", k("Printing", "Sends documents to printers. Safe to stop if you don't print.", Optional, "printer-symbolic")),
    ("cups-browsed", k("Printer finder", "Looks for shared printers on the network. Safe to stop.", Optional, "printer-symbolic")),
    ("dbus", k("System messages", "Lets the parts of the system talk to each other. Nothing works without it.", Needed, "network-transmit-receive-symbolic")),
    ("docker", k("Docker", "Runs your Docker containers. Stopping it stops all of them.", Optional, "package-x-generic-symbolic")),
    ("fwupd", k("Firmware updates", "Checks for updates to the laptop's built-in firmware.", Optional, "software-update-available-symbolic")),
    ("getty@", k("Text login", "The backup text login screen (Ctrl+Alt+F1). Harmless to leave running.", Optional, "utilities-terminal-symbolic")),
    ("kerneloops", k("Crash reports", "Sends crash reports to Ubuntu. Safe to stop.", Optional, "dialog-warning-symbolic")),
    ("lightdm", k("Login screen and desktop", "Shows the login screen and runs your desktop. Stopping it logs you out at once.", Needed, "computer-symbolic")),
    ("ModemManager", k("Mobile modems", "Only needed for SIM-card internet sticks. Usually safe to stop.", Optional, "network-cellular-symbolic")),
    ("mongod", k("MongoDB", "A database server.", Optional, "drive-multidisk-symbolic")),
    ("NetworkManager", k("Wi-Fi and internet", "Keeps you connected to Wi-Fi and the internet.", Needed, "network-wireless-symbolic")),
    ("ollama", k("Ollama", "Runs AI models on this computer. Uses a lot of memory while a model is loaded.", Optional, "applications-science-symbolic")),
    ("polkit", k("Password prompts", "Asks for your password when something needs administrator rights.", Needed, "dialog-password-symbolic")),
    ("postgresql", k("PostgreSQL", "A database server.", Optional, "drive-multidisk-symbolic")),
    ("power-profiles-daemon", k("Power modes", "Switches between power saving and performance.", Optional, "power-profile-balanced-symbolic")),
    ("preload", k("App preloader", "Learns which apps you use and loads them ahead of time so they open faster.", Optional, "system-run-symbolic")),
    ("rtkit-daemon", k("Smooth sound", "Gives sound priority so it doesn't stutter.", Optional, "audio-speakers-symbolic")),
    ("snapd", k("Snap apps", "Installs and updates Snap apps.", Optional, "system-software-install-symbolic")),
    ("ssh", k("Remote login (SSH)", "Lets other computers log in to this one.", Optional, "network-server-symbolic")),
    ("switcheroo-control", k("Graphics switching", "Only useful on laptops with two graphics chips. Usually safe to stop.", Optional, "video-display-symbolic")),
    ("systemd-journald", k("System diary", "Keeps the record of events and errors.", Needed, "x-office-document-symbolic")),
    ("systemd-logind", k("Logins, lid and power button", "Handles logging in, closing the lid and the power button.", Needed, "system-shutdown-symbolic")),
    ("systemd-resolved", k("Website name lookup", "Turns website names into addresses. The internet needs it.", Needed, "network-server-symbolic")),
    ("systemd-timesyncd", k("Clock sync", "Keeps the clock right using the internet.", Optional, "preferences-system-time-symbolic")),
    ("systemd-udevd", k("Device setup", "Sets up USB drives, keyboards and other devices when you plug them in.", Needed, "drive-removable-media-symbolic")),
    ("thermald", k("Heat control", "Keeps the processor from overheating.", Needed, "sensors-temperature-symbolic")),
    ("touchegg", k("Touchpad gestures", "Powers your two- and three-finger gestures.", Optional, "input-touchpad-symbolic")),
    ("udisks2", k("Disks and USB drives", "Opens USB drives and other disks.", Needed, "drive-harddisk-symbolic")),
    ("unattended-upgrades", k("Automatic updates", "Installs security updates in the background.", Optional, "software-update-available-symbolic")),
    ("upower", k("Battery and power", "Reports the battery level and charging.", Needed, "battery-good-symbolic")),
    ("user@", k("Your session", "Runs your personal background services. Stopping it logs you out.", Needed, "avatar-default-symbolic")),
    ("whoopsie", k("Crash reports", "Sends crash reports to Ubuntu. Safe to stop.", Optional, "dialog-warning-symbolic")),
    ("wpa_supplicant", k("Wi-Fi security", "Handles Wi-Fi passwords and encryption.", Needed, "network-wireless-encrypted-symbolic")),
    // yours
    ("at-spi-dbus-bus", k("Accessibility", "Lets screen readers and other helpers read your apps.", Optional, "preferences-desktop-accessibility-symbolic")),
    ("dconf", k("Settings storage", "Saves your apps' settings.", Needed, "preferences-system-symbolic")),
    ("filter-chain", k("Sound filters", "Extra audio processing for your speakers.", Optional, "audio-speakers-symbolic")),
    ("gnome-keyring-daemon", k("Password keyring", "Keeps saved passwords (Wi-Fi, Chrome) unlocked while you're logged in.", Needed, "dialog-password-symbolic")),
    ("gpg-agent", k("Encryption keys", "Remembers your GPG key passphrase for a while.", Optional, "channel-secure-symbolic")),
    ("gvfs-afc-volume-monitor", k("iPhone and iPad in Files", "Shows Apple devices in the file manager.", Optional, "phone-symbolic")),
    ("gvfs-daemon", k("Files helper", "Lets the file manager open the trash, network folders and phones.", Needed, "folder-symbolic")),
    ("gvfs-goa-volume-monitor", k("Online accounts in Files", "Shows online accounts in the file manager.", Optional, "folder-remote-symbolic")),
    ("gvfs-gphoto2-volume-monitor", k("Cameras in Files", "Shows digital cameras in the file manager.", Optional, "camera-photo-symbolic")),
    ("gvfs-metadata", k("Folder view memory", "Remembers how you arranged each folder.", Optional, "folder-symbolic")),
    ("gvfs-mtp-volume-monitor", k("Android phones in Files", "Shows Android phones in the file manager.", Optional, "phone-symbolic")),
    ("gvfs-udisks2-volume-monitor", k("Drives in Files", "Shows USB drives and disks in the file manager.", Needed, "drive-removable-media-symbolic")),
    ("obex", k("Bluetooth file transfer", "Sends and receives files over Bluetooth.", Optional, "bluetooth-active-symbolic")),
    ("pipewire", k("Sound", "Plays all sound and records from the microphone.", Needed, "audio-speakers-symbolic")),
    ("pipewire-pulse", k("Sound for older apps", "Lets apps made for PulseAudio play sound.", Needed, "audio-speakers-symbolic")),
    ("wireplumber", k("Sound devices", "Picks your speakers, headphones and microphone.", Needed, "audio-headphones-symbolic")),
    ("xdg-desktop-portal", k("App helpers", "Lets apps open file pickers, share the screen and send notifications.", Needed, "application-x-addon-symbolic")),
    ("xdg-desktop-portal-gtk", k("File pickers", "The open and save windows apps use.", Needed, "document-open-symbolic")),
    ("xdg-document-portal", k("Shared files for apps", "Lets sandboxed apps open files you pick.", Optional, "document-open-symbolic")),
    ("xdg-permission-store", k("App permissions", "Remembers what each app is allowed to do.", Optional, "security-medium-symbolic")),
    ("xfce4-notifyd", k("Notifications", "Shows notification bubbles.", Needed, "preferences-system-notifications-symbolic")),
];

/// Loose processes worth naming (by program name).
const PROCESSES: &[(&str, Known)] = &[
    ("systemd", k("System start-up", "The first program; it starts and watches everything else.", Needed, "")),
    ("Xorg", k("Display server", "Draws everything on the screen.", Needed, "")),
    ("lightdm", k("Login screen", "Started your desktop when you logged in.", Needed, "")),
    ("xfce4-session", k("Desktop session", "Holds your desktop together. Stopping it logs you out.", Needed, "")),
    ("xfwm4", k("Window manager", "Moves, resizes and draws window frames.", Needed, "")),
    ("xfce4-panel", k("The bar", "The bar along the bottom of the screen.", Needed, "")),
    ("xfce4-power-man", k("Power manager", "Handles sleep, the lid, screen dimming and low battery.", Needed, "")),
    ("light-locker", k("Screen lock", "Locks the screen when the computer sleeps or you step away.", Optional, "")),
    ("blueman-applet", k("Bluetooth tray icon", "The Bluetooth icon and its pairing pop-ups.", Optional, "")),
    ("blueman-tray", k("Bluetooth tray icon", "The Bluetooth icon on the bar.", Optional, "")),
    ("nm-applet", k("Network tray icon", "The Wi-Fi icon and its pop-ups.", Optional, "")),
    ("(sd-pam)", k("Login helper", "Part of your login session.", Needed, "")),
    ("xfconfd", k("Desktop settings store", "Saves your desktop settings.", Needed, "")),
    ("xfdesktop", k("Desktop and wallpaper", "Draws the wallpaper and desktop icons.", Needed, "")),
    ("xfsettingsd", k("Desktop settings", "Applies your theme, fonts and keyboard settings.", Needed, "")),
    ("picom", k("Window effects", "Rounded corners, shadows, blur and animations.", Optional, "")),
    ("polkit-gnome-au", k("Password window", "Shows the window that asks for your password.", Needed, "")),
    ("jungey-center", k("Jungey Center", "Wi-Fi, sound and battery behind the icon on the bar.", Optional, "")),
    ("jungey-launchpa", k("Jungey Launchpad", "The app grid, kept ready so it opens instantly.", Optional, "")),
    ("jungey-tasks", k("Jungey Tasks", "This app.", Optional, "")),
    ("touchegg", k("Touchpad gestures", "Turns your swipes into actions.", Optional, "")),
    ("docker-proxy", k("Docker network link", "Connects a container's port to this computer.", Optional, "")),
    ("containerd-shim", k("Container keeper", "Keeps one Docker container running.", Optional, "")),
    ("ssh-agent", k("SSH keys", "Remembers your SSH key passphrase for a while.", Optional, "")),
    ("gpg-agent", k("Encryption keys", "Remembers your GPG key passphrase for a while.", Optional, "")),
];

/// A piece of the bar: `panel-25-dockli` running `wrapper-2.0 libdocklike.so ...`.
pub fn bar_plugin(name: &str, cmd: &str) -> Option<(String, String)> {
    if !name.starts_with("panel-") || !cmd.contains("wrapper-2.0") {
        return None;
    }
    let lib = cmd.split_whitespace().find(|w| w.ends_with(".so"))?;
    let lib = lib.rsplit('/').next()?.trim_start_matches("lib").trim_end_matches(".so");
    let what = match lib {
        "docklike" => "app icons",
        "systray" => "tray icons",
        "xfce4powermanager" => "battery",
        "pulseaudio" => "sound",
        "clock" | "datetime" => "clock",
        "whiskermenu" | "applicationsmenu" => "menu",
        "notification-plugin" => "notifications",
        "actions" => "log out and power",
        other => other,
    };
    Some((format!("Bar: {what}"), "Part of the bar along the bottom of the screen.".into()))
}

/// Apps whose closing takes the desktop down with it.
const DESKTOP_APPS: &[&str] = &["xfce4-panel", "xfdesktop", "xfce-settings-manager"];

pub fn service(unit: &str) -> Option<&'static Known> {
    let name = unit.strip_suffix(".service").unwrap_or(unit);
    SERVICES
        .iter()
        .find(|(key, _)| {
            if let Some(prefix) = key.strip_suffix('@') {
                name.starts_with(&format!("{prefix}@"))
            } else {
                *key == name
            }
        })
        .map(|(_, k)| k)
}

/// What a container is, from its image (`mongo:7.0` -> "MongoDB database").
pub fn image(image: &str) -> Option<&'static str> {
    let name = image.rsplit('/').next().unwrap_or(image).split(':').next().unwrap_or("").to_lowercase();
    Some(match name.as_str() {
        "mongo" | "mongodb" => "MongoDB database",
        "postgres" | "postgresql" => "PostgreSQL database",
        "mysql" | "mariadb" => "MySQL database",
        "redis" | "valkey" => "Redis cache",
        "kafka" | "cp-kafka" => "Kafka message stream",
        "zookeeper" | "cp-zookeeper" => "ZooKeeper (for Kafka)",
        "rabbitmq" => "RabbitMQ message queue",
        "keycloak" => "Keycloak sign-in server",
        "nginx" | "httpd" | "caddy" | "traefik" => "Web server",
        "elasticsearch" | "opensearch" => "Search engine",
        "grafana" | "otel-lgtm" => "Dashboards and monitoring",
        "prometheus" => "Metrics collector",
        "minio" => "File storage (S3)",
        "ollama" => "Local AI models",
        _ => return None,
    })
}

pub fn process(name: &str) -> Option<&'static Known> {
    PROCESSES.iter().find(|(key, _)| *key == name).map(|(_, k)| k)
}

pub fn app(desktop_id: &str) -> Safety {
    if DESKTOP_APPS.contains(&desktop_id) {
        Needed
    } else {
        Optional
    }
}
