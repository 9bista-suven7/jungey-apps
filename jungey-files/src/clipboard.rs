//! Copy/cut/paste of files through the system clipboard, in the formats other
//! file managers use, so copying between Jungey Files and other apps works.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

const GNOME: &str = "x-special/gnome-copied-files";
const URIS: &str = "text/uri-list";

pub fn put(clipboard: &gdk::Clipboard, files: &[gio::File], cut: bool) {
    let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
    let gnome = format!("{}\n{}", if cut { "cut" } else { "copy" }, uris.join("\n"));
    let list = uris.iter().map(|u| format!("{u}\r\n")).collect::<String>();
    let text = files
        .iter()
        .map(|f| f.path().map(|p| p.display().to_string()).unwrap_or_else(|| f.uri().to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let provider = gdk::ContentProvider::new_union(&[
        gdk::ContentProvider::for_bytes(GNOME, &glib::Bytes::from_owned(gnome.into_bytes())),
        gdk::ContentProvider::for_bytes(URIS, &glib::Bytes::from_owned(list.into_bytes())),
        gdk::ContentProvider::for_value(&text.to_value()),
    ]);
    let _ = clipboard.set_content(Some(&provider));
}

pub fn put_text(clipboard: &gdk::Clipboard, text: &str) {
    clipboard.set_text(text);
}

pub fn clear(clipboard: &gdk::Clipboard) {
    let _ = clipboard.set_content(None::<&gdk::ContentProvider>);
}

/// Files on the clipboard, and whether they were cut (to be moved) rather than copied.
pub async fn files(clipboard: &gdk::Clipboard) -> Option<(Vec<gio::File>, bool)> {
    let formats = clipboard.formats();
    if !formats.contain_mime_type(GNOME) && !formats.contain_mime_type(URIS) {
        return None;
    }
    let (stream, mime) = clipboard.read_future(&[GNOME, URIS], glib::Priority::DEFAULT).await.ok()?;
    let out = gio::MemoryOutputStream::new_resizable();
    out.splice_future(
        &stream,
        gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
        glib::Priority::DEFAULT,
    )
    .await
    .ok()?;
    let bytes = out.steal_as_bytes();
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    let cut = if mime == GNOME { lines.next() == Some("cut") } else { false };
    let files: Vec<gio::File> = lines.map(gio::File::for_uri).collect();
    (!files.is_empty()).then_some((files, cut))
}

/// A picture on the clipboard (e.g. from Jungey Snip), to paste as a PNG file.
pub async fn image(clipboard: &gdk::Clipboard) -> Option<gdk::Texture> {
    if !clipboard.formats().contains_type(gdk::Texture::static_type()) {
        return None;
    }
    clipboard.read_texture_future().await.ok().flatten()
}
