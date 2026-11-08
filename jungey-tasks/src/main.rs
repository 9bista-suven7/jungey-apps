//! Jungey Tasks: what's running on this computer, in plain words, and a way to stop it.
//!
//! Three lists (Apps you opened, Services, Other processes) show only a name, one line about
//! it, its memory and processor use. Click a row for everything else: what it is, whether
//! it's safe to stop, start-at-boot, details, its processes and its recent messages.

mod act;
mod collect;
mod know;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use act::Action;
use collect::{Kind, Snapshot, Summary, Task};
use know::Safety;

const CSS: &str = include_str!("tasks.css");
const SECTIONS: [(&str, &str); 3] = [("Apps", "view-grid-symbolic"), ("Services", "system-run-symbolic"), ("Other", "utilities-terminal-symbolic")];

fn section(kind: Kind) -> usize {
    match kind {
        Kind::App => 0,
        Kind::Process => 2,
        _ => 1,
    }
}

/// Order and heading inside a list.
fn group(t: &Task) -> (u8, &'static str) {
    match t.kind {
        Kind::App => (0, ""),
        Kind::Container => (0, "DOCKER CONTAINERS"),
        Kind::UserService => (1, "YOUR BACKGROUND SERVICES"),
        Kind::Service => (2, "SYSTEM SERVICES"),
        Kind::Process if t.mine => (0, "YOURS"),
        Kind::Process => (1, "SYSTEM"),
    }
}

fn kind_label(t: &Task) -> &'static str {
    match t.kind {
        Kind::App => "App",
        Kind::Service => "System service",
        Kind::UserService => "Your service",
        Kind::Container => "Docker container",
        Kind::Process if t.mine => "Your process",
        Kind::Process => "System process",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Memory,
    Cpu,
    Name,
}

struct Row {
    row: gtk::ListBoxRow,
    name: gtk::Label,
    about: gtk::Label,
    memory: gtk::Label,
    cpu: gtk::Label,
    section: usize,
}

struct Stat {
    value: gtk::Label,
    detail: gtk::Label,
    bar: gtk::LevelBar,
}

struct Detail {
    id: String,
    dialog: adw::Dialog,
    status: gtk::Label,
    memory: gtk::Label,
    cpu: gtk::Label,
    running: gtk::Label,
    count: gtk::Label,
    stop: gtk::Button,
    second: gtk::Button,
}

struct Tasks {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    lists: Vec<gtk::ListBox>,
    pages: Vec<adw::ViewStackPage>,
    rows: RefCell<HashMap<String, Row>>,
    tasks: RefCell<HashMap<String, Task>>,
    stats: [Stat; 4],
    search: gtk::SearchEntry,
    sort: Cell<Sort>,
    detail: RefCell<Option<Detail>>,
    collector: Arc<Mutex<collect::Collector>>,
    busy: Cell<bool>,
}

fn main() -> glib::ExitCode {
    if std::env::args().any(|a| a == "--list") {
        print_list();
        return glib::ExitCode::SUCCESS;
    }
    let app = adw::Application::builder().application_id("os.jungey.Tasks").build();
    app.connect_startup(|app| {
        jungey_ui::init();
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        }
        app.set_accels_for_action("win.search", &["<Control>f"]);
        for (i, (name, _)) in SECTIONS.iter().enumerate() {
            app.set_accels_for_action(&format!("win.page::{name}"), &[&format!("<Alt>{}", i + 1)]);
        }
        app.set_accels_for_action("window.close", &["<Control>w", "<Control>q"]);
    });
    app.connect_activate(|app| match app.active_window() {
        Some(w) => w.present(),
        None => Tasks::open(app),
    });
    app.run()
}

