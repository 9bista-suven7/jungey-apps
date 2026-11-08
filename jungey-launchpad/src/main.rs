//! Jungey Launchpad: every app in a full-screen grid over the blurred wallpaper.
//!
//!     jungey-launchpad               open, or close if already open
//!     jungey-launchpad --background  load at login without showing, so it opens instantly later
//!
//! Type to search; Enter opens the first match. Two-finger swipe (or scroll) flips pages.
//! Esc, a click on empty space, or opening an app closes it.

mod apps;
mod pager;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use apps::{App, Usage};
use pager::Pager;

const ICON: i32 = 88;
const TILE_W: i32 = 170;
const TILE_H: i32 = 150;

struct Pad {
    window: gtk::ApplicationWindow,
    content: gtk::Box,
    search: gtk::SearchEntry,
    stack: gtk::Stack,
    pager: Rc<Pager>,
    frequent: gtk::Box,
    results: gtk::Box,
    all_button: gtk::ToggleButton,
    frequent_button: gtk::ToggleButton,
    apps: RefCell<Vec<App>>,
    usage: RefCell<Usage>,
    cols: Cell<i32>,
    rows: Cell<i32>,
}

impl Pad {
    fn new(app: &adw::Application) -> Rc<Pad> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Jungey Launchpad")
            .decorated(false)
            .css_classes(["jungey-launchpad"])
            .build();

        let backdrop = gtk::Picture::for_filename(glib::user_data_dir().join("backgrounds/jungey-blur.jpg"));
        backdrop.set_content_fit(gtk::ContentFit::Cover);
        backdrop.set_can_target(false);
        let dim = gtk::Box::new(gtk::Orientation::Vertical, 0);
        dim.add_css_class("launchpad-dim");
        dim.set_can_target(false);

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Type to search…")
            .halign(gtk::Align::Center)
            .margin_top(44)
            .css_classes(["launchpad-search"])
            .build();

        // pages that follow your fingers (see pager.rs)
        let pager = Pager::new();
        let all_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        all_page.append(&pager.widget);

        let frequent = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frequent.set_valign(gtk::Align::Center);
        let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
        results.set_valign(gtk::Align::Start);
        results.set_margin_top(40);

