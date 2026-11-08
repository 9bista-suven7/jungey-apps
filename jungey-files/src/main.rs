//! Jungey Files: a light, fast file manager for Jungey OS.

mod clipboard;
mod config;
mod ops;
mod pathbar;
mod props;
mod rename;
mod sidebar;
mod thumbs;
mod util;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("os.jungey.Files")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|app| {
        jungey_ui::init();
        window::set_accels(app);
    });
    app.connect_activate(|app| {
        window::Win::open(app, &gio::File::for_path(glib::home_dir()), None);
    });
    // `jungey-files PATH...`: folders open as they are; files open their folder with the file selected
    app.connect_open(|app, files, _| {
        for f in files {
            let kind = f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE);
            match (kind, f.parent()) {
                (gio::FileType::Directory, _) | (_, None) => window::Win::open(app, f, None),
                (_, Some(parent)) => window::Win::open(app, &parent, Some(f.clone())),
            }
        }
    });
    app.run()
}