impl Tasks {
    fn open(app: &adw::Application) {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Jungey Tasks")
            .icon_name("jungey-monitor")
            .default_width(960)
            .default_height(760)
            .build();

        // the three lists
        let stack = adw::ViewStack::new();
        let mut lists = Vec::new();
        let mut pages = Vec::new();
        for (title, icon) in SECTIONS {
            let list = gtk::ListBox::new();
            list.set_selection_mode(gtk::SelectionMode::None);
            list.add_css_class("task-list");
            let empty = gtk::Label::new(Some("Nothing here matches your search"));
            empty.add_css_class("dim-label");
            empty.set_margin_top(48);
            list.set_placeholder(Some(&empty));
            let clamp = adw::Clamp::builder().maximum_size(980).child(&list).build();
            clamp.set_margin_start(12);
            clamp.set_margin_end(12);
            clamp.set_margin_bottom(18);
            let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build();
            pages.push(stack.add_titled_with_icon(&scroller, Some(title), title, icon));
            lists.push(list);
        }

        // header: the switcher, search and sort
        let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&switcher));
        let find = gtk::ToggleButton::builder().icon_name("system-search-symbolic").tooltip_text("Search (Ctrl+F)").build();
        header.pack_start(&find);
        let sorts = gio::Menu::new();
        sorts.append(Some("Most memory first"), Some("win.sort::memory"));
        sorts.append(Some("Busiest first"), Some("win.sort::cpu"));
        sorts.append(Some("By name"), Some("win.sort::name"));
        let menu = gio::Menu::new();
        menu.append_section(Some("Sort"), &sorts);
        let sort_button = gtk::MenuButton::builder().icon_name("view-sort-descending-symbolic").menu_model(&menu).tooltip_text("Sort").build();
        header.pack_end(&sort_button);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search apps, services and processes"));
        search.set_hexpand(true);
        let search_bar = gtk::SearchBar::builder().child(&adw::Clamp::builder().maximum_size(560).child(&search).build()).build();
        search_bar.connect_entry(&search);
        search_bar.set_key_capture_widget(Some(&window));
        find.bind_property("active", &search_bar, "search-mode-enabled").bidirectional().sync_create().build();

        // the four numbers at the top
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        strip.set_homogeneous(true);
        strip.add_css_class("summary");
        let mut stats = Vec::new();
        for (caption, bar) in [("PROCESSOR", true), ("MEMORY", true), ("DISK", true), ("TURNED ON", false)] {
            let (tile, stat) = stat_tile(caption, bar);
            strip.append(&tile);
            stats.push(stat);
        }
        let strip_clamp = adw::Clamp::builder().maximum_size(980).child(&strip).build();
        strip_clamp.set_margin_start(12);
        strip_clamp.set_margin_end(12);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&strip_clamp);
        body.append(&stack);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&body));
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.add_top_bar(&search_bar);
        view.set_content(Some(&toasts));
        window.set_content(Some(&view));

        let this = Rc::new(Tasks {
            window: window.clone(),
            toasts,
            lists,
            pages,
            rows: RefCell::new(HashMap::new()),
            tasks: RefCell::new(HashMap::new()),
            stats: stats.try_into().ok().unwrap(),
            search: search.clone(),
            sort: Cell::new(Sort::Memory),
            detail: RefCell::new(None),
            collector: Arc::new(Mutex::new(collect::Collector::new())),
            busy: Cell::new(false),
        });

        for list in &this.lists {
            let weak = Rc::downgrade(&this);
            list.set_sort_func(move |a, b| {
                let Some(this) = weak.upgrade() else { return gtk::Ordering::Equal };
                let tasks = this.tasks.borrow();
                match (tasks.get(a.widget_name().as_str()), tasks.get(b.widget_name().as_str())) {
                    (Some(x), Some(y)) => this.compare(x, y).into(),
                    _ => gtk::Ordering::Equal,
                }
            });
            let weak = Rc::downgrade(&this);
            list.set_filter_func(move |row| {
                let Some(this) = weak.upgrade() else { return true };
                let q = this.search.text().to_lowercase();
                q.is_empty() || this.tasks.borrow().get(row.widget_name().as_str()).map(|t| matches(t, &q)).unwrap_or(true)
            });
            let weak = Rc::downgrade(&this);
            list.set_header_func(move |row, before| {
                let Some(this) = weak.upgrade() else { return };
                let tasks = this.tasks.borrow();
                let mine = tasks.get(row.widget_name().as_str()).map(group);
                let prev = before.and_then(|b| tasks.get(b.widget_name().as_str())).map(group);
                match mine {
                    Some((g, title)) if !title.is_empty() && prev.map(|p| p.0) != Some(g) => {
                        let label = gtk::Label::new(Some(title));
                        label.set_xalign(0.0);
                        label.add_css_class("list-heading");
                        row.set_header(Some(&label));
                    }
                    _ => row.set_header(None::<&gtk::Widget>),
                }
            });
            let weak = Rc::downgrade(&this);
            list.connect_row_activated(move |_, row| {
                if let Some(this) = weak.upgrade() {
                    this.show(row.widget_name().as_str());
                }
            });
        }

        let weak = Rc::downgrade(&this);
        search.connect_search_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                for list in &this.lists {
                    list.invalidate_filter();
                    list.invalidate_headers();
                }
            }
        });

        let sort = gio::SimpleAction::new_stateful("sort", Some(glib::VariantTy::STRING), &"memory".to_variant());
        let weak = Rc::downgrade(&this);
        sort.connect_activate(move |action, value| {
            let Some(this) = weak.upgrade() else { return };
            let v = value.and_then(|v| v.str().map(str::to_string)).unwrap_or_else(|| "memory".into());
            action.set_state(&v.to_variant());
            this.sort.set(match v.as_str() {
                "cpu" => Sort::Cpu,
                "name" => Sort::Name,
                _ => Sort::Memory,
            });
            for list in &this.lists {
                list.invalidate_sort();
                list.invalidate_headers();
            }
        });
        window.add_action(&sort);
        let page = gio::SimpleAction::new("page", Some(glib::VariantTy::STRING));
        let pages_stack = stack.clone();
        page.connect_activate(move |_, v| {
            if let Some(name) = v.and_then(|v| v.str().map(str::to_string)) {
                pages_stack.set_visible_child_name(&name);
            }
        });
        window.add_action(&page);
        let toggle = gio::SimpleAction::new("search", None);
        toggle.connect_activate(move |_, _| find.set_active(!find.is_active()));
        window.add_action(&toggle);

        // read now, again shortly (processor use needs two readings), then every 2 seconds
        this.refresh();
        let weak = Rc::downgrade(&this);
        glib::timeout_add_local_once(Duration::from_millis(700), move || {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });
        let weak = Rc::downgrade(&this);
        glib::timeout_add_local(Duration::from_secs(2), move || match weak.upgrade() {
            Some(this) => {
                this.refresh();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        // keep the app alive as long as its window
        let keep = this.clone();
        window.connect_destroy(move |_| {
            keep.detail.borrow_mut().take();
        });
        window.present();
    }

    fn refresh(self: &Rc<Self>) {
        if self.busy.replace(true) {
            return;
        }
        let collector = self.collector.clone();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let snap = gio::spawn_blocking(move || collector.lock().unwrap().collect()).await;
            this.busy.set(false);
            if let Ok(snap) = snap {
                this.apply(snap);
            }
        });
    }

    fn apply(self: &Rc<Self>, snap: Snapshot) {
        self.show_summary(&snap.summary);
        let mut counts = [0u32; 3];
        let seen: HashSet<String> = snap.tasks.iter().map(|t| t.id.clone()).collect();
        *self.tasks.borrow_mut() = snap.tasks.iter().map(|t| (t.id.clone(), t.clone())).collect();
        {
            let mut rows = self.rows.borrow_mut();
            for t in &snap.tasks {
                counts[section(t.kind)] += 1;
                match rows.get(&t.id) {
                    Some(r) => r.update(t),
                    None => {
                        let r = self.make_row(t);
                        self.lists[r.section].append(&r.row);
                        rows.insert(t.id.clone(), r);
                    }
                }
            }
            rows.retain(|id, r| {
                let keep = seen.contains(id);
                if !keep {
                    self.lists[r.section].remove(&r.row);
                }
                keep
            });
        }
        for (list, (page, n)) in self.lists.iter().zip(self.pages.iter().zip(counts)) {
            list.invalidate_sort();
            list.invalidate_headers();
            page.set_badge_number(n);
        }
        if let Some(d) = self.detail.borrow().as_ref() {
            d.update(self.tasks.borrow().get(&d.id));
        }
    }

    fn show_summary(&self, s: &Summary) {
        let [cpu, mem, disk, up] = &self.stats;
        cpu.value.set_label(&format!("{:.0}%", s.cpu));
        cpu.detail.set_label(&format!("{} processes", s.processes));
        cpu.bar.set_value(s.cpu / 100.0);
        mem.value.set_label(&format!("{} used", bytes(s.mem_used)));
        mem.detail.set_label(&format!("of {} · {:.0}%", bytes(s.mem_total), pct(s.mem_used, s.mem_total)));
        mem.value.set_tooltip_text(Some(&format!("Overflow to disk (swap): {} of {}", bytes(s.swap_used), bytes(s.swap_total))));
        mem.bar.set_value(pct(s.mem_used, s.mem_total) / 100.0);
        disk.value.set_label(&format!("{} free", bytes(s.disk_free)));
        disk.detail.set_label(&format!("of {}", bytes(s.disk_total)));
        disk.bar.set_value(1.0 - pct(s.disk_free, s.disk_total) / 100.0);
        up.value.set_label(&duration(s.uptime));
        up.detail.set_label("since start-up");
    }

    fn compare(&self, x: &Task, y: &Task) -> std::cmp::Ordering {
        let by_group = group(x).0.cmp(&group(y).0);
        if by_group != std::cmp::Ordering::Equal {
            return by_group;
        }
        let by_memory = y.memory.cmp(&x.memory);
        match self.sort.get() {
            Sort::Memory => by_memory,
            Sort::Cpu => y.cpu.partial_cmp(&x.cpu).unwrap_or(std::cmp::Ordering::Equal).then(by_memory),
            Sort::Name => x.name.to_lowercase().cmp(&y.name.to_lowercase()),
        }
    }

    fn make_row(self: &Rc<Self>, t: &Task) -> Row {
        let icon = image(&t.icon, 32);
        let name = label(&t.name, "task-name");
        let about = label(&t.about, "task-about");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        text.append(&name);
        text.append(&about);
        let memory = label("", "task-memory");
        memory.set_xalign(1.0);
        let cpu = label("", "task-cpu");
        cpu.set_xalign(1.0);
        let numbers = gtk::Box::new(gtk::Orientation::Vertical, 2);
        numbers.set_valign(gtk::Align::Center);
        numbers.set_width_request(96);
        numbers.append(&memory);
        numbers.append(&cpu);
        let stop = gtk::Button::builder().icon_name("process-stop-symbolic").tooltip_text("Stop").valign(gtk::Align::Center).build();
        stop.add_css_class("flat");
        stop.add_css_class("circular");
        stop.add_css_class("row-stop");
        let id = t.id.clone();
        let weak = Rc::downgrade(self);
        stop.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.ask(&id, Action::Stop);
            }
        });
        let more = gtk::Image::from_icon_name("go-next-symbolic");
        more.add_css_class("dim-label");
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        line.append(&icon);
        line.append(&text);
        line.append(&numbers);
        line.append(&stop);
        line.append(&more);
        let row = gtk::ListBoxRow::builder().child(&line).activatable(true).build();
        row.set_widget_name(&t.id);
        row.add_css_class("task-row");
        let r = Row { row, name, about, memory, cpu, section: section(t.kind) };
        r.update(t);
        r
    }

    // --- the details window ------------------------------------------------------------------

    fn show(self: &Rc<Self>, id: &str) {
        let Some(t) = self.tasks.borrow().get(id).cloned() else { return };
        let dialog = adw::Dialog::new();
        dialog.set_title(&t.name);
        dialog.set_content_width(600);
        dialog.set_content_height(760);

        let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
        page.set_margin_top(6);
        page.set_margin_bottom(24);
        page.set_margin_start(24);
        page.set_margin_end(24);

        // who it is
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        top.append(&image(&t.icon, 56));
        let words = gtk::Box::new(gtk::Orientation::Vertical, 6);
        words.set_valign(gtk::Align::Center);
        let title = label(&t.name, "title-2");
        title.set_wrap(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::None);
        let about = label(&t.about, "task-about");
        about.set_wrap(true);
        about.set_ellipsize(gtk::pango::EllipsizeMode::None);
        let tags = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let status = tag("Running", "running");
        tags.append(&status);
        tags.append(&tag(kind_label(&t), ""));
        match t.safety {
            Safety::Needed => tags.append(&tag("Needed by the system", "needed")),
            Safety::Optional => tags.append(&tag("Safe to stop", "safe")),
            Safety::Unknown => {}
        }
        words.append(&title);
        words.append(&about);
        words.append(&tags);
        top.append(&words);
        page.append(&top);

        // the live numbers
        let grid = gtk::Grid::builder().column_spacing(10).row_spacing(10).column_homogeneous(true).build();
        let (memory, cpu, running, count) = (big(), big(), big(), big());
        for (i, (caption, value)) in [("Memory", &memory), ("Processor", &cpu), ("Running for", &running), ("Processes", &count)].into_iter().enumerate() {
            let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
            tile.add_css_class("stat");
            let c = label(caption, "caption-heading");
            tile.append(&c);
            tile.append(value);
            grid.attach(&tile, (i % 2) as i32, (i / 2) as i32, 1, 1);
        }
        page.append(&grid);

        // what you can do
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        buttons.set_homogeneous(true);
        let stop = gtk::Button::with_label(if t.kind == Kind::App { "Close app" } else { "Stop" });
        stop.add_css_class("destructive-action");
        stop.add_css_class("pill");
        let restartable = matches!(t.kind, Kind::Service | Kind::UserService | Kind::Container);
        let second = gtk::Button::with_label(if restartable { "Restart" } else { "Force stop" });
        second.add_css_class("pill");
        second.set_tooltip_text(Some(if restartable { "Stop it and start it again" } else { "End it right away, without letting it save" }));
        buttons.append(&stop);
        buttons.append(&second);
        page.append(&buttons);
        let note = gtk::Label::new(Some(match t.safety {
            Safety::Needed => "Your computer needs this. Stopping it can break things until you restart.",
            _ if t.kind == Kind::Container => "Stopping a container is safe; start it again any time with docker start.",
            _ if !t.mine && t.kind != Kind::Container => "This belongs to the system, so you'll be asked for your password.",
            _ => "Stopping it only affects this one; nothing else depends on it.",
        }));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class(if t.safety == Safety::Needed { "note-warning" } else { "note" });
        page.append(&note);

        // start with the computer
        let boot_group = adw::PreferencesGroup::new();
        let boot = adw::SwitchRow::builder().title("Start when the computer turns on").build();
        boot_group.add(&boot);
        let boot_note = gtk::Label::new(None);
        boot_note.set_xalign(0.0);
        boot_note.set_wrap(true);
        boot_note.add_css_class("note");
        boot_group.set_visible(false);
        boot_note.set_visible(false);
        page.append(&boot_group);
        page.append(&boot_note);

        // more, folded away
        let more_group = adw::PreferencesGroup::new();
        let more = adw::ExpanderRow::builder().title("More details").subtitle("Names, files, ports and settings").build();
        more_group.add(&more);
        if t.procs.len() > 1 {
            let procs = adw::ExpanderRow::builder().title(format!("Processes ({})", t.procs.len())).subtitle("The separate programs it's made of").build();
            let mut list = t.procs.clone();
            list.sort_by(|a, b| b.memory.cmp(&a.memory));
            for p in list.iter().take(60) {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&p.name).as_str())
                    .subtitle(glib::markup_escape_text(&format!("PID {} · {}", p.pid, collect::short_cmd(&p.cmd))).as_str())
                    .build();
                let m = label(&bytes(p.memory), "task-cpu");
                row.add_suffix(&m);
                procs.add_row(&row);
            }
            more_group.add(&procs);
        }
        let has_log = matches!(t.kind, Kind::Service | Kind::UserService | Kind::Container);
        if has_log {
            let messages = adw::ExpanderRow::builder().title("Recent messages").subtitle("What it last wrote to its log").build();
            let text = gtk::Label::new(Some("Loading…"));
            text.set_xalign(0.0);
            text.set_wrap(true);
            text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            text.set_selectable(true);
            text.add_css_class("log");
            text.set_margin_top(10);
            text.set_margin_bottom(10);
            text.set_margin_start(12);
            text.set_margin_end(12);
            messages.add_row(&text);
            let loaded = Cell::new(false);
            let task = t.clone();
            messages.connect_expanded_notify(move |row| {
                if row.is_expanded() && !loaded.replace(true) {
                    let task = task.clone();
                    let text = text.clone();
                    glib::spawn_future_local(async move {
                        let log = gio::spawn_blocking(move || act::log(&task)).await.unwrap_or_default();
                        text.set_label(&log);
                    });
                }
            });
            more_group.add(&messages);
        }
        page.append(&more_group);

        let clamp = adw::Clamp::builder().maximum_size(560).child(&page).build();
        let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&clamp).build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        view.set_content(Some(&scroller));
        dialog.set_child(Some(&view));

        for (button, action) in [(&stop, Action::Stop), (&second, if restartable { Action::Restart } else { Action::ForceStop })] {
            let weak = Rc::downgrade(self);
            let id = t.id.clone();
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.ask(&id, action);
                }
            });
        }

        // fill in the slow parts
        let task = t.clone();
        let weak = Rc::downgrade(self);
        let updating = Rc::new(Cell::new(false));
        let flag = updating.clone();
        let boot_row = boot.clone();
        glib::spawn_future_local(async move {
            let Ok(d) = gio::spawn_blocking(move || act::details(&task)).await else { return };
            for (k, v) in d.rows {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&k).as_str())
                    .subtitle(glib::markup_escape_text(&v).as_str())
                    .subtitle_selectable(true)
                    .build();
                row.add_css_class("property");
                more.add_row(&row);
            }
            if let Some(on) = d.boot {
                flag.set(true);
                boot_row.set_active(on);
                flag.set(false);
                boot_group.set_visible(true);
            }
            if let Some(n) = d.boot_note {
                boot_note.set_label(&n);
                boot_note.set_visible(true);
            }
            let _ = weak;
        });
        let weak = Rc::downgrade(self);
        let id = t.id.clone();
        boot.connect_active_notify(move |row| {
            if updating.get() {
                return;
            }
            let Some(this) = weak.upgrade() else { return };
            let on = row.is_active();
            let Some(task) = this.tasks.borrow().get(&id).cloned() else { return };
            let target = act::Target::from(&task);
            let row = row.clone();
            let updating = updating.clone();
            glib::spawn_future_local(async move {
                let res = gio::spawn_blocking(move || act::run(&target, Action::Boot(on))).await;
                match res {
                    Ok(Ok(msg)) => this.toast(&msg),
                    Ok(Err(e)) => {
                        this.toast(&e);
                        updating.set(true);
                        row.set_active(!on);
                        updating.set(false);
                    }
                    Err(_) => this.toast("It didn't work."),
                }
            });
        });

        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |d| {
            if let Some(this) = weak.upgrade() {
                let mut open = this.detail.borrow_mut();
                if open.as_ref().map(|o| &o.dialog == d).unwrap_or(false) {
                    *open = None;
                }
            }
        });
        let detail = Detail { id: t.id.clone(), dialog: dialog.clone(), status, memory, cpu, running, count, stop, second };
        detail.update(Some(&t));
        *self.detail.borrow_mut() = Some(detail);
        dialog.present(Some(&self.window));
    }

    // --- stopping ------------------------------------------------------------------------------

    /// Stop (or restart) something, asking first when the system needs it.
    fn ask(self: &Rc<Self>, id: &str, action: Action) {
        let Some(t) = self.tasks.borrow().get(id).cloned() else { return };
        if t.safety != Safety::Needed || matches!(action, Action::Boot(_)) {
            self.act(t, action);
            return;
        }
        let verb = if action == Action::Restart { "Restart" } else { "Stop" };
        let alert = adw::AlertDialog::new(
            Some(&format!("{verb} {}?", t.name)),
            Some(&format!("{}\n\nYour computer needs this. {verb}ping it can break things until you restart the computer.", t.about)),
        );
        let go = format!("{verb} anyway");
        alert.add_responses(&[("cancel", "Cancel"), ("go", go.as_str())]);
        alert.set_response_appearance("go", adw::ResponseAppearance::Destructive);
        alert.set_default_response(Some("cancel"));
        alert.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        alert.connect_response(None, move |_, response| {
            if response == "go" {
                if let Some(this) = weak.upgrade() {
                    this.act(t.clone(), action);
                }
            }
        });
        let parent: gtk::Widget = match self.detail.borrow().as_ref() {
            Some(d) => d.dialog.clone().upcast(),
            None => self.window.clone().upcast(),
        };
        alert.present(Some(&parent));
    }

    fn act(self: &Rc<Self>, t: Task, action: Action) {
        let target = act::Target::from(&t);
        let busy = match action {
            Action::Restart => format!("Restarting {}…", t.name),
            _ => format!("Stopping {}…", t.name),
        };
        let pending = jungey_ui::toast(&self.toasts, &busy);
        if let Some(d) = self.detail.borrow().as_ref() {
            d.stop.set_sensitive(false);
            d.second.set_sensitive(false);
        }
        let this = self.clone();
        let id = t.id.clone();
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || act::run(&target, action)).await;
            pending.dismiss();
            let stopped = matches!(action, Action::Stop | Action::ForceStop);
            match res {
                Ok(Ok(msg)) => {
                    this.toast(&msg);
                    if stopped {
                        let open = this.detail.borrow().as_ref().filter(|d| d.id == id).map(|d| d.dialog.clone());
                        if let Some(dialog) = open {
                            dialog.close();
                        }
                    }
                }
                Ok(Err(e)) => this.toast(&e),
                Err(_) => this.toast("It didn't work."),
            }
            if let Some(d) = this.detail.borrow().as_ref() {
                d.stop.set_sensitive(true);
                d.second.set_sensitive(true);
            }
            // processes take a moment to exit: look again now and in a second
            this.refresh();
            let weak = Rc::downgrade(&this);
            glib::timeout_add_local_once(Duration::from_millis(1200), move || {
                if let Some(this) = weak.upgrade() {
                    this.refresh();
                }
            });
        });
    }

    fn toast(&self, text: &str) {
        jungey_ui::toast(&self.toasts, text);
    }
}

