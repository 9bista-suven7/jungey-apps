//! Places down the left: your folders, bookmarks, and drives (kept up to date
//! as drives are plugged in or removed).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

pub struct Sidebar {
    pub widget: gtk::ScrolledWindow,
    list: gtk::ListBox,
    rows: RefCell<Vec<(gtk::ListBoxRow, gio::File)>>,
    monitor: gio::VolumeMonitor,
    on_open: RefCell<Option<Rc<dyn Fn(gio::File)>>>,
}

fn bookmarks_file() -> std::path::PathBuf {
    glib::user_config_dir().join("gtk-3.0/bookmarks")
}

impl Sidebar {
    pub fn new() -> Rc<Sidebar> {
        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk::SelectionMode::Single);
        let widget = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .vexpand(true)
            .build();
        let sidebar = Rc::new(Sidebar {
            widget,
            list,
            rows: RefCell::default(),
            monitor: gio::VolumeMonitor::get(),
            on_open: RefCell::default(),
        });

        let weak = Rc::downgrade(&sidebar);
        sidebar.list.connect_row_activated(move |_, row| {
            let Some(s) = weak.upgrade() else { return };
            let file = s.rows.borrow().iter().find(|(r, _)| r == row).map(|(_, f)| f.clone());
            let cb = s.on_open.borrow().clone();
            if let (Some(file), Some(cb)) = (file, cb) {
                cb(file);
            }
        });
        for signal in ["mount-added", "mount-removed", "mount-changed"] {
            let weak = Rc::downgrade(&sidebar);
            sidebar.monitor.connect_local(signal, false, move |_| {
                if let Some(s) = weak.upgrade() {
                    s.rebuild();
                }
                None
            });
        }
        sidebar.rebuild();
        sidebar
    }

    pub fn connect_open(&self, f: impl Fn(gio::File) + 'static) {
        self.on_open.replace(Some(Rc::new(f)));
    }

    /// Highlight the row for `file`, if there is one.
    pub fn select(&self, file: &gio::File) {
        let rows = self.rows.borrow();
        match rows.iter().find(|(_, f)| f.equal(file)) {
            Some((row, _)) => self.list.select_row(Some(row)),
            None => self.list.unselect_all(),
        }
    }

    pub fn add_bookmark(&self, file: &gio::File) -> bool {
        let path = bookmarks_file();
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        let uri = file.uri();
        if current.lines().any(|l| l.split(' ').next() == Some(uri.as_str())) {
            return false;
        }
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let sep = if current.is_empty() || current.ends_with('\n') { "" } else { "\n" };
        if std::fs::write(&path, format!("{current}{sep}{uri}\n")).is_err() {
            return false;
        }
        self.rebuild();
        true
    }

    pub fn rebuild(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.rows.borrow_mut().clear();

        self.heading("Places");
        let home = glib::home_dir();
        self.add("user-home-symbolic", "Home", gio::File::for_path(&home));
        use glib::UserDirectory as D;
        for (dir, icon, fallback) in [
            (D::Desktop, "user-desktop-symbolic", "Desktop"),
            (D::Documents, "folder-documents-symbolic", "Documents"),
            (D::Downloads, "folder-download-symbolic", "Downloads"),
            (D::Music, "folder-music-symbolic", "Music"),
            (D::Pictures, "folder-pictures-symbolic", "Pictures"),
            (D::Videos, "folder-videos-symbolic", "Videos"),
        ] {
            if let Some(p) = glib::user_special_dir(dir).filter(|p| *p != home && p.is_dir()) {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(fallback.into());
                self.add(icon, &name, gio::File::for_path(p));
            }
        }
        self.add("user-trash-symbolic", "Trash", gio::File::for_uri("trash:///"));

        let bookmarks: Vec<(gio::File, String)> = std::fs::read_to_string(bookmarks_file())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (uri, label) = l.split_once(' ').unwrap_or((l, ""));
                if uri.is_empty() {
                    return None;
                }
                let f = gio::File::for_uri(uri);
                let name = if label.is_empty() {
                    f.basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_else(|| uri.to_string())
                } else {
                    label.to_string()
                };
                Some((f, name))
            })
            .collect();
        if !bookmarks.is_empty() {
            self.heading("Bookmarks");
            for (f, name) in bookmarks {
                self.add("folder-symbolic", &name, f);
            }
        }

        self.heading("Devices");
        self.add("drive-harddisk-symbolic", "Computer", gio::File::for_path("/"));
        for mount in self.monitor.mounts() {
            if mount.is_shadowed() {
                continue;
            }
            let row = self.add_gicon(&mount.symbolic_icon(), &mount.name(), mount.root());
            if mount.can_unmount() {
                let eject = gtk::Button::from_icon_name("media-eject-symbolic");
                eject.add_css_class("flat");
                eject.set_tooltip_text(Some("Unmount"));
                eject.connect_clicked(glib::clone!(
                    #[weak]
                    mount,
                    move |_| {
                        mount.unmount_with_operation(
                            gio::MountUnmountFlags::NONE,
                            gio::MountOperation::NONE,
                            gio::Cancellable::NONE,
                            |_| {},
                        );
                    }
                ));
                if let Some(b) = row.child().and_downcast::<gtk::Box>() {
                    b.append(&eject);
                }
            }
        }
    }

    fn heading(&self, text: &str) {
        let label = gtk::Label::new(Some(&text.to_uppercase()));
        label.set_xalign(0.0);
        let row = gtk::ListBoxRow::builder().child(&label).selectable(false).activatable(false).build();
        row.add_css_class("jungey-heading");
        self.list.append(&row);
    }

    fn add(&self, icon: &str, name: &str, file: gio::File) -> gtk::ListBoxRow {
        self.add_gicon(&gio::ThemedIcon::new(icon).upcast(), name, file)
    }

    fn add_gicon(&self, icon: &gio::Icon, name: &str, file: gio::File) -> gtk::ListBoxRow {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        b.append(&gtk::Image::from_gicon(icon));
        let label = gtk::Label::new(Some(name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        b.append(&label);
        let row = gtk::ListBoxRow::builder().child(&b).tooltip_text(file.parse_name().as_str()).build();
        self.list.append(&row);
        self.rows.borrow_mut().push((row.clone(), file));
        row
    }
}
