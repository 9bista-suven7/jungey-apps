use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

use gtk::prelude::*;
use gtk::{gio, glib};

/// The `GFile` a `GtkDirectoryList` item points at.
pub fn file_of(info: &gio::FileInfo) -> gio::File {
    info.attribute_object("standard::file")
        .and_downcast::<gio::File>()
        .expect("directory list items carry standard::file")
}

pub fn is_dir(info: &gio::FileInfo) -> bool {
    info.file_type() == gio::FileType::Directory
}

/// Content type guessed from the name only (no disk reads), with fallbacks for
/// locations like trash:// that only report the sniffed type.
pub fn content_type(info: &gio::FileInfo) -> String {
    if is_dir(info) {
        return "inode/directory".into();
    }
    info.attribute_string("standard::fast-content-type")
        .or_else(|| info.content_type())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".into())
}

pub fn is_image(info: &gio::FileInfo) -> bool {
    let t = content_type(info);
    t.starts_with("image/") && !t.contains("svg") && !t.contains("x-xcf")
}

pub fn is_archive(name: &str) -> bool {
    archive_kind(name).is_some()
}

pub enum ArchiveKind {
    Zip,
    Tar,
}

pub fn archive_kind(name: &str) -> Option<ArchiveKind> {
    let n = name.to_lowercase();
    if [".zip", ".jar", ".apk", ".epub", ".cbz"].iter().any(|e| n.ends_with(e)) {
        Some(ArchiveKind::Zip)
    } else if [".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tar.zst"]
        .iter()
        .any(|e| n.ends_with(e))
    {
        Some(ArchiveKind::Tar)
    } else {
        None
    }
}

/// "report.final.pdf" -> ("report.final", ".pdf"); "data.tar.gz" -> ("data", ".tar.gz");
/// ".bashrc" -> (".bashrc", "")
pub fn split_ext(name: &str) -> (&str, &str) {
    let lower = name.to_lowercase();
    for double in [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst"] {
        if lower.ends_with(double) && name.len() > double.len() {
            let i = name.len() - double.len();
            return (&name[..i], &name[i..]);
        }
    }
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// A child of `dir` named like `name` that doesn't exist yet: "x.txt", "x (2).txt", ...
pub fn unique_child(dir: &gio::File, name: &str) -> gio::File {
    let first = dir.child(name);
    if !first.query_exists(gio::Cancellable::NONE) {
        return first;
    }
    let (stem, ext) = split_ext(name);
    (2..)
        .map(|n| dir.child(format!("{stem} ({n}){ext}")))
        .find(|f| !f.query_exists(gio::Cancellable::NONE))
        .unwrap()
}

pub fn human_size(bytes: u64) -> String {
    glib::format_size(bytes).to_string()
}

/// "Today 14:02", "Yesterday 09:15", "3 Oct", "3 Oct 2024"
pub fn friendly_time(unix: u64) -> String {
    let Ok(t) = glib::DateTime::from_unix_local(unix as i64) else {
        return String::new();
    };
    let Ok(now) = glib::DateTime::now_local() else {
        return String::new();
    };
    let days = now.ymd();
    let same = |d: &glib::DateTime| d.ymd() == t.ymd();
    let fmt = if same(&now) {
        "Today %H:%M"
    } else if now.add_days(-1).map(|d| same(&d)).unwrap_or(false) {
        "Yesterday %H:%M"
    } else if t.year() == days.0 {
        "%e %b, %H:%M"
    } else {
        "%e %b %Y"
    };
    t.format(fmt).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Icon for an item, using the folder-specific icons for Documents, Downloads, ...
pub fn icon_for(info: &gio::FileInfo) -> gio::Icon {
    if is_dir(info) {
        if let Some(path) = file_of(info).path() {
            if let Some(name) = special_folder_icon(&path) {
                return gio::ThemedIcon::new(name).upcast();
            }
        }
    }
    if info.attribute_string("standard::fast-content-type").is_none() {
        if let Some(icon) = info.icon() {
            return icon;
        }
    }
    gio::content_type_get_icon(&content_type(info))
}

pub fn special_folder_icon(path: &std::path::Path) -> Option<&'static str> {
    if path == glib::home_dir() {
        return Some("user-home");
    }
    use glib::UserDirectory as D;
    [
        (D::Desktop, "user-desktop"),
        (D::Documents, "folder-documents"),
        (D::Downloads, "folder-download"),
        (D::Music, "folder-music"),
        (D::Pictures, "folder-pictures"),
        (D::Videos, "folder-videos"),
        (D::Templates, "folder-templates"),
        (D::PublicShare, "folder-publicshare"),
    ]
    .into_iter()
    .find(|(d, _)| glib::user_special_dir(*d).as_deref() == Some(path))
    .map(|(_, icon)| icon)
}

/// Name order people expect: case-insensitive, and "img2" before "img10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (na, nb) = (digits(&mut a), digits(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)).then_with(|| na.len().cmp(&nb.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn digits(it: &mut Peekable<Chars>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["img10.png", "IMG2.png", "img1.png", "Apple", "banana"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Apple", "banana", "img1.png", "IMG2.png", "img10.png"]);
    }

    #[test]
    fn extensions() {
        assert_eq!(split_ext("data.tar.gz"), ("data", ".tar.gz"));
        assert_eq!(split_ext(".bashrc"), (".bashrc", ""));
        assert_eq!(split_ext("a.b.pdf"), ("a.b", ".pdf"));
        assert_eq!(split_ext("README"), ("README", ""));
    }
}
