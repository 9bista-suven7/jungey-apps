//! Jungey Center: Wi-Fi, Bluetooth, sound, brightness, battery, Do Not Disturb and
//! power behind one icon on the bar, like macOS Control Center.
//!
//!     jungey-center               open, or close if already open
//!     jungey-center --background  load at login without showing, so it opens instantly
//!
//! It opens as a card above the bar; a click outside it or Esc closes it.

mod system;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use system::State;

struct Tile {
    button: gtk::Button,
    subtitle: gtk::Label,
}

struct Center {
    window: gtk::ApplicationWindow,
    card: gtk::Box,
    pages: gtk::Stack,
    battery_icon: gtk::Image,
    battery_label: gtk::Label,
    battery_detail: gtk::Label,
    wifi: Tile,
    bluetooth: Tile,
    dnd: Tile,
    airplane: Tile,
    brightness: gtk::Scale,
    volume: gtk::Scale,
    mute: gtk::Button,
    networks: gtk::ListBox,
    network_note: gtk::Label,
    confirm: RefCell<Option<&'static str>>,
    state: RefCell<State>,
    updating: Cell<bool>,
}

fn tile(icon: &str, title: &str) -> Tile {
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(20);
    image.add_css_class("tile-icon");
    let t = gtk::Label::new(Some(title));
    t.set_xalign(0.0);
    t.add_css_class("tile-title");
    let subtitle = gtk::Label::new(None);
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.add_css_class("tile-subtitle");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_valign(gtk::Align::Center);
    text.append(&t);
    text.append(&subtitle);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&image);
    row.append(&text);
    let button = gtk::Button::builder().child(&row).css_classes(["center-tile"]).hexpand(true).build();
    Tile { button, subtitle }
}

fn slider(icon: &str) -> (gtk::Box, gtk::Scale, gtk::Button) {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("slider-icon");
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(false);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("center-slider");
    row.append(&button);
    row.append(&scale);
    (row, scale, button)
}

fn round_button(icon: &str, tip: &str) -> gtk::Button {
    gtk::Button::builder().icon_name(icon).tooltip_text(tip).css_classes(["power-button"]).build()
}

impl Center {
    fn new(app: &adw::Application) -> Rc<Center> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Jungey Center")
            .decorated(false)
            .css_classes(["jungey-center"])
            .build();

        // header: battery, and a way into Settings
        let battery_icon = gtk::Image::from_icon_name("battery-level-100-symbolic");
        battery_icon.set_pixel_size(30);
        let battery_label = gtk::Label::new(None);
        battery_label.add_css_class("battery-percent");
        battery_label.set_xalign(0.0);
        let battery_detail = gtk::Label::new(None);
        battery_detail.add_css_class("tile-subtitle");
        battery_detail.set_xalign(0.0);
        let battery_text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        battery_text.append(&battery_label);
        battery_text.append(&battery_detail);
        let settings = round_button("emblem-system-symbolic", "Settings");
        settings.set_hexpand(true);
        settings.set_halign(gtk::Align::End);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.add_css_class("center-header");
        header.append(&battery_icon);
        header.append(&battery_text);
        header.append(&settings);

        let wifi = tile("network-wireless-symbolic", "Wi-Fi");
        let bluetooth = tile("bluetooth-active-symbolic", "Bluetooth");
        let dnd = tile("notifications-disabled-symbolic", "Do Not Disturb");
        let airplane = tile("airplane-mode-symbolic", "Airplane Mode");
        let tiles = gtk::Grid::builder().row_spacing(10).column_spacing(10).column_homogeneous(true).build();
        tiles.attach(&wifi.button, 0, 0, 1, 1);
        tiles.attach(&bluetooth.button, 1, 0, 1, 1);
        tiles.attach(&dnd.button, 0, 1, 1, 1);
        tiles.attach(&airplane.button, 1, 1, 1, 1);

