//! Properties: what it is, where it is, how big, when it changed.

use adw::prelude::*;
use gtk::{gio, glib};

use crate::{ops, util};

pub fn show(parent: &impl IsA<gtk::Widget>, files: Vec<gio::File>) {
    let Some(first) = files.first().cloned() else { return };
    let none = gio::Cancellable::NONE;
    let attrs = "standard::*,time::modified,unix::mode,owner::user,trash::orig-path";
    let info = first.query_info(attrs, gio::FileQueryInfoFlags::NONE, none).ok();

    let icon = gtk::Image::new();
    icon.set_pixel_size(96);
    let title = gtk::Label::new(None);
    title.add_css_class("title-2");
    title.set_wrap(true);
    title.set_justify(gtk::Justification::Center);
    title.set_selectable(true);

    let grid = gtk::Grid::builder().column_spacing(18).row_spacing(10).margin_top(18).build();
    let mut row = 0;
    let mut add = |key: &str, value: &str| -> gtk::Label {
        let k = gtk::Label::new(Some(key));
        k.set_xalign(1.0);
        k.add_css_class("dim-label");
        let v = gtk::Label::new(Some(value));
        v.set_xalign(0.0);
        v.set_wrap(true);
        v.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        v.set_selectable(true);
        v.set_hexpand(true);
        grid.attach(&k, 0, row, 1, 1);
        grid.attach(&v, 1, row, 1, 1);
        row += 1;
        v
    };

    let size_label;
    if files.len() == 1 {
        let info = info.clone();
        let name = info.as_ref().map(|i| i.display_name().to_string()).unwrap_or_default();
        title.set_text(&name);
        let kind = info.as_ref().map(|i| gio::content_type_get_description(&util::content_type(i)).to_string()).unwrap_or_default();
        let gicon = info.as_ref().and_then(|i| i.icon()).unwrap_or_else(|| gio::ThemedIcon::new("text-x-generic").upcast());
        icon.set_from_gicon(&gicon);
        add("Type", &kind);
        let place = first.parent().map(|p| p.parse_name().to_string()).unwrap_or_default();
        add("Location", &place);
        if let Some(orig) = info.as_ref().and_then(|i| i.attribute_byte_string("trash::orig-path")) {
            add("Deleted from", orig.as_str());
        }
        size_label = add("Size", "…");
        if let Some(i) = info.as_ref() {
            let t = i.attribute_uint64("time::modified");
            if t > 0 {
                add("Modified", &util::friendly_time(t));
            }
            if i.has_attribute("unix::mode") {
                add("Permissions", &perm_string(i.attribute_uint32("unix::mode")));
            }
            if let Some(owner) = i.attribute_string("owner::user") {
                add("Owner", &owner);
            }
        }
    } else {
        title.set_text(&format!("{} items", files.len()));
        icon.set_icon_name(Some("edit-copy"));
        let place = first.parent().map(|p| p.parse_name().to_string()).unwrap_or_default();
        add("Location", &place);
        size_label = add("Size", "…");
    }

    // sizes of folders take a while on a hard drive: work them out in the background
    let is_single_file = files.len() == 1 && info.as_ref().map(|i| !util::is_dir(i)).unwrap_or(false);
    if is_single_file {
        let bytes = info.as_ref().map(|i| i.size() as u64).unwrap_or(0);
        size_label.set_text(&format!("{} ({} bytes)", util::human_size(bytes), group(bytes)));
    } else if files.iter().all(|f| f.is_native()) {
        let label = size_label.clone();
        glib::spawn_future_local(async move {
            if let Ok((bytes, count)) = gio::spawn_blocking(move || ops::measure(&files)).await {
                let items = if count == 1 { "1 file".into() } else { format!("{} files", group(count)) };
                label.set_text(&format!("{} ({items})", util::human_size(bytes)));
            }
        });
    } else {
        size_label.set_text("—");
    }

    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_top(12);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);
    body.append(&icon);
    body.append(&title);
    body.append(&grid);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&body));
    let dialog = adw::Dialog::builder().title("Properties").content_width(440).child(&view).build();
    dialog.present(Some(parent));
}

fn perm_string(mode: u32) -> String {
    let bits = ['r', 'w', 'x'];
    let mut s = String::new();
    for shift in [6, 3, 0] {
        for (i, c) in bits.iter().enumerate() {
            s.push(if mode >> (shift + 2 - i as u32) & 1 == 1 { *c } else { '-' });
        }
        if shift > 0 {
            s.push(' ');
        }
    }
    let owner = match (mode >> 6) & 0o7 {
        7 | 6 => "you can read and change it",
        5 | 4 => "you can read it",
        _ => "you can't open it",
    };
    format!("{s}  ({owner})")
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
