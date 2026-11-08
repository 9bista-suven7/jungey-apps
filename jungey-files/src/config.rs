//! Remembers view settings between runs in ~/.config/jungey-files/settings.ini.

use gtk::glib;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Type,
}

impl SortKey {
    pub fn id(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Modified => "modified",
            SortKey::Size => "size",
            SortKey::Type => "type",
        }
    }

    pub fn from_id(id: &str) -> SortKey {
        match id {
            "modified" => SortKey::Modified,
            "size" => SortKey::Size,
            "type" => SortKey::Type,
            _ => SortKey::Name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub list_view: bool,
    pub show_hidden: bool,
    pub sort: SortKey,
    pub descending: bool,
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            list_view: false,
            show_hidden: false,
            sort: SortKey::Name,
            descending: false,
            width: 1100,
            height: 700,
            maximized: false,
        }
    }
}

fn path() -> PathBuf {
    glib::user_config_dir().join("jungey-files/settings.ini")
}

impl Config {
    pub fn load() -> Config {
        let d = Config::default();
        let kf = glib::KeyFile::new();
        if kf.load_from_file(path(), glib::KeyFileFlags::NONE).is_err() {
            return d;
        }
        let g = "view";
        Config {
            list_view: kf.boolean(g, "list").unwrap_or(d.list_view),
            show_hidden: kf.boolean(g, "hidden").unwrap_or(d.show_hidden),
            sort: kf.string(g, "sort").map(|s| SortKey::from_id(&s)).unwrap_or(d.sort),
            descending: kf.boolean(g, "descending").unwrap_or(d.descending),
            width: kf.integer("window", "width").unwrap_or(d.width).max(480),
            height: kf.integer("window", "height").unwrap_or(d.height).max(360),
            maximized: kf.boolean("window", "maximized").unwrap_or(d.maximized),
        }
    }

    pub fn save(&self) {
        let kf = glib::KeyFile::new();
        kf.set_boolean("view", "list", self.list_view);
        kf.set_boolean("view", "hidden", self.show_hidden);
        kf.set_string("view", "sort", self.sort.id());
        kf.set_boolean("view", "descending", self.descending);
        kf.set_integer("window", "width", self.width);
        kf.set_integer("window", "height", self.height);
        kf.set_boolean("window", "maximized", self.maximized);
        let p = path();
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = kf.save_to_file(p);
    }
}
