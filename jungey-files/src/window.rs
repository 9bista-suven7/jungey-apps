//! One Jungey Files window: sidebar, breadcrumbs, the file grid/list, and every action.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib, graphene};

use crate::config::{Config, SortKey};
use crate::ops::{self, Transfer};
use crate::pathbar::PathBar;
use crate::sidebar::Sidebar;
use crate::thumbs::Thumbs;
use crate::{clipboard, props, rename, util};

const GRID_ICON: i32 = 64;
const LIST_ICON: i32 = 24;
// name-based types only: sniffing every file's contents is slow on a hard drive
const ATTRS_LOCAL: &str = "standard::name,standard::display-name,standard::type,standard::size,\
standard::is-hidden,standard::is-backup,standard::fast-content-type,standard::is-symlink,\
time::modified,thumbnail::path,thumbnail::is-valid,access::can-trash";
const ATTRS_OTHER: &str = "standard::*,time::modified,thumbnail::path,thumbnail::is-valid,\
trash::orig-path,trash::deletion-date";

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<Win>>> = RefCell::default();
}

/// What the sort and filter functions read.
struct ViewState {
    show_hidden: Cell<bool>,
    search: RefCell<String>,
    sort: Cell<SortKey>,
    descending: Cell<bool>,
}

pub struct Win {
    pub window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    state: Rc<ViewState>,
    dir_list: gtk::DirectoryList,
    filter: gtk::CustomFilter,
    sorter: gtk::CustomSorter,
    sorted: gtk::SortListModel,
    selection: gtk::MultiSelection,
    grid: gtk::GridView,
    list: gtk::ColumnView,
    grid_scroll: gtk::ScrolledWindow,
    list_scroll: gtk::ScrolledWindow,
    views: gtk::Stack,
    content: gtk::Stack,
    empty_page: adw::StatusPage,
    error_page: adw::StatusPage,
    trash_banner: adw::Banner,
    pathbar: PathBar,
    location: gtk::Entry,
    title_stack: gtk::Stack,
    search_bar: gtk::SearchBar,
    search_entry: gtk::SearchEntry,
    view_button: gtk::Button,
    spinner: gtk::Spinner,
    sidebar: Rc<Sidebar>,
    status: gtk::Label,
    free: gtk::Label,
    popover: gtk::PopoverMenu,
    thumbs: Rc<Thumbs>,
    cfg: RefCell<Config>,
    current: RefCell<gio::File>,
    back: RefCell<Vec<gio::File>>,
    forward: RefCell<Vec<gio::File>>,
    pending_select: RefCell<Vec<gio::File>>,
}

pub fn set_accels(app: &adw::Application) {
    for (action, keys) in [
        ("win.back", &["<Alt>Left"][..]),
        ("win.forward", &["<Alt>Right"]),
        ("win.up", &["<Alt>Up"]),
        ("win.home", &["<Alt>Home"]),
        ("win.reload", &["F5", "<Control>r"]),
        ("win.show-hidden", &["<Control>h"]),
        ("win.location", &["<Control>l"]),
        ("win.search", &["<Control>f"]),
        ("win.new-folder", &["<Control><Shift>n"]),
        ("win.new-window", &["<Control>n"]),
        ("win.bookmark", &["<Control>d"]),
        ("win.properties", &["<Alt>Return"]),
        ("win.open-terminal", &["F4"]),
        ("win.view::grid", &["<Control>1"]),
        ("win.view::list", &["<Control>2"]),
        ("window.close", &["<Control>w"]),
    ] {
        app.set_accels_for_action(action, keys);
    }
}

impl Win {
    /// Open a new window at `dir`, optionally highlighting `select` inside it.
    pub fn open(app: &adw::Application, dir: &gio::File, select: Option<gio::File>) {
        let win = Win::new(app, dir);
        if let Some(f) = select {
            win.select_when_shown(vec![f]);
        }
        win.window.present();
        WINDOWS.with(|w| w.borrow_mut().push(win));
    }

