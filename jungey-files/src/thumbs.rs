//! Image previews. Uses the shared freedesktop thumbnail cache (~/.cache/thumbnails),
//! and makes missing ones itself, two at a time so the hard drive isn't swamped.
//! The most recently shown item goes first, so scrolling fast stays responsive.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib};

use crate::util;

const MAX_RUNNING: usize = 2;
const MAX_SOURCE_BYTES: i64 = 60 * 1024 * 1024;
const MEMORY_CACHE: usize = 800;

struct Job {
    info: gio::FileInfo,
    item: glib::WeakRef<gtk::ListItem>,
    image: glib::WeakRef<gtk::Image>,
}

#[derive(Default)]
pub struct Thumbs {
    queue: RefCell<VecDeque<Job>>,
    running: Cell<usize>,
    done: RefCell<HashMap<String, gdk::Texture>>,
}

impl Thumbs {
    pub fn new() -> Rc<Thumbs> {
        Rc::new(Thumbs::default())
    }

    /// Ask for a preview of `info` to be put into `image` once ready, as long as
    /// `item` still shows the same file by then.
    pub fn request(self: &Rc<Self>, info: &gio::FileInfo, item: &gtk::ListItem, image: &gtk::Image) {
        let has_cached = info.attribute_byte_string("thumbnail::path").is_some()
            && info.boolean("thumbnail::is-valid");
        if !has_cached && !util::is_image(info) {
            return;
        }
        let uri = util::file_of(info).uri().to_string();
        if let Some(tex) = self.done.borrow().get(&uri) {
            image.set_paintable(Some(tex));
            return;
        }
        self.queue.borrow_mut().push_back(Job { info: info.clone(), item: item.downgrade(), image: image.downgrade() });
        self.pump();
    }

    pub fn clear_queue(&self) {
        self.queue.borrow_mut().clear();
    }

    fn pump(self: &Rc<Self>) {
        while self.running.get() < MAX_RUNNING {
            let Some(job) = self.queue.borrow_mut().pop_back() else { return };
            if !still_shown(&job) {
                continue;
            }
            self.running.set(self.running.get() + 1);
            let this = self.clone();
            glib::spawn_future_local(async move {
                let input = Input::from(&job.info);
                let uri = input.uri.clone();
                let png = gio::spawn_blocking(move || load_or_make(input)).await.ok().flatten();
                // textures must be made on the main thread; decoding a 256px PNG is quick
                let tex = png.and_then(|b| gdk::Texture::from_bytes(&glib::Bytes::from_owned(b)).ok());
                if let Some(tex) = tex {
                    {
                        let mut done = this.done.borrow_mut();
                        if done.len() > MEMORY_CACHE {
                            done.clear();
                        }
                        done.insert(uri, tex.clone());
                    }
                    if still_shown(&job) {
                        if let Some(image) = job.image.upgrade() {
                            image.set_paintable(Some(&tex));
                        }
                    }
                }
                this.running.set(this.running.get() - 1);
                this.pump();
            });
        }
    }
}

fn still_shown(job: &Job) -> bool {
    job.item
        .upgrade()
        .and_then(|item| item.item())
        .map(|obj| obj == *job.info.upcast_ref::<glib::Object>())
        .unwrap_or(false)
}

/// What the worker thread needs (FileInfo itself isn't Send).
struct Input {
    uri: String,
    path: Option<PathBuf>,
    cached: Option<PathBuf>,
    mtime: u64,
    size: i64,
    image: bool,
}

impl Input {
    fn from(info: &gio::FileInfo) -> Input {
        let file = util::file_of(info);
        Input {
            uri: file.uri().to_string(),
            path: file.path(),
            cached: info
                .attribute_byte_string("thumbnail::path")
                .filter(|_| info.boolean("thumbnail::is-valid"))
                .map(|p| PathBuf::from(p.as_str())),
            mtime: info.attribute_uint64("time::modified"),
            size: info.size(),
            image: util::is_image(info),
        }
    }
}

/// PNG bytes of the preview: from the cache, or freshly made (and cached).
fn load_or_make(input: Input) -> Option<Vec<u8>> {
    if let Some(cached) = &input.cached {
        if let Ok(bytes) = std::fs::read(cached) {
            return Some(bytes);
        }
    }
    let path = input.path.as_ref()?;
    if !input.image || input.size > MAX_SOURCE_BYTES {
        return None;
    }
    let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, &input.uri)?;
    let dir = glib::user_cache_dir().join("thumbnails/large");
    let out = dir.join(format!("{md5}.png"));
    // reuse our own earlier preview unless the image changed since
    let fresh = std::fs::metadata(&out)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|age| age.as_secs() >= input.mtime);
    if fresh {
        if let Ok(bytes) = std::fs::read(&out) {
            return Some(bytes);
        }
    }
    let pixbuf = gdk_pixbuf::Pixbuf::from_file_at_scale(path, 256, 256, true).ok()?;
    let pixbuf = pixbuf.apply_embedded_orientation().unwrap_or(pixbuf);
    let mtime = input.mtime.to_string();
    let opts = [("tEXt::Thumb::URI", input.uri.as_str()), ("tEXt::Thumb::MTime", mtime.as_str()), ("tEXt::Software", "Jungey Files")];
    let bytes = pixbuf.save_to_bufferv("png", &opts).ok()?;
    // share it with every app (spec: write to a temp name, then rename)
    if std::fs::create_dir_all(&dir).is_ok() {
        let tmp = dir.join(format!(".{md5}.{}.png", std::process::id()));
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &out);
        }
    }
    Some(bytes)
}
