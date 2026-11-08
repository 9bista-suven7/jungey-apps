//! The apps to show, how they rank in search, and how often each one is opened.

use std::collections::HashMap;
use std::path::PathBuf;

use gtk::prelude::*;
use gtk::{gio, glib};

#[derive(Clone)]
pub struct App {
    pub id: String,
    pub name: String,
    pub icon: Option<gio::Icon>,
    pub info: gio::AppInfo,
    haystack: String,
}

/// Every app that belongs in a launcher, sorted by name.
pub fn all() -> Vec<App> {
    let mut apps: Vec<App> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .map(|info| {
            let name = info.display_name().to_string();
            let haystack = format!(
                "{} {} {} {} {}",
                name,
                info.name(),
                info.description().unwrap_or_default(),
                info.executable().to_string_lossy(),
                info.id().unwrap_or_default(),
            )
            .to_lowercase();
            App { id: info.id().map(|s| s.to_string()).unwrap_or_default(), name, icon: info.icon(), info, haystack }
        })
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

/// Apps matching `query`, best first: name starts with it, then a word in the name
/// does, then the name contains it, then anything else about the app does.
pub fn search<'a>(apps: &'a [App], query: &str) -> Vec<&'a App> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<(u8, &App)> = apps
        .iter()
        .filter_map(|a| {
            let name = a.name.to_lowercase();
            let score = if name.starts_with(&q) {
                0
            } else if name.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q)) {
                1
            } else if name.contains(&q) {
                2
            } else if a.haystack.contains(&q) {
                3
            } else {
                return None;
            };
            Some((score, a))
        })
        .collect();
    hits.sort_by(|(sa, a), (sb, b)| sa.cmp(sb).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    hits.into_iter().map(|(_, a)| a).collect()
}

/// How often each app was opened from the Launchpad, for the Frequent view.
#[derive(Default)]
pub struct Usage {
    counts: HashMap<String, u32>,
}

fn usage_file() -> PathBuf {
    glib::user_data_dir().join("jungey/launchpad-usage.ini")
}

impl Usage {
    pub fn load() -> Usage {
        let kf = glib::KeyFile::new();
        let mut counts = HashMap::new();
        if kf.load_from_file(usage_file(), glib::KeyFileFlags::NONE).is_ok() {
            if let Ok(keys) = kf.keys("usage") {
                for k in keys.iter() {
                    if let Ok(n) = kf.integer("usage", k) {
                        counts.insert(k.to_string(), n.max(0) as u32);
                    }
                }
            }
        }
        Usage { counts }
    }

    pub fn bump(&mut self, id: &str) {
        *self.counts.entry(id.to_string()).or_default() += 1;
        let kf = glib::KeyFile::new();
        for (k, v) in &self.counts {
            kf.set_integer("usage", k, *v as i32);
        }
        let path = usage_file();
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = kf.save_to_file(path);
    }

    /// The most-opened apps, most first.
    pub fn top<'a>(&self, apps: &'a [App], n: usize) -> Vec<&'a App> {
        let mut used: Vec<&App> = apps.iter().filter(|a| self.counts.contains_key(&a.id)).collect();
        used.sort_by(|a, b| self.counts[&b.id].cmp(&self.counts[&a.id]).then_with(|| a.name.cmp(&b.name)));
        used.truncate(n);
        used
    }
}