    fn new(app: &adw::Application, start: &gio::File) -> Rc<Win> {
        let cfg = Config::load();
        let state = Rc::new(ViewState {
            show_hidden: Cell::new(cfg.show_hidden),
            search: RefCell::default(),
            sort: Cell::new(cfg.sort),
            descending: Cell::new(cfg.descending),
        });

        // --- model: folder contents -> hide/search -> sort -> selection
        let dir_list = gtk::DirectoryList::new(Some(ATTRS_LOCAL), None::<&gio::File>);
        let st = state.clone();
        let filter = gtk::CustomFilter::new(move |obj| {
            let info = obj.downcast_ref::<gio::FileInfo>().unwrap();
            if !st.show_hidden.get() && (info.is_hidden() || info.is_backup()) {
                return false;
            }
            let q = st.search.borrow();
            q.is_empty() || info.display_name().to_lowercase().contains(q.as_str())
        });
        let filtered = gtk::FilterListModel::new(Some(dir_list.clone()), Some(filter.clone()));
        let st = state.clone();
        let sorter = gtk::CustomSorter::new(move |a, b| {
            let a = a.downcast_ref::<gio::FileInfo>().unwrap();
            let b = b.downcast_ref::<gio::FileInfo>().unwrap();
            let folders_first = util::is_dir(b).cmp(&util::is_dir(a));
            if folders_first.is_ne() {
                return folders_first.into();
            }
            let mut o = match st.sort.get() {
                SortKey::Name => std::cmp::Ordering::Equal,
                SortKey::Modified => a.attribute_uint64("time::modified").cmp(&b.attribute_uint64("time::modified")),
                SortKey::Size => a.size().cmp(&b.size()),
                SortKey::Type => util::content_type(a).cmp(&util::content_type(b)),
            };
            if o.is_eq() {
                o = util::natural_cmp(&a.display_name(), &b.display_name());
            }
            if st.descending.get() {
                o = o.reverse();
            }
            o.into()
        });
        let sorted = gtk::SortListModel::new(Some(filtered), Some(sorter.clone()));
        let selection = gtk::MultiSelection::new(Some(sorted.clone()));

        // --- views
        let grid = gtk::GridView::new(Some(selection.clone()), None::<gtk::ListItemFactory>);
        grid.add_css_class("jungey-grid");
        grid.set_max_columns(40);
        grid.set_enable_rubberband(true);
        let list = gtk::ColumnView::new(Some(selection.clone()));
        list.add_css_class("jungey-list");
        list.set_enable_rubberband(true);
        let grid_scroll = gtk::ScrolledWindow::builder().child(&grid).vexpand(true).build();
        let list_scroll = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();
        let views = gtk::Stack::new();
        views.add_named(&grid_scroll, Some("grid"));
        views.add_named(&list_scroll, Some("list"));

        let empty_page = adw::StatusPage::builder().icon_name("folder-symbolic").build();
        let error_page = adw::StatusPage::builder().icon_name("dialog-warning-symbolic").title("Can't open this folder").build();
        let content = gtk::Stack::new();
        content.set_transition_type(gtk::StackTransitionType::Crossfade);
        content.add_named(&views, Some("views"));
        content.add_named(&empty_page, Some("empty"));
        content.add_named(&error_page, Some("error"));

        let trash_banner = adw::Banner::new("Items in the Trash can be put back, or deleted for good");
        trash_banner.set_button_label(Some("Empty Trash"));
        trash_banner.set_action_name(Some("win.empty-trash"));

        let search_entry = gtk::SearchEntry::builder().placeholder_text("Search this folder").hexpand(true).build();
        let clamp = adw::Clamp::builder().maximum_size(520).child(&search_entry).build();
        let search_bar = gtk::SearchBar::builder().child(&clamp).build();
        search_bar.connect_entry(&search_entry);

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.append(&trash_banner);
        main_box.append(&content);

        // --- status bar
        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.set_hexpand(true);
        status.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let free = gtk::Label::new(None);
        let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status_bar.add_css_class("jungey-statusbar");
        status_bar.append(&status);
        status_bar.append(&free);

        // --- header
        let pathbar = PathBar::new();
        let location = gtk::Entry::builder().hexpand(true).build();
        let title_stack = gtk::Stack::new();
        title_stack.set_hexpand(true);
        title_stack.add_named(&pathbar.widget, Some("path"));
        title_stack.add_named(&location, Some("entry"));

        let header = adw::HeaderBar::new();
        header.set_show_start_title_buttons(false);
        let sidebar_toggle = gtk::ToggleButton::builder().icon_name("sidebar-show-symbolic").tooltip_text("Show Sidebar").visible(false).build();
        header.pack_start(&sidebar_toggle);
        for (icon, action, tip) in [
            ("go-previous-symbolic", "win.back", "Back (Alt+Left)"),
            ("go-next-symbolic", "win.forward", "Forward (Alt+Right)"),
            ("go-up-symbolic", "win.up", "Up a Folder (Alt+Up)"),
        ] {
            let b = gtk::Button::builder().icon_name(icon).action_name(action).tooltip_text(tip).build();
            header.pack_start(&b);
        }
        header.set_title_widget(Some(&title_stack));
        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&main_menu())
            .primary(true)
            .tooltip_text("Menu")
            .build();
        header.pack_end(&menu);
        let view_button = gtk::Button::new();
        header.pack_end(&view_button);
        let search_toggle = gtk::ToggleButton::builder().icon_name("system-search-symbolic").tooltip_text("Search (Ctrl+F)").build();
        search_bar.bind_property("search-mode-enabled", &search_toggle, "active").bidirectional().sync_create().build();
        header.pack_end(&search_toggle);
        let spinner = gtk::Spinner::new();
        header.pack_end(&spinner);

        let page = adw::ToolbarView::new();
        page.add_top_bar(&header);
        page.add_top_bar(&search_bar);
        page.set_content(Some(&main_box));
        page.add_bottom_bar(&status_bar);

        // --- sidebar
        let sidebar = Sidebar::new();
        let side_header = adw::HeaderBar::new();
        side_header.set_show_end_title_buttons(false);
        let brand = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let logo = gtk::Image::from_icon_name("jungey-files");
        logo.set_pixel_size(22);
        let name = gtk::Label::new(Some("Files"));
        name.add_css_class("heading");
        brand.append(&logo);
        brand.append(&name);
        side_header.set_title_widget(Some(&brand));
        let side_page = adw::ToolbarView::new();
        side_page.add_top_bar(&side_header);
        side_page.set_content(Some(&sidebar.widget));

        let split = adw::OverlaySplitView::new();
        split.set_sidebar(Some(&side_page));
        split.set_content(Some(&page));
        split.set_min_sidebar_width(190.0);
        split.set_max_sidebar_width(250.0);
        split.bind_property("show-sidebar", &sidebar_toggle, "active").bidirectional().sync_create().build();
        split.bind_property("collapsed", &sidebar_toggle, "visible").sync_create().build();

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(cfg.width)
            .default_height(cfg.height)
            .maximized(cfg.maximized)
            .content(&toasts)
            .width_request(360)
            .height_request(300)
            .build();
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 640sp").unwrap());
        narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(narrow);
        search_bar.set_key_capture_widget(Some(&window));

        let popover = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        popover.set_has_arrow(false);
        popover.set_halign(gtk::Align::Start);
        popover.set_parent(&content);