        let (bright_row, brightness, _) = slider("display-brightness-symbolic");
        let (sound_row, volume, mute) = slider("audio-volume-high-symbolic");
        let sliders = gtk::Box::new(gtk::Orientation::Vertical, 6);
        sliders.add_css_class("center-group");
        sliders.append(&bright_row);
        sliders.append(&sound_row);

        let power = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        power.set_homogeneous(true);
        power.add_css_class("center-group");
        for (icon, tip, action) in [
            ("system-lock-screen-symbolic", "Lock", "lock"),
            ("weather-clear-night-symbolic", "Sleep", "sleep"),
            ("system-reboot-symbolic", "Restart", "restart"),
            ("system-shutdown-symbolic", "Shut Down", "shutdown"),
        ] {
            let b = round_button(icon, tip);
            b.set_detailed_action_name(&format!("win.power::{action}"));
            power.append(&b);
        }

        let main = gtk::Box::new(gtk::Orientation::Vertical, 12);
        main.append(&header);
        main.append(&tiles);
        main.append(&sliders);
        main.append(&power);

        // Wi-Fi networks, one level down
        let back = gtk::Button::from_icon_name("go-previous-symbolic");
        back.add_css_class("flat");
        let net_title = gtk::Label::new(Some("Wi-Fi Networks"));
        net_title.add_css_class("tile-title");
        let net_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        net_header.append(&back);
        net_header.append(&net_title);
        let networks = gtk::ListBox::new();
        networks.add_css_class("network-list");
        networks.set_selection_mode(gtk::SelectionMode::None);
        let network_note = gtk::Label::new(Some("Looking for networks…"));
        network_note.add_css_class("tile-subtitle");
        let scroll = gtk::ScrolledWindow::builder()
            .child(&networks)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(300)
            .max_content_height(360)
            .propagate_natural_height(true)
            .build();
        let net_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
        net_page.append(&net_header);
        net_page.append(&network_note);
        net_page.append(&scroll);