        let stack = gtk::Stack::builder()
            .vexpand(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(160)
            .build();
        stack.add_named(&all_page, Some("all"));
        stack.add_named(&frequent, Some("frequent"));
        stack.add_named(&results, Some("results"));

        let frequent_button = gtk::ToggleButton::with_label("FREQUENT");
        let all_button = gtk::ToggleButton::with_label("ALL");
        all_button.set_group(Some(&frequent_button));
        all_button.set_active(true);
        let switch = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        switch.add_css_class("launchpad-switch");
        switch.set_halign(gtk::Align::Center);
        switch.set_margin_bottom(30);
        switch.append(&frequent_button);
        switch.append(&all_button);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content.add_css_class("launchpad-content");
        content.append(&search);
        content.append(&stack);
        content.append(&switch);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&backdrop));
        overlay.add_overlay(&dim);
        overlay.add_overlay(&content);
        window.set_child(Some(&overlay));

        let pad = Rc::new(Pad {
            window,
            content,
            search,
            stack,
            pager,
            frequent,
            results,
            all_button,
            frequent_button,
            apps: RefCell::new(apps::all()),
            usage: RefCell::new(Usage::load()),
            cols: Cell::new(7),
            rows: Cell::new(4),
        });
        pad.connect();
        pad
    }

    /// Fit the grid to the screen: roomy, 4-6 columns, 3-4 rows.
    fn measure(&self) {
        let Some(display) = gdk::Display::default() else { return };
        let monitor = display.monitors().item(0).and_downcast::<gdk::Monitor>();
        if let Some(m) = monitor {
            let g = m.geometry();
            self.cols.set(((g.width() - 360) / (TILE_W + 60)).clamp(4, 6));
            self.rows.set(((g.height() - 300) / (TILE_H + 30)).clamp(3, 4));
        }
    }

    fn connect(self: &Rc<Self>) {
        self.search.set_key_capture_widget(Some(&self.window));

        let weak = Rc::downgrade(self);
        self.search.connect_search_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.show_view();
            }
        });
        let weak = Rc::downgrade(self);
        self.search.connect_activate(move |e| {
            let Some(p) = weak.upgrade() else { return };
            let first = apps::search(&p.apps.borrow(), &e.text()).first().map(|a| (a.info.clone(), a.id.clone()));
            if let Some((info, id)) = first {
                p.launch(&info, &id);
            }
        });

        for button in [&self.all_button, &self.frequent_button] {
            let weak = Rc::downgrade(self);
            button.connect_toggled(move |b| {
                if b.is_active() {
                    if let Some(p) = weak.upgrade() {
                        p.show_view();
                    }
                }
            });
        }

        // Esc clears the search, then closes; Page Up / Down flip pages (arrows move between apps)
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(p) = weak.upgrade() else { return glib::Propagation::Proceed };
            match key {
                gdk::Key::Escape => {
                    if p.search.text().is_empty() {
                        p.hide();
                    } else {
                        p.search.set_text("");
                    }
                    glib::Propagation::Stop
                }
                gdk::Key::Page_Down => {
                    p.flip(1);
                    glib::Propagation::Stop
                }
                gdk::Key::Page_Up => {
                    p.flip(-1);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.window.add_controller(keys);

        // a click on empty space closes (app tiles and buttons take their own clicks)
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        click.connect_released(move |_, _, _, _| {
            if let Some(p) = weak.upgrade() {
                p.hide();
            }
        });
        self.content.add_controller(click);

        // switching to another window closes it, like a menu
        let weak = Rc::downgrade(self);
        self.window.connect_is_active_notify(move |w| {
            if !w.is_active() && w.is_visible() {
                if let Some(p) = weak.upgrade() {
                    p.hide();
                }
            }
        });
        self.window.connect_close_request(|w| {
            w.set_visible(false);
            glib::Propagation::Stop
        });

        // apps installed or removed while running
        let weak = Rc::downgrade(self);
        gio::AppInfoMonitor::get().connect_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.apps.replace(apps::all());
                p.rebuild();
            }
        });
    }

    fn flip(&self, by: i32) {
        self.pager.go_to(self.pager.current() + by, true);
    }

    fn tile(self: &Rc<Self>, app: &App) -> gtk::Button {
        let image = gtk::Image::new();
        image.set_pixel_size(ICON);
        match &app.icon {
            Some(icon) => image.set_from_gicon(icon),
            None => image.set_icon_name(Some("application-x-executable")),
        }
        let label = gtk::Label::new(Some(&app.name.to_uppercase()));
        label.set_max_width_chars(14);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_justify(gtk::Justification::Center);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
        column.append(&image);
        column.append(&label);
        let button = gtk::Button::builder()
            .child(&column)
            .tooltip_text(app.name.as_str())
            .css_classes(["launchpad-tile"])
            .build();
        button.set_size_request(TILE_W, TILE_H);
        let (weak, info, id) = (Rc::downgrade(self), app.info.clone(), app.id.clone());
        button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.launch(&info, &id);
            }
        });
        button
    }

    fn grid(self: &Rc<Self>, apps: &[&App]) -> gtk::Grid {
        let grid = gtk::Grid::builder()
            .row_spacing(30)
            .column_spacing(60)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        let cols = self.cols.get();
        for (i, app) in apps.iter().enumerate() {
            let i = i as i32;
            grid.attach(&self.tile(app), i % cols, i / cols, 1, 1);
        }
        grid
    }

    fn note(text: &str) -> gtk::Label {
        let l = gtk::Label::new(Some(text));
        l.add_css_class("launchpad-note");
        l
    }

    fn clear(container: &impl IsA<gtk::Widget>) {
        let w = container.upcast_ref::<gtk::Widget>();
        while let Some(child) = w.first_child() {
            child.unparent();
        }
    }

    /// Lay out the All pages and the Frequent view.
    fn rebuild(self: &Rc<Self>) {
        self.measure();
        let per_page = (self.cols.get() * self.rows.get()) as usize;
        let apps = self.apps.borrow().clone();
        let all: Vec<&App> = apps.iter().collect();
        let width = gdk::Display::default()
            .and_then(|d| d.monitors().item(0).and_downcast::<gdk::Monitor>())
            .map(|m| m.geometry().width())
            .unwrap_or(1920);
        let pages: Vec<gtk::Widget> = all.chunks(per_page.max(1)).map(|c| self.grid(c).upcast()).collect();
        self.pager.set_pages(pages, width);

        Self::clear(&self.frequent);
        let usage = self.usage.borrow();
        let top = usage.top(&apps, per_page);
        if top.is_empty() {
            self.frequent.append(&Self::note("Apps you open often will show up here"));
        } else {
            self.frequent.append(&self.grid(&top));
        }
    }

    fn show_view(self: &Rc<Self>) {
        let query = self.search.text().trim().to_string();
        if !query.is_empty() {
            Self::clear(&self.results);
            let apps = self.apps.borrow().clone();
            let per_page = (self.cols.get() * self.rows.get()) as usize;
            let hits: Vec<&App> = apps::search(&apps, &query).into_iter().take(per_page).collect();
            if hits.is_empty() {
                self.results.append(&Self::note(&format!("Nothing matches “{query}”")));
            } else {
                self.results.append(&self.grid(&hits));
            }
            self.stack.set_visible_child_name("results");
        } else if self.frequent_button.is_active() {
            self.stack.set_visible_child_name("frequent");
        } else {
            self.stack.set_visible_child_name("all");
        }
    }

    fn launch(self: &Rc<Self>, info: &gio::AppInfo, id: &str) {
        let context = WidgetExt::display(&self.window).app_launch_context();
        if let Err(e) = info.launch(&[], Some(&context)) {
            eprintln!("jungey-launchpad: couldn't open {id}: {e}");
        }
        self.usage.borrow_mut().bump(id);
        self.hide();
        // refresh Frequent quietly for next time
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(p) = weak.upgrade() {
                p.rebuild();
            }
        });
    }

    fn show(self: &Rc<Self>) {
        self.search.set_text("");
        self.all_button.set_active(true);
        self.show_view();
        self.pager.go_to(0, false);
        self.window.fullscreen();
        self.window.present();
        self.search.grab_focus();
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
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("os.jungey.Launchpad")
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let pad: Rc<RefCell<Option<Rc<Pad>>>> = Rc::default();

    app.connect_startup(|app| {
        jungey_ui::init();
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("launchpad.css"));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        // stay loaded between uses, so it opens instantly
        std::mem::forget(app.hold());
    });

    let slot = pad.clone();
    app.connect_command_line(move |app, cmd| {
        let first_time = slot.borrow().is_none();
        if first_time {
            let p = Pad::new(app);
            p.rebuild();
            slot.replace(Some(p));
        }
        let background = cmd.arguments().iter().any(|a| a.to_str() == Some("--background"));
        if !background {
            if let Some(p) = slot.borrow().as_ref() {
                p.toggle();
            }
        }
        glib::ExitCode::SUCCESS
    });
    app.run()
}