        let win = Rc::new(Win {
            window,
            toasts,
            state,
            dir_list,
            filter,
            sorter,
            sorted,
            selection,
            grid,
            list,
            grid_scroll,
            list_scroll,
            views,
            content,
            empty_page,
            error_page,
            trash_banner,
            pathbar,
            location,
            title_stack,
            search_bar,
            search_entry,
            view_button,
            spinner,
            sidebar,
            status,
            free,
            popover,
            thumbs: Thumbs::new(),
            cfg: RefCell::new(cfg),
            current: RefCell::new(start.clone()),
            back: RefCell::default(),
            forward: RefCell::default(),
            pending_select: RefCell::default(),
        });
        win.grid.set_factory(Some(&win.grid_factory()));
        win.build_columns();
        win.connect_signals();
        win.add_actions();
        win.add_shortcuts();
        let list_view = win.cfg.borrow().list_view;
        win.set_view(list_view);
        win.load();
        win
    }

    // ---------------------------------------------------------------- helpers

    fn current(&self) -> gio::File {
        self.current.borrow().clone()
    }

    fn in_trash(&self) -> bool {
        self.current().uri_scheme().as_deref() == Some("trash")
    }

    fn toast(&self, text: &str) -> adw::Toast {
        jungey_ui::toast(&self.toasts, text)
    }

    fn info_at(&self, pos: u32) -> Option<gio::FileInfo> {
        self.sorted.item(pos).and_downcast()
    }

    fn selected_infos(&self) -> Vec<gio::FileInfo> {
        let set = self.selection.selection();
        (0..set.size()).filter_map(|i| self.info_at(set.nth(i as u32))).collect()
    }

    fn selected_files(&self) -> Vec<gio::File> {
        self.selected_infos().iter().map(util::file_of).collect()
    }

    /// Run blocking file work off the main thread. A "busy" toast appears only if
    /// it takes more than a moment.
    fn run_op<T: Send + 'static>(
        self: &Rc<Self>,
        busy: String,
        job: impl FnOnce() -> Result<T, glib::Error> + Send + 'static,
        done: impl FnOnce(&Rc<Win>, T) + 'static,
    ) {
        let finished = Rc::new(Cell::new(false));
        let busy_toast: Rc<RefCell<Option<adw::Toast>>> = Rc::default();
        {
            let (finished, busy_toast, overlay) = (finished.clone(), busy_toast.clone(), self.toasts.clone());
            glib::timeout_add_local_once(Duration::from_millis(400), move || {
                if !finished.get() {
                    let t = adw::Toast::new(&busy);
                    t.set_timeout(0);
                    overlay.add_toast(t.clone());
                    busy_toast.replace(Some(t));
                }
            });
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(job).await;
            finished.set(true);
            if let Some(t) = busy_toast.take() {
                t.dismiss();
            }
            let Some(win) = weak.upgrade() else { return };
            match res {
                Ok(Ok(v)) => done(&win, v),
                Ok(Err(e)) => {
                    win.toast(e.message());
                }
                Err(_) => {
                    win.toast("Something went wrong");
                }
            }
        });
    }

    // ------------------------------------------------------------ navigation

    fn navigate(self: &Rc<Self>, file: gio::File) {
        if self.current().equal(&file) {
            return;
        }
        let old = self.current.replace(file);
        self.back.borrow_mut().push(old);
        self.forward.borrow_mut().clear();
        self.load();
    }

    fn load(self: &Rc<Self>) {
        let file = self.current();
        self.thumbs.clear_queue();
        self.pending_select.borrow_mut().retain(|f| f.parent().map(|p| p.equal(&file)).unwrap_or(false));
        self.dir_list.set_attributes(Some(if file.is_native() { ATTRS_LOCAL } else { ATTRS_OTHER }));
        self.dir_list.set_file(Some(&file));
        self.search_bar.set_search_mode(false);
        self.grid_scroll.vadjustment().set_value(0.0);
        self.list_scroll.vadjustment().set_value(0.0);

        let weak = Rc::downgrade(self);
        self.pathbar.set(
            &file,
            Rc::new(move |f| {
                if let Some(w) = weak.upgrade() {
                    w.navigate(f);
                }
            }),
        );
        let title = if self.in_trash() {
            "Trash".to_string()
        } else {
            file.basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_else(|| file.uri().to_string())
        };
        self.window.set_title(Some(&format!("{title} — Jungey Files")));
        self.sidebar.select(&file);
        self.trash_banner.set_revealed(self.in_trash());
        self.update_nav_actions();
        self.update_content();
        self.focus_view();

        self.free.set_text("");
        let free = self.free.clone();
        glib::spawn_future_local(async move {
            if let Ok(fs) = file.query_filesystem_info_future("filesystem::free", glib::Priority::LOW).await {
                let bytes = fs.attribute_uint64("filesystem::free");
                if bytes > 0 {
                    free.set_text(&format!("{} free", util::human_size(bytes)));
                }
            }
        });
    }

    fn go_back(self: &Rc<Self>) {
        let Some(prev) = self.back.borrow_mut().pop() else { return };
        let cur = self.current.replace(prev);
        self.forward.borrow_mut().push(cur.clone());
        self.select_when_shown(vec![cur]);
        self.load();
    }

    fn go_forward(self: &Rc<Self>) {
        let Some(next) = self.forward.borrow_mut().pop() else { return };
        let cur = self.current.replace(next);
        self.back.borrow_mut().push(cur);
        self.load();
    }

    fn go_up(self: &Rc<Self>) {
        let cur = self.current();
        if let Some(parent) = cur.parent() {
            self.navigate(parent);
            self.select_when_shown(vec![cur]);
        }
    }

    fn update_nav_actions(&self) {
        let set = |name: &str, on: bool| {
            if let Some(a) = self.window.lookup_action(name).and_downcast::<gio::SimpleAction>() {
                a.set_enabled(on);
            }
        };
        set("back", !self.back.borrow().is_empty());
        set("forward", !self.forward.borrow().is_empty());
        set("up", self.current().parent().is_some());
    }

    fn focus_view(&self) {
        if self.views.visible_child_name().as_deref() == Some("list") {
            self.list.grab_focus();
        } else {
            self.grid.grab_focus();
        }
    }

    /// Select these files as soon as they show up (after paste, rename, going up...).
    fn select_when_shown(self: &Rc<Self>, files: Vec<gio::File>) {
        self.pending_select.replace(files);
        self.apply_pending_select();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_secs(4), move || {
            if let Some(w) = weak.upgrade() {
                w.pending_select.borrow_mut().clear();
            }
        });
    }

    fn apply_pending_select(&self) {
        let pending = self.pending_select.borrow().clone();
        if pending.is_empty() {
            return;
        }
        let n = self.sorted.n_items();
        let found: Vec<u32> = (0..n)
            .filter(|p| {
                self.info_at(*p)
                    .map(|i| {
                        let f = util::file_of(&i);
                        pending.iter().any(|x| x.equal(&f))
                    })
                    .unwrap_or(false)
            })
            .collect();
        let Some(&first) = found.first() else { return };
        let set = gtk::Bitset::new_empty();
        for p in &found {
            set.add(*p);
        }
        self.selection.set_selection(&set, &gtk::Bitset::new_range(0, n));
        if self.views.visible_child_name().as_deref() == Some("list") {
            self.list.scroll_to(first, None::<&gtk::ColumnViewColumn>, gtk::ListScrollFlags::FOCUS, None);
        } else {
            self.grid.scroll_to(first, gtk::ListScrollFlags::FOCUS, None);
        }
        if found.len() == pending.len() {
            self.pending_select.borrow_mut().clear();
        }
    }

    fn update_content(&self) {
        if let Some(err) = self.dir_list.error() {
            self.error_page.set_description(Some(err.message()));
            self.content.set_visible_child_name("error");
            return;
        }
        let loading = self.dir_list.is_loading();
        self.spinner.set_spinning(loading);
        self.spinner.set_visible(loading);
        if self.sorted.n_items() == 0 && !loading {
            let q = self.state.search.borrow().clone();
            let (icon, title, desc) = if !q.is_empty() {
                ("system-search-symbolic", "No results".to_string(), format!("Nothing here matches “{q}”"))
            } else if self.in_trash() {
                ("user-trash-symbolic", "Trash is empty".to_string(), String::new())
            } else {
                ("folder-symbolic", "This folder is empty".to_string(), "Drop files here, or paste with Ctrl+V".to_string())
            };
            self.empty_page.set_icon_name(Some(icon));
            self.empty_page.set_title(&title);
            self.empty_page.set_description(Some(&desc).filter(|d| !d.is_empty()).map(|d| d.as_str()));
            self.content.set_visible_child_name("empty");
        } else {
            self.content.set_visible_child_name("views");
        }
        self.update_status();
    }

    fn update_status(&self) {
        let n = self.sorted.n_items();
        let infos = self.selected_infos();
        let s = |k: u32| if k == 1 { "" } else { "s" };
        let text = match infos.as_slice() {
            [] => format!("{n} item{}", s(n)),
            [one] if util::is_dir(one) => format!("“{}” selected", one.display_name()),
            [one] => format!("“{}” selected · {}", one.display_name(), util::human_size(one.size() as u64)),
            many => {
                let bytes: u64 = many.iter().filter(|i| !util::is_dir(i)).map(|i| i.size() as u64).sum();
                format!("{} of {n} selected · {}", many.len(), util::human_size(bytes))
            }
        };
        self.status.set_text(&text);
    }

    fn set_view(self: &Rc<Self>, list: bool) {
        self.views.set_visible_child_name(if list { "list" } else { "grid" });
        self.view_button.set_icon_name(if list { "view-grid-symbolic" } else { "view-list-symbolic" });
        self.view_button.set_tooltip_text(Some(if list { "Show as Icons (Ctrl+1)" } else { "Show as List (Ctrl+2)" }));
        self.cfg.borrow_mut().list_view = list;
        if let Some(a) = self.window.lookup_action("view") {
            a.change_state(&(if list { "list" } else { "grid" }).to_variant());
        }
        self.focus_view();
    }

    // ----------------------------------------------------------------- views

    fn grid_factory(self: &Rc<Self>) -> gtk::SignalListItemFactory {
        let f = gtk::SignalListItemFactory::new();
        let weak = Rc::downgrade(self);
        f.connect_setup(move |_, obj| {
            let item = obj.downcast_ref::<gtk::ListItem>().unwrap();
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 6);
            cell.add_css_class("jungey-cell");
            let image = gtk::Image::new();
            image.set_pixel_size(GRID_ICON);
            let label = gtk::Label::new(None);
            label.add_css_class("name");
            label.set_wrap(true);
            label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            label.set_lines(2);
            label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            label.set_justify(gtk::Justification::Center);
            label.set_width_chars(12);
            label.set_max_width_chars(14);
            cell.append(&image);
            cell.append(&label);
            item.set_child(Some(&cell));
            if let Some(w) = weak.upgrade() {
                w.attach_cell(&cell, item);
            }
        });
        let weak = Rc::downgrade(self);
        f.connect_bind(move |_, obj| {
            let item = obj.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(info) = item.item().and_downcast::<gio::FileInfo>() else { return };
            let cell = item.child().and_downcast::<gtk::Box>().unwrap();
            let image = cell.first_child().and_downcast::<gtk::Image>().unwrap();
            let label = image.next_sibling().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&info.display_name());
            image.set_from_gicon(&util::icon_for(&info));
            set_dimmed(&cell, &info);
            if let Some(w) = weak.upgrade() {
                w.thumbs.request(&info, item, &image);
            }
        });
        f
    }

    fn build_columns(self: &Rc<Self>) {
        let f = gtk::SignalListItemFactory::new();
        let weak = Rc::downgrade(self);
        f.connect_setup(move |_, obj| {
            let item = obj.downcast_ref::<gtk::ListItem>().unwrap();
            let cell = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            cell.add_css_class("jungey-cell");
            let image = gtk::Image::new();
            image.set_pixel_size(LIST_ICON);
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            cell.append(&image);
            cell.append(&label);
            item.set_child(Some(&cell));
            if let Some(w) = weak.upgrade() {
                w.attach_cell(&cell, item);
            }
        });
        f.connect_bind(|_, obj| {
            let item = obj.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(info) = item.item().and_downcast::<gio::FileInfo>() else { return };
            let cell = item.child().and_downcast::<gtk::Box>().unwrap();
            let image = cell.first_child().and_downcast::<gtk::Image>().unwrap();
            let label = image.next_sibling().and_downcast::<gtk::Label>().unwrap();
            image.set_from_gicon(&util::icon_for(&info));
            label.set_text(&info.display_name());
            set_dimmed(&cell, &info);
        });
        let name = gtk::ColumnViewColumn::new(Some("Name"), Some(f));
        name.set_expand(true);
        name.set_resizable(true);
        self.list.append_column(&name);

        self.text_column("Size", 110, 1.0, |i| if util::is_dir(i) { "—".into() } else { util::human_size(i.size() as u64) });
        self.text_column("Modified", 170, 0.0, |i| util::friendly_time(i.attribute_uint64("time::modified")));
        self.text_column("Type", 180, 0.0, |i| type_description(&util::content_type(i)));
    }

    fn text_column(self: &Rc<Self>, title: &str, width: i32, xalign: f32, text: impl Fn(&gio::FileInfo) -> String + 'static) {
        let f = gtk::SignalListItemFactory::new();
        let weak = Rc::downgrade(self);
        f.connect_setup(move |_, obj| {
            let item = obj.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_xalign(xalign);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.add_css_class("dim-label");
            label.add_css_class("numeric");
            item.set_child(Some(&label));
            if let Some(w) = weak.upgrade() {
                w.attach_cell(&label, item);
            }
        });
        f.connect_bind(move |_, obj| {
            let item = obj.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(info) = item.item().and_downcast::<gio::FileInfo>() else { return };
            if let Some(label) = item.child().and_downcast::<gtk::Label>() {
                label.set_text(&text(&info));
            }
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(f));
        col.set_fixed_width(width);
        col.set_resizable(true);
        self.list.append_column(&col);
    }

    /// Right-click, drag out, and drop-into-folder for one cell.
    fn attach_cell(self: &Rc<Self>, widget: &impl IsA<gtk::Widget>, item: &gtk::ListItem) {
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_SECONDARY);
        let (weak, item_w) = (Rc::downgrade(self), item.downgrade());
        click.connect_pressed(move |g, _, x, y| {
            let (Some(win), Some(item)) = (weak.upgrade(), item_w.upgrade()) else { return };
            let pos = item.position();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }
            g.set_state(gtk::EventSequenceState::Claimed);
            if !win.selection.is_selected(pos) {
                win.selection.select_item(pos, true);
            }
            win.popup_menu(Option::<gtk::Widget>::from(g.widget()).as_ref(), x, y, true);
        });
        widget.add_controller(click);

        let drag = gtk::DragSource::new();
        drag.set_actions(gdk::DragAction::COPY | gdk::DragAction::MOVE);
        let (weak, item_w) = (Rc::downgrade(self), item.downgrade());
        drag.connect_prepare(move |src, _, _| {
            let (win, item) = (weak.upgrade()?, item_w.upgrade()?);
            let pos = item.position();
            if pos == gtk::INVALID_LIST_POSITION {
                return None;
            }
            if !win.selection.is_selected(pos) {
                win.selection.select_item(pos, true);
            }
            let files = win.selected_files();
            if files.is_empty() {
                return None;
            }
            if let Some(w) = Option::<gtk::Widget>::from(src.widget()) {
                src.set_icon(Some(&gtk::WidgetPaintable::new(Some(&w))), 0, 0);
            }
            Some(gdk::ContentProvider::for_value(&gdk::FileList::from_array(&files).to_value()))
        });
        widget.add_controller(drag);

        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE);
        let (weak, item_w) = (Rc::downgrade(self), item.downgrade());
        drop.connect_drop(move |t, value, _, _| {
            let (Some(win), Some(item)) = (weak.upgrade(), item_w.upgrade()) else { return false };
            let Ok(list) = value.get::<gdk::FileList>() else { return false };
            let dest = item
                .item()
                .and_downcast::<gio::FileInfo>()
                .filter(util::is_dir)
                .map(|i| util::file_of(&i))
                .unwrap_or_else(|| win.current());
            win.drop_files(list.files(), dest, t.current_event_state());
            true
        });
        widget.add_controller(drop);
    }

    fn drop_files(self: &Rc<Self>, files: Vec<gio::File>, dest: gio::File, mods: gdk::ModifierType) {
        let files: Vec<gio::File> = files
            .into_iter()
            .filter(|f| !f.equal(&dest) && f.parent().map(|p| !p.equal(&dest)).unwrap_or(true))
            .collect();
        if files.is_empty() || self.in_trash() {
            return;
        }
        let how = if mods.contains(gdk::ModifierType::CONTROL_MASK) {
            Transfer::Copy
        } else if mods.contains(gdk::ModifierType::SHIFT_MASK) || same_drive(&files[0], &dest) {
            Transfer::Move
        } else {
            Transfer::Copy
        };
        self.transfer(files, dest, how);
    }

    fn transfer(self: &Rc<Self>, files: Vec<gio::File>, dest: gio::File, how: Transfer) {
        let n = files.len();
        let (busy, done) = match how {
            Transfer::Copy => ("Copying", "Copied"),
            Transfer::Move => ("Moving", "Moved"),
        };
        let items = if n == 1 { "1 item".to_string() } else { format!("{n} items") };
        let d = dest.clone();
        self.run_op(
            format!("{busy} {items}…"),
            move || ops::transfer(&files, &d, how),
            move |win, new| {
                if win.current().equal(&dest) {
                    win.select_when_shown(new);
                }
                win.toast(&format!("{done} {items}"));
            },
        );
    }

    // ----------------------------------------------------------------- menus

    fn popup_menu(self: &Rc<Self>, from: Option<&gtk::Widget>, x: f64, y: f64, on_items: bool) {
        let model = if on_items { self.item_menu() } else { self.background_menu() };
        self.popover.set_menu_model(Some(&model));
        let at = graphene::Point::new(x as f32, y as f32);
        let p = from.and_then(|w| w.compute_point(&self.content, &at)).unwrap_or(at);
        self.popover.set_pointing_to(Some(&gdk::Rectangle::new(p.x() as i32, p.y() as i32, 1, 1)));
        self.popover.popup();
    }

    fn item_menu(&self) -> gio::Menu {
        let infos = self.selected_infos();
        let one_file = infos.len() == 1 && !util::is_dir(&infos[0]);
        let has_dirs = infos.iter().any(util::is_dir);
        let archives = !infos.is_empty() && infos.iter().all(|i| util::is_archive(&i.display_name()));
        let menu = gio::Menu::new();
        if self.in_trash() {
            let s = section(&menu);
            item(&s, "Put Back", "win.restore", None);
            item(&s, "Delete Permanently…", "win.trash", Some("Delete"));
            let s = section(&menu);
            item(&s, "Properties", "win.properties", Some("<Alt>Return"));
            return menu;
        }
        let s = section(&menu);
        item(&s, "Open", "win.open", Some("Return"));
        if one_file {
            item(&s, "Open With…", "win.open-with", None);
        }
        if has_dirs {
            item(&s, "Open in New Window", "win.open-new-window", None);
        }
        let s = section(&menu);
        item(&s, "Cut", "win.cut", Some("<Control>x"));
        item(&s, "Copy", "win.copy", Some("<Control>c"));
        item(&s, "Copy Path", "win.copy-path", Some("<Control><Shift>c"));
        let s = section(&menu);
        item(&s, if infos.len() > 1 { "Rename All…" } else { "Rename…" }, "win.rename", Some("F2"));
        if self.current().is_native() {
            item(&s, "Compress to ZIP", "win.compress", None);
            if archives {
                item(&s, "Extract Here", "win.extract", None);
            }
        }
        let s = section(&menu);
        item(&s, "Move to Trash", "win.trash", Some("Delete"));
        let s = section(&menu);
        item(&s, "Properties", "win.properties", Some("<Alt>Return"));
        menu
    }

    fn background_menu(&self) -> gio::Menu {
        let menu = gio::Menu::new();
        if self.in_trash() {
            let s = section(&menu);
            item(&s, "Empty Trash…", "win.empty-trash", None);
            item(&s, "Select All", "win.select-all", Some("<Control>a"));
            return menu;
        }
        let s = section(&menu);
        item(&s, "New Folder…", "win.new-folder", Some("<Control><Shift>n"));
        item(&s, "Paste", "win.paste", Some("<Control>v"));
        let s = section(&menu);
        item(&s, "Open Terminal Here", "win.open-terminal", Some("F4"));
        item(&s, "Select All", "win.select-all", Some("<Control>a"));
        let s = section(&menu);
        s.append_submenu(Some("Sort By"), &sort_menu());
        item(&s, "Show Hidden Files", "win.show-hidden", Some("<Control>h"));
        let s = section(&menu);
        item(&s, "Folder Properties", "win.folder-properties", None);
        menu
    }

    // --------------------------------------------------------------- signals

    fn connect_signals(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let refresh = move || {
            if let Some(w) = weak.upgrade() {
                w.update_content();
                w.apply_pending_select();
            }
        };
        let r = refresh.clone();
        self.dir_list.connect_loading_notify(move |_| r());
        let r = refresh.clone();
        self.dir_list.connect_error_notify(move |_| r());
        let r = refresh.clone();
        self.sorted.connect_items_changed(move |_, _, _, _| r());

        let weak = Rc::downgrade(self);
        self.selection.connect_selection_changed(move |_, _, _| {
            if let Some(w) = weak.upgrade() {
                w.update_status();
            }
        });

        for view in [self.grid.upcast_ref::<gtk::Widget>(), self.list.upcast_ref()] {
            let weak = Rc::downgrade(self);
            let activate = move |pos: u32| {
                if let Some(w) = weak.upgrade() {
                    w.activate_item(pos);
                }
            };
            if let Some(g) = view.downcast_ref::<gtk::GridView>() {
                let a = activate.clone();
                g.connect_activate(move |_, pos| a(pos));
            }
            if let Some(l) = view.downcast_ref::<gtk::ColumnView>() {
                l.connect_activate(move |_, pos| activate(pos));
            }
        }

        // right-click on empty space
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |_, _, x, y| {
            if let Some(w) = weak.upgrade() {
                w.selection.unselect_all();
                w.popup_menu(None, x, y, false);
            }
        });
        self.content.add_controller(click);

        // drop onto empty space = into this folder
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE);
        let weak = Rc::downgrade(self);
        drop.connect_drop(move |t, value, _, _| {
            let Some(w) = weak.upgrade() else { return false };
            let Ok(list) = value.get::<gdk::FileList>() else { return false };
            w.drop_files(list.files(), w.current(), t.current_event_state());
            true
        });
        self.content.add_controller(drop);

        // mouse back/forward buttons
        let nav = gtk::GestureClick::new();
        nav.set_button(0);
        let weak = Rc::downgrade(self);
        nav.connect_pressed(move |g, _, _, _| {
            let Some(w) = weak.upgrade() else { return };
            match g.current_button() {
                8 => w.go_back(),
                9 => w.go_forward(),
                _ => {}
            }
        });
        self.window.add_controller(nav);

        let weak = Rc::downgrade(self);
        self.search_entry.connect_search_changed(move |e| {
            let Some(w) = weak.upgrade() else { return };
            w.state.search.replace(e.text().to_lowercase());
            w.filter.changed(gtk::FilterChange::Different);
            w.update_content();
        });
        let weak = Rc::downgrade(self);
        self.search_bar.connect_search_mode_enabled_notify(move |bar| {
            if !bar.is_search_mode() {
                if let Some(w) = weak.upgrade() {
                    w.search_entry.set_text("");
                    w.focus_view();
                }
            }
        });

        let weak = Rc::downgrade(self);
        self.view_button.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                let list = w.cfg.borrow().list_view;
                w.set_view(!list);
            }
        });

        let weak = Rc::downgrade(self);
        self.sidebar.connect_open(move |f| {
            if let Some(w) = weak.upgrade() {
                w.navigate(f);
            }
        });

        // typing a path
        let weak = Rc::downgrade(self);
        self.location.connect_activate(move |e| {
            let Some(w) = weak.upgrade() else { return };
            let text = e.text().trim().to_string();
            let text = match text.strip_prefix('~') {
                Some(rest) => format!("{}{rest}", glib::home_dir().display()),
                None => text,
            };
            let f = gio::File::for_parse_name(&text);
            match f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
                gio::FileType::Directory => w.navigate(f),
                gio::FileType::Unknown => {
                    w.toast(&format!("Can't find “{text}”"));
                }
                _ => {
                    if let Some(p) = f.parent() {
                        w.navigate(p);
                        w.select_when_shown(vec![f]);
                    }
                }
            }
            w.hide_location();
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(w) = weak.upgrade() {
                    w.hide_location();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.location.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            if let Some(w) = weak.upgrade() {
                w.title_stack.set_visible_child_name("path");
            }
        });
        self.location.add_controller(focus);

        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |window| {
            if let Some(w) = weak.upgrade() {
                let mut cfg = w.cfg.borrow_mut();
                cfg.maximized = window.is_maximized();
                if !cfg.maximized {
                    let (width, height) = window.default_size();
                    cfg.width = width;
                    cfg.height = height;
                }
                cfg.show_hidden = w.state.show_hidden.get();
                cfg.sort = w.state.sort.get();
                cfg.descending = w.state.descending.get();
                cfg.save();
                w.popover.unparent();
                WINDOWS.with(|all| all.borrow_mut().retain(|x| !Rc::ptr_eq(x, &w)));
            }
            glib::Propagation::Proceed
        });
    }

    fn hide_location(&self) {
        self.title_stack.set_visible_child_name("path");
        self.focus_view();
    }

    fn activate_item(self: &Rc<Self>, pos: u32) {
        let Some(info) = self.info_at(pos) else { return };
        let file = util::file_of(&info);
        if util::is_dir(&info) {
            self.navigate(file);
        } else if self.in_trash() {
            props::show(&self.window, vec![file]);
        } else {
            self.launch(&file, false);
        }
    }

    fn launch(self: &Rc<Self>, file: &gio::File, ask: bool) {
        let launcher = gtk::FileLauncher::new(Some(file));
        launcher.set_always_ask(ask);
        let weak = Rc::downgrade(self);
        launcher.launch(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if let (Err(e), Some(w)) = (res, weak.upgrade()) {
                if !e.matches(gtk::DialogError::Dismissed) && !e.matches(gtk::DialogError::Cancelled) {
                    w.toast(e.message());
                }
            }
        });
    }

    // --------------------------------------------------------------- actions

    fn add_actions(self: &Rc<Self>) {
        let simple = |name: &str, f: fn(&Rc<Win>)| {
            let a = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            a.connect_activate(move |_, _| {
                if let Some(w) = weak.upgrade() {
                    f(&w);
                }
            });
            self.window.add_action(&a);
        };

        simple("back", |w| w.go_back());
        simple("forward", |w| w.go_forward());
        simple("up", |w| w.go_up());
        simple("home", |w| w.navigate(gio::File::for_path(glib::home_dir())));
        simple("reload", |w| {
            w.dir_list.set_file(None::<&gio::File>);
            w.dir_list.set_file(Some(&w.current()));
        });
        simple("location", |w| {
            w.location.set_text(&w.current().parse_name());
            w.title_stack.set_visible_child_name("entry");
            w.location.grab_focus();
        });
        simple("search", |w| {
            let on = !w.search_bar.is_search_mode();
            w.search_bar.set_search_mode(on);
            if on {
                w.search_entry.grab_focus();
            }
        });
        simple("new-window", |w| {
            if let Some(app) = w.window.application().and_downcast::<adw::Application>() {
                Win::open(&app, &w.current(), None);
            }
        });
        simple("select-all", |w| {
            w.selection.select_all();
        });
        simple("open", |w| {
            let infos = w.selected_infos();
            if let [one] = infos.as_slice() {
                if util::is_dir(one) {
                    return w.navigate(util::file_of(one));
                }
            }
            for i in &infos {
                if !util::is_dir(i) {
                    w.launch(&util::file_of(i), false);
                }
            }
        });
        simple("open-with", |w| {
            if let [one] = w.selected_files().as_slice() {
                w.launch(one, true);
            }
        });
        simple("open-new-window", |w| {
            let Some(app) = w.window.application().and_downcast::<adw::Application>() else { return };
            for i in w.selected_infos().iter().filter(|i| util::is_dir(i)) {
                Win::open(&app, &util::file_of(i), None);
            }
        });
        simple("open-terminal", |w| {
            let Some(path) = w.current().path() else { return };
            let dir = format!("--working-directory={}", path.display());
            let argv: [&std::ffi::OsStr; 4] = ["exo-open".as_ref(), "--launch".as_ref(), "TerminalEmulator".as_ref(), dir.as_ref()];
            if gio::Subprocess::newv(&argv, gio::SubprocessFlags::NONE).is_err() {
                w.toast("Couldn't open a terminal");
            }
        });
        simple("copy", |w| w.copy_or_cut(false));
        simple("cut", |w| w.copy_or_cut(true));
        simple("paste", |w| {
            let w = w.clone();
            glib::spawn_future_local(async move { w.paste().await });
        });
        simple("copy-path", |w| {
            let paths: Vec<String> = w
                .selected_files()
                .iter()
                .map(|f| f.path().map(|p| p.display().to_string()).unwrap_or_else(|| f.uri().to_string()))
                .collect();
            if !paths.is_empty() {
                clipboard::put_text(&w.window.clipboard(), &paths.join("\n"));
                w.toast(if paths.len() == 1 { "Path copied" } else { "Paths copied" });
            }
        });
        simple("rename", |w| w.rename());
        simple("new-folder", |w| {
            let w = w.clone();
            glib::spawn_future_local(async move {
                let Some(name) = rename::ask_one(&w.window, "New Folder", "New Folder", "Create").await else { return };
                let dir = w.current();
                w.run_op(String::new(), move || ops::new_folder(&dir, &name), |w, f| w.select_when_shown(vec![f]));
            });
        });
        simple("compress", |w| {
            let files = w.selected_files();
            if files.is_empty() {
                return;
            }
            let dir = w.current();
            w.run_op("Compressing…".into(), move || ops::compress(&dir, &files), |w, f| {
                w.select_when_shown(vec![f]);
                w.toast("Compressed");
            });
        });
        simple("extract", |w| {
            let files: Vec<gio::File> = w.selected_files().into_iter().filter(|f| util::is_archive(&f.basename().unwrap().to_string_lossy())).collect();
            w.run_op(
                "Extracting…".into(),
                move || files.iter().map(ops::extract).collect::<Result<Vec<_>, _>>(),
                |w, out| {
                    w.select_when_shown(out);
                    w.toast("Extracted");
                },
            );
        });
        simple("trash", |w| w.trash_selected());
        simple("restore", |w| {
            let files = w.selected_files();
            w.run_op("Putting back…".into(), move || ops::restore(&files), |w, n| {
                w.toast(&format!("Put back {n} item{}", if n == 1 { "" } else { "s" }));
            });
        });
        simple("empty-trash", |w| {
            let w = w.clone();
            glib::spawn_future_local(async move {
                if confirm(&w.window, "Empty the Trash?", "Everything in the Trash will be deleted for good.", "Empty Trash").await {
                    w.run_op("Emptying the Trash…".into(), ops::empty_trash, |w, _| {
                        w.toast("Trash emptied");
                    });
                }
            });
        });
        simple("properties", |w| {
            let files = w.selected_files();
            props::show(&w.window, if files.is_empty() { vec![w.current()] } else { files });
        });
        simple("folder-properties", |w| props::show(&w.window, vec![w.current()]));
        simple("bookmark", |w| {
            let added = w.sidebar.add_bookmark(&w.current());
            w.toast(if added { "Added to Bookmarks" } else { "Already in Bookmarks" });
        });
        simple("about", |w| {
            let about = adw::AboutDialog::builder()
                .application_name("Jungey Files")
                .application_icon("jungey-files")
                .version(env!("CARGO_PKG_VERSION"))
                .developer_name("Jungey OS")
                .comments("A light, fast file manager for Jungey OS.")
                .build();
            about.present(Some(&w.window));
        });

        // stateful: view, hidden files, sorting
        let cfg = self.cfg.borrow().clone();
        let view = gio::SimpleAction::new_stateful(
            "view",
            Some(glib::VariantTy::STRING),
            &(if cfg.list_view { "list" } else { "grid" }).to_variant(),
        );
        let weak = Rc::downgrade(self);
        view.connect_activate(move |a, v| {
            let (Some(w), Some(v)) = (weak.upgrade(), v) else { return };
            a.set_state(v);
            w.set_view(v.str() == Some("list"));
        });
        view.connect_change_state(|a, v| {
            if let Some(v) = v {
                a.set_state(v);
            }
        });
        self.window.add_action(&view);

        let hidden = gio::SimpleAction::new_stateful("show-hidden", None, &cfg.show_hidden.to_variant());
        let weak = Rc::downgrade(self);
        hidden.connect_change_state(move |a, v| {
            let (Some(w), Some(v)) = (weak.upgrade(), v) else { return };
            a.set_state(v);
            w.state.show_hidden.set(v.get::<bool>().unwrap_or(false));
            w.filter.changed(gtk::FilterChange::Different);
        });
        self.window.add_action(&hidden);

        let sort = gio::SimpleAction::new_stateful("sort", Some(glib::VariantTy::STRING), &cfg.sort.id().to_variant());
        let weak = Rc::downgrade(self);
        sort.connect_change_state(move |a, v| {
            let (Some(w), Some(v)) = (weak.upgrade(), v) else { return };
            a.set_state(v);
            w.state.sort.set(SortKey::from_id(v.str().unwrap_or("name")));
            w.sorter.changed(gtk::SorterChange::Different);
        });
        self.window.add_action(&sort);

        let desc = gio::SimpleAction::new_stateful("descending", None, &cfg.descending.to_variant());
        let weak = Rc::downgrade(self);
        desc.connect_change_state(move |a, v| {
            let (Some(w), Some(v)) = (weak.upgrade(), v) else { return };
            a.set_state(v);
            w.state.descending.set(v.get::<bool>().unwrap_or(false));
            w.sorter.changed(gtk::SorterChange::Different);
        });
        self.window.add_action(&desc);
    }

    /// File shortcuts that must not fire while typing in a text box: they run
    /// only if the focused widget (e.g. the search entry) didn't use the key.
    fn add_shortcuts(&self) {
        let ctl = gtk::ShortcutController::new();
        ctl.set_propagation_phase(gtk::PropagationPhase::Bubble);
        for (keys, action) in [
            ("<Control>c", "win.copy"),
            ("<Control>x", "win.cut"),
            ("<Control>v", "win.paste"),
            ("<Control><Shift>c", "win.copy-path"),
            ("Delete|KP_Delete", "win.trash"),
            ("F2", "win.rename"),
            ("BackSpace", "win.up"),
        ] {
            ctl.add_shortcut(gtk::Shortcut::new(
                gtk::ShortcutTrigger::parse_string(keys),
                Some(gtk::NamedAction::new(action)),
            ));
        }
        self.window.add_controller(ctl);
    }

    // ------------------------------------------------------------ operations

    fn copy_or_cut(&self, cut: bool) {
        let files = self.selected_files();
        if files.is_empty() || self.in_trash() {
            return;
        }
        clipboard::put(&self.window.clipboard(), &files, cut);
        let n = files.len();
        let what = if n == 1 { "1 item".into() } else { format!("{n} items") };
        self.toast(&format!("{} {what} — paste with Ctrl+V", if cut { "Cut" } else { "Copied" }));
    }

    async fn paste(self: Rc<Self>) {
        if self.in_trash() {
            return;
        }
        let cb = self.window.clipboard();
        let dest = self.current();
        if let Some((files, cut)) = clipboard::files(&cb).await {
            if cut {
                clipboard::clear(&cb);
            }
            self.transfer(files, dest, if cut { Transfer::Move } else { Transfer::Copy });
        } else if let Some(tex) = clipboard::image(&cb).await {
            let target = util::unique_child(&dest, "Pasted image.png");
            match target.path().map(|p| tex.save_to_png(p)) {
                Some(Ok(())) => {
                    self.select_when_shown(vec![target]);
                    self.toast("Pasted image");
                }
                _ => {
                    self.toast("Couldn't save the image here");
                }
            }
        } else {
            self.toast("Nothing to paste");
        }
    }

    fn rename(self: &Rc<Self>) {
        let files = self.selected_files();
        if files.is_empty() || self.in_trash() {
            return;
        }
        if files.len() > 1 {
            let weak = Rc::downgrade(self);
            rename::bulk(&self.window, files, move |res| {
                let Some(w) = weak.upgrade() else { return };
                match res {
                    Ok(new) => {
                        let n = new.len();
                        w.select_when_shown(new);
                        w.toast(&format!("Renamed {n} item{}", if n == 1 { "" } else { "s" }));
                    }
                    Err(e) => {
                        w.toast(e.message());
                    }
                }
            });
            return;
        }
        let file = files[0].clone();
        let w = self.clone();
        glib::spawn_future_local(async move {
            let current = file.basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_default();
            let Some(name) = rename::ask_one(&w.window, "Rename", &current, "Rename").await else { return };
            w.run_op(
                String::new(),
                move || file.set_display_name(&name, gio::Cancellable::NONE),
                |w, f| w.select_when_shown(vec![f]),
            );
        });
    }

    fn trash_selected(self: &Rc<Self>) {
        let files = self.selected_files();
        if files.is_empty() {
            return;
        }
        let n = files.len();
        let items = if n == 1 { "1 item".to_string() } else { format!("{n} items") };
        if self.in_trash() {
            let w = self.clone();
            glib::spawn_future_local(async move {
                let body = format!("{items} will be deleted for good. This can't be undone.");
                if confirm(&w.window, "Delete permanently?", &body, "Delete").await {
                    w.run_op("Deleting…".into(), move || files.iter().try_for_each(ops::delete_recursive), |w, _| {
                        w.toast("Deleted");
                    });
                }
            });
            return;
        }
        let originals: Vec<PathBuf> = files.iter().filter_map(|f| f.path()).collect();
        self.run_op("Moving to Trash…".into(), move || ops::trash(&files), move |w, _| {
            let toast = adw::Toast::new(&format!("Moved {items} to Trash"));
            toast.set_button_label(Some("Undo"));
            toast.set_timeout(6);
            let weak = Rc::downgrade(w);
            toast.connect_button_clicked(move |_| {
                let Some(w) = weak.upgrade() else { return };
                let originals = originals.clone();
                let back = originals.iter().map(gio::File::for_path).collect::<Vec<_>>();
                w.run_op("Putting back…".into(), move || ops::untrash(&originals), move |w, _| {
                    w.select_when_shown(back);
                });
            });
            w.toasts.add_toast(toast);
        });
    }
}

