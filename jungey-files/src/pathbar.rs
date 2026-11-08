//! Breadcrumbs: Home › Documents › Work. Click any part to go there.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

pub struct PathBar {
    pub widget: gtk::ScrolledWindow,
    row: gtk::Box,
}

impl PathBar {
    pub fn new() -> PathBar {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.add_css_class("jungey-pathbar");
        let widget = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .child(&row)
            .build();
        PathBar { widget, row }
    }

    pub fn set(&self, file: &gio::File, on_click: Rc<dyn Fn(gio::File)>) {
        while let Some(child) = self.row.first_child() {
            self.row.remove(&child);
        }
        let home = glib::home_dir();
        let mut chain = Vec::new();
        let mut cur = Some(file.clone());
        while let Some(f) = cur {
            let at_home = f.path().as_deref() == Some(home.as_path());
            cur = if at_home { None } else { f.parent() };
            chain.push(f);
        }
        chain.reverse();

        let last = chain.len() - 1;
        for (i, f) in chain.into_iter().enumerate() {
            if i > 0 {
                let sep = gtk::Label::new(Some("›"));
                sep.add_css_class("sep");
                self.row.append(&sep);
            }
            let button = gtk::Button::new();
            button.add_css_class("flat");
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            if let Some(icon) = crumb_icon(&f, &home) {
                content.append(&gtk::Image::from_icon_name(icon));
            }
            content.append(&gtk::Label::new(Some(&crumb_name(&f, &home))));
            button.set_child(Some(&content));
            if i == last {
                button.add_css_class("current");
            }
            let cb = on_click.clone();
            button.connect_clicked(move |_| cb(f.clone()));
            self.row.append(&button);
        }

        // keep the current folder in view when the path is long
        let adj = self.widget.hadjustment();
        glib::idle_add_local_once(move || adj.set_value(adj.upper()));
    }
}

fn crumb_name(f: &gio::File, home: &std::path::Path) -> String {
    match f.path() {
        Some(p) if p == home => "Home".into(),
        Some(p) if p.as_os_str() == "/" => "Computer".into(),
        _ if f.uri_scheme().as_deref() == Some("trash") && f.parent().is_none() => "Trash".into(),
        _ => f
            .basename()
            .map(|b| b.to_string_lossy().to_string())
            .filter(|b| b != "/")
            .unwrap_or_else(|| f.uri().to_string()),
    }
}

fn crumb_icon(f: &gio::File, home: &std::path::Path) -> Option<&'static str> {
    match f.path() {
        Some(p) if p == home => Some("user-home-symbolic"),
        Some(p) if p.as_os_str() == "/" => Some("drive-harddisk-symbolic"),
        _ if f.uri_scheme().as_deref() == Some("trash") && f.parent().is_none() => Some("user-trash-symbolic"),
        _ => None,
    }
}