impl Row {
    fn update(&self, t: &Task) {
        self.name.set_label(&t.name);
        self.about.set_label(&t.about);
        self.memory.set_label(&if t.memory > 0 { bytes(t.memory) } else { "—".into() });
        self.cpu.set_label(&format!("CPU {}", cpu(t.cpu)));
    }
}

impl Detail {
    fn update(&self, t: Option<&Task>) {
        match t {
            Some(t) => {
                self.status.set_label("Running");
                self.status.remove_css_class("stopped");
                self.status.add_css_class("running");
                self.memory.set_label(&if t.memory > 0 { bytes(t.memory) } else { "—".into() });
                self.cpu.set_label(&cpu(t.cpu));
                self.running.set_label(&t.started.map(|s| duration(now().saturating_sub(s))).unwrap_or_else(|| "—".into()));
                self.count.set_label(&t.procs.len().to_string());
            }
            None => {
                self.status.set_label("Stopped");
                self.status.remove_css_class("running");
                self.status.add_css_class("stopped");
                for l in [&self.memory, &self.cpu, &self.running, &self.count] {
                    l.set_label("—");
                }
                self.stop.set_sensitive(false);
                self.second.set_sensitive(false);
            }
        }
    }
}

/// `jungey-tasks --list`: the same three lists as text, for a terminal.
fn print_list() {
    let mut c = collect::Collector::new();
    c.collect();
    std::thread::sleep(Duration::from_millis(800));
    let snap = c.collect();
    let mut tasks = snap.tasks;
    tasks.sort_by(|a, b| (section(a.kind), group(a).0).cmp(&(section(b.kind), group(b).0)).then(b.memory.cmp(&a.memory)));
    let mut last = (usize::MAX, u8::MAX);
    for t in &tasks {
        let here = (section(t.kind), group(t).0);
        if here != last {
            let heading = match group(t).1 {
                "" => SECTIONS[here.0].0.to_uppercase(),
                h => h.to_string(),
            };
            println!("\n{heading}");
            last = here;
        }
        let safety = match t.safety {
            Safety::Needed => " [needed]",
            Safety::Optional => "",
            Safety::Unknown => " [?]",
        };
        println!("  {:<34} {:>8} {:>5}  {}{}", t.name, bytes(t.memory), cpu(t.cpu), t.about, safety);
    }
}