fn set_dimmed(cell: &gtk::Box, info: &gio::FileInfo) {
    if info.is_hidden() || info.is_backup() {
        cell.add_css_class("dimmed");
    } else {
        cell.remove_css_class("dimmed");
    }
}

fn same_drive(a: &gio::File, b: &gio::File) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (a.path(), b.path()) {
        (Some(a), Some(b)) => match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev(),
            _ => false,
        },
        _ => false,
    }
}

fn type_description(content_type: &str) -> String {
    thread_local! {
        static CACHE: RefCell<std::collections::HashMap<String, String>> = RefCell::default();
    }
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(content_type.to_string())
            .or_insert_with(|| gio::content_type_get_description(content_type).to_string())
            .clone()
    })
}

async fn confirm(parent: &adw::ApplicationWindow, heading: &str, body: &str, action: &str) -> bool {
    let dialog = adw::AlertDialog::new(Some(heading), Some(body));
    dialog.add_responses(&[("cancel", "Cancel"), ("ok", action)]);
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.choose_future(Some(parent)).await == "ok"
}

fn section(menu: &gio::Menu) -> gio::Menu {
    let s = gio::Menu::new();
    menu.append_section(None, &s);
    s
}

fn item(menu: &gio::Menu, label: &str, action: &str, accel: Option<&str>) {
    let i = gio::MenuItem::new(Some(label), Some(action));
    if let Some(a) = accel {
        i.set_attribute_value("accel", Some(&a.to_variant()));
    }
    menu.append_item(&i);
}

fn sort_menu() -> gio::Menu {
    let m = gio::Menu::new();
    let s = section(&m);
    for (label, id) in [("Name", "name"), ("Date Modified", "modified"), ("Size", "size"), ("Type", "type")] {
        item(&s, label, &format!("win.sort::{id}"), None);
    }
    let s = section(&m);
    item(&s, "Reverse Order", "win.descending", None);
    m
}

fn main_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let s = section(&menu);
    item(&s, "New Window", "win.new-window", Some("<Control>n"));
    item(&s, "New Folder…", "win.new-folder", Some("<Control><Shift>n"));
    item(&s, "Bookmark This Folder", "win.bookmark", Some("<Control>d"));
    let s = section(&menu);
    item(&s, "Show Hidden Files", "win.show-hidden", Some("<Control>h"));
    s.append_submenu(Some("Sort By"), &sort_menu());
    let s = section(&menu);
    item(&s, "Open Terminal Here", "win.open-terminal", Some("F4"));
    item(&s, "Go to Location…", "win.location", Some("<Control>l"));
    let s = section(&menu);
    item(&s, "About Jungey Files", "win.about", None);
    menu
}