        let pages = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::SlideLeftRight)
            .transition_duration(180)
            .vhomogeneous(false)
            .interpolate_size(true)
            .build();
        pages.add_named(&main, Some("main"));
        pages.add_named(&net_page, Some("networks"));

        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("center-card");
        card.set_size_request(360, -1);
        card.set_halign(gtk::Align::End);
        card.set_valign(gtk::Align::End);
        card.set_vexpand(true); // take the full height so End puts it just above the bar
        card.set_margin_bottom(72);
        card.append(&pages);

        let backdrop = gtk::Box::new(gtk::Orientation::Vertical, 0);
        backdrop.add_css_class("center-backdrop");
        backdrop.append(&card);
        window.set_child(Some(&backdrop));

        let c = Rc::new(Center {
            window,
            card,
            pages,
            battery_icon,
            battery_label,
            battery_detail,
            wifi,
            bluetooth,
            dnd,
            airplane,
            brightness,
            volume,
            mute,
            networks,
            network_note,
            confirm: RefCell::default(),
            state: RefCell::default(),
            updating: Cell::new(false),
        });
        c.connect(&settings, &back, &backdrop);
        c
    }

    fn connect(self: &Rc<Self>, settings: &gtk::Button, back: &gtk::Button, backdrop: &gtk::Box) {
        // tiles toggle; the Wi-Fi tile's long press / right click shows the networks
        let weak = Rc::downgrade(self);
        self.wifi.button.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                let on = !c.state.borrow().wifi_on;
                c.apply(move || system::set_wifi(on));
            }
        });
        let pick = gtk::GestureClick::new();
        pick.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(self);
        pick.connect_pressed(move |_, _, _, _| {
            if let Some(c) = weak.upgrade() {
                c.show_networks();
            }
        });
        self.wifi.button.add_controller(pick);
        let more = gtk::Button::from_icon_name("go-next-symbolic");
        more.add_css_class("tile-more");
        more.set_tooltip_text(Some("Choose a network"));
        if let Some(row) = self.wifi.button.child().and_downcast::<gtk::Box>() {
            more.set_hexpand(true);
            more.set_halign(gtk::Align::End);
            row.append(&more);
        }
        let weak = Rc::downgrade(self);
        more.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                c.show_networks();
            }
        });

        let weak = Rc::downgrade(self);
        self.bluetooth.button.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                let on = !c.state.borrow().bt_on;
                c.apply(move || system::set_bluetooth(on));
            }
        });
        let weak = Rc::downgrade(self);
        self.dnd.button.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                let on = !c.state.borrow().dnd;
                c.apply(move || system::set_dnd(on));
            }
        });
        let weak = Rc::downgrade(self);
        self.airplane.button.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                let on = !c.state.borrow().airplane;
                c.apply(move || system::set_airplane(on));
            }
        });

        let weak = Rc::downgrade(self);
        self.volume.connect_value_changed(move |s| {
            if let Some(c) = weak.upgrade() {
                if !c.updating.get() {
                    let v = s.value();
                    gio::spawn_blocking(move || system::set_volume(v));
                    c.mute.set_icon_name(volume_icon(v, false));
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.mute.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                c.apply(|| {
                    system::toggle_mute();
                    true
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.brightness.connect_value_changed(move |s| {
            if let Some(c) = weak.upgrade() {
                if !c.updating.get() {
                    let v = s.value();
                    gio::spawn_blocking(move || system::set_brightness(v));
                }
            }
        });

        // power: lock and sleep right away; restart and shut down ask for a second tap
        let power = gio::SimpleAction::new("power", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        power.connect_activate(move |_, v| {
            let (Some(c), Some(v)) = (weak.upgrade(), v.and_then(|v| v.str().map(str::to_string))) else { return };
            c.power(&v);
        });
        self.window.add_action(&power);

        let weak = Rc::downgrade(self);
        settings.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                system::launch("xfce4-settings-manager", &[]);
                c.hide();
            }
        });
        let weak = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                c.pages.set_visible_child_name("main");
            }
        });

        // a click outside the card, Esc, or another window taking focus closes it
        let outside = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        outside.connect_released(move |g, _, x, y| {
            let Some(c) = weak.upgrade() else { return };
            let Some(w) = g.widget() else { return };
            let p = gtk::graphene::Point::new(x as f32, y as f32);
            let inside = w.compute_point(&c.card, &p).map(|q| c.card.contains(q.x() as f64, q.y() as f64)).unwrap_or(false);
            if !inside {
                c.hide();
            }
        });
        backdrop.add_controller(outside);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(c) = weak.upgrade() {
                    c.hide();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(keys);
        let weak = Rc::downgrade(self);
        self.window.connect_is_active_notify(move |w| {
            if !w.is_active() && w.is_visible() {
                if let Some(c) = weak.upgrade() {
                    c.hide();
                }
            }
        });
        self.window.connect_close_request(|w| {
            w.set_visible(false);
            glib::Propagation::Stop
        });

        // keep the card current while it's open
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_secs(4), move || {
            let Some(c) = weak.upgrade() else { return glib::ControlFlow::Break };
            if c.window.is_visible() {
                c.refresh();
            }
            glib::ControlFlow::Continue
        });
    }

    /// Change something, then re-read everything so the card shows the result.
    fn apply(self: &Rc<Self>, change: impl FnOnce() -> bool + Send + 'static) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(change).await;
            if let Some(c) = weak.upgrade() {
                c.refresh();
            }
        });
    }

    fn refresh(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(state) = gio::spawn_blocking(system::read).await else { return };
            if let Some(c) = weak.upgrade() {
                c.show_state(state);
            }
        });
    }

    fn show_state(&self, s: State) {
        let set = |t: &Tile, on: bool, text: &str| {
            if on {
                t.button.add_css_class("on");
            } else {
                t.button.remove_css_class("on");
            }
            t.subtitle.set_text(text);
        };
        set(&self.wifi, s.wifi_on, &if !s.wifi_on { "Off".into() } else { s.wifi_name.clone().unwrap_or_else(|| "Not connected".into()) });
        let bt = match (s.bt_on, s.bt_devices.as_slice()) {
            (false, _) => "Off".to_string(),
            (true, []) => "On".to_string(),
            (true, [one]) => one.clone(),
            (true, many) => format!("{} devices", many.len()),
        };
        set(&self.bluetooth, s.bt_on, &bt);
        set(&self.dnd, s.dnd, if s.dnd { "On" } else { "Off" });
        set(&self.airplane, s.airplane, if s.airplane { "On" } else { "Off" });

        self.updating.set(true);
        self.volume.set_value(if s.muted { 0.0 } else { s.volume });
        self.mute.set_icon_name(volume_icon(s.volume, s.muted));
        match s.brightness {
            Some(b) => {
                self.brightness.set_value(b);
                self.brightness.set_sensitive(true);
            }
            None => self.brightness.set_sensitive(false),
        }
        self.updating.set(false);

        match &s.battery {
            Some(b) => {
                self.battery_label.set_text(&format!("{}%", b.percent));
                let level = (b.percent + 5) / 10 * 10;
                let icon = if b.full {
                    "battery-level-100-charged-symbolic".to_string()
                } else if b.charging {
                    format!("battery-level-{level}-charging-symbolic")
                } else {
                    format!("battery-level-{level}-symbolic")
                };
                self.battery_icon.set_icon_name(Some(&icon));
                let detail = if b.full {
                    "Fully charged".to_string()
                } else if b.charging {
                    "Charging".to_string()
                } else {
                    match b.hours_left {
                        Some(h) if h >= 1.0 => format!("On battery · about {} h {} min left", h as u32, ((h.fract()) * 60.0) as u32),
                        Some(h) => format!("On battery · about {} min left", (h * 60.0) as u32),
                        None => "On battery".to_string(),
                    }
                };
                self.battery_detail.set_text(&detail);
            }
            None => {
                self.battery_label.set_text("Plugged in");
                self.battery_detail.set_text("");
                self.battery_icon.set_icon_name(Some("ac-adapter-symbolic"));
            }
        }
        self.state.replace(s);
    }

    fn show_networks(self: &Rc<Self>) {
        self.pages.set_visible_child_name("networks");
        self.network_note.set_text("Looking for networks…");
        self.network_note.set_visible(true);
        while let Some(row) = self.networks.first_child() {
            self.networks.remove(&row);
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(nets) = gio::spawn_blocking(system::networks).await else { return };
            let Some(c) = weak.upgrade() else { return };
            c.network_note.set_visible(nets.is_empty());
            c.network_note.set_text("No networks found");
            for n in nets {
                c.networks.append(&c.network_row(n));
            }
        });
    }

    fn network_row(self: &Rc<Self>, n: system::Network) -> gtk::Box {
        let bars = match n.signal {
            0..=29 => "network-wireless-signal-weak-symbolic",
            30..=54 => "network-wireless-signal-ok-symbolic",
            55..=79 => "network-wireless-signal-good-symbolic",
            _ => "network-wireless-signal-excellent-symbolic",
        };
        let name = gtk::Label::new(Some(&n.name));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        line.append(&gtk::Image::from_icon_name(bars));
        line.append(&name);
        if n.secure {
            line.append(&gtk::Image::from_icon_name("system-lock-screen-symbolic"));
        }
        if n.active {
            line.append(&gtk::Image::from_icon_name("object-select-symbolic"));
        }
        let button = gtk::Button::builder().child(&line).css_classes(["network-row"]).build();
        let password = gtk::PasswordEntry::builder().placeholder_text("Password").show_peek_icon(true).visible(false).build();
        let status = gtk::Label::new(None);
        status.add_css_class("tile-subtitle");
        status.set_xalign(0.0);
        status.set_wrap(true);
        status.set_visible(false);
        let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
        row.append(&button);
        row.append(&password);
        row.append(&status);

        let join = {
            let weak = Rc::downgrade(self);
            let (name, password, status) = (n.name.clone(), password.clone(), status.clone());
            move |pw: Option<String>| {
                status.set_text("Connecting…");
                status.set_visible(true);
                let (weak, name, status, password) = (weak.clone(), name.clone(), status.clone(), password.clone());
                glib::spawn_future_local(async move {
                    let n2 = name.clone();
                    let res = gio::spawn_blocking(move || system::connect(&n2, pw.as_deref())).await;
                    match res {
                        Ok(Ok(())) => {
                            if let Some(c) = weak.upgrade() {
                                c.pages.set_visible_child_name("main");
                                c.refresh();
                            }
                        }
                        Ok(Err(e)) => {
                            status.set_text(&e);
                            password.set_visible(true);
                            password.grab_focus();
                        }
                        Err(_) => status.set_text("Couldn't connect"),
                    }
                });
            }
        };
        let needs_password = n.secure && !n.known && !n.active;
        let j = join.clone();
        let pw = password.clone();
        button.connect_clicked(move |_| {
            if needs_password && !pw.is_visible() {
                pw.set_visible(true);
                pw.grab_focus();
            } else if needs_password {
                j(Some(pw.text().to_string()));
            } else {
                j(None);
            }
        });
        password.connect_activate(move |e| join(Some(e.text().to_string())));
        row
    }

    fn power(self: &Rc<Self>, action: &str) {
        match action {
            "lock" => {
                self.hide();
                system::launch("xflock4", &[]);
            }
            "sleep" => {
                self.hide();
                system::launch("xfce4-session-logout", &["--suspend"]);
            }
            "restart" | "shutdown" => {
                let action: &'static str = if action == "restart" { "restart" } else { "shutdown" };
                if *self.confirm.borrow() == Some(action) {
                    self.hide();
                    let flag = if action == "restart" { "--reboot" } else { "--halt" };
                    system::launch("xfce4-session-logout", &[flag, "--fast"]);
                } else {
                    self.confirm.replace(Some(action));
                    self.battery_detail.set_text(if action == "restart" { "Tap Restart again to restart" } else { "Tap Shut Down again to turn off" });
                    let weak = Rc::downgrade(self);
                    glib::timeout_add_local_once(Duration::from_secs(4), move || {
                        if let Some(c) = weak.upgrade() {
                            c.confirm.replace(None);
                            c.refresh();
                        }
                    });
                }
            }
            _ => {}
        }
    }

    fn show(self: &Rc<Self>) {
        self.pages.set_visible_child_name("main");
        self.confirm.replace(None);
        self.refresh();
        self.window.fullscreen();
        self.window.present();
    }

    fn hide(&self) {
        if !self.window.is_visible() {
            return;
        }
        self.window.set_visible(false);
    }

    fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Put the card's right edge over the right end of the bar.
    fn place(&self) {
        let Some(display) = gdk::Display::default() else { return };
        if let Some(m) = display.monitors().item(0).and_downcast::<gdk::Monitor>() {
            let w = m.geometry().width();
            self.card.set_margin_end((w / 5).max(16));
        }
    }
}

fn volume_icon(v: f64, muted: bool) -> &'static str {
    if muted || v <= 0.0 {
        "audio-volume-muted-symbolic"
    } else if v < 34.0 {
        "audio-volume-low-symbolic"
    } else if v < 67.0 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("os.jungey.Center")
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let center: Rc<RefCell<Option<Rc<Center>>>> = Rc::default();

    app.connect_startup(|app| {
        jungey_ui::init();
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("center.css"));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        std::mem::forget(app.hold());
    });

    let slot = center.clone();
    app.connect_command_line(move |app, cmd| {
        if slot.borrow().is_none() {
            let c = Center::new(app);
            c.place();
            slot.replace(Some(c));
        }
        let background = cmd.arguments().iter().any(|a| a.to_str() == Some("--background"));
        if !background {
            if let Some(c) = slot.borrow().as_ref() {
                c.toggle();
            }
        }
        glib::ExitCode::SUCCESS
    });
    app.run()
}