// --- small pieces --------------------------------------------------------------------------------

fn stat_tile(caption: &str, with_bar: bool) -> (gtk::Box, Stat) {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 3);
    tile.add_css_class("stat");
    tile.append(&label(caption, "caption-heading"));
    let value = label("…", "stat-value");
    let detail = label("", "stat-detail");
    let bar = gtk::LevelBar::new();
    bar.set_min_value(0.0);
    bar.set_max_value(1.0);
    bar.set_margin_top(6);
    bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_LOW));
    bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_HIGH));
    bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_FULL));
    bar.set_visible(with_bar);
    tile.append(&value);
    tile.append(&detail);
    tile.append(&bar);
    (tile, Stat { value, detail, bar })
}

fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l
}

fn big() -> gtk::Label {
    label("—", "stat-value")
}

fn tag(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("tag");
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l
}

/// An icon from the theme, or from a file when the app names one.
fn image(icon: &str, size: i32) -> gtk::Image {
    let img = if icon.starts_with('/') {
        gtk::Image::from_file(icon)
    } else {
        gtk::Image::from_icon_name(icon)
    };
    img.set_pixel_size(size);
    img
}

fn matches(t: &Task, q: &str) -> bool {
    t.name.to_lowercase().contains(q)
        || t.about.to_lowercase().contains(q)
        || t.unit.as_deref().map(|u| u.to_lowercase().contains(q)).unwrap_or(false)
        || t.procs.iter().any(|p| p.pid.to_string() == q || p.name.to_lowercase().contains(q))
}

fn bytes(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{b:.0} B")
    }
}

fn cpu(p: f64) -> String {
    if p <= 0.05 {
        "0%".into()
    } else if p < 1.0 {
        "<1%".into()
    } else {
        format!("{p:.0}%")
    }
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}

fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        format!("{d} d {h} h")
    } else if h > 0 {
        format!("{h} h {m} min")
    } else if m > 0 {
        format!("{m} min")
    } else {
        format!("{secs} s")
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
