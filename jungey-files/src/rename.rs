//! Renaming: a quick dialog for one item, and Bulk Rename for many
//! (find & replace, numbering, change case) with a live preview.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::util;

/// Ask for a new name for `file`. Returns it once confirmed.
pub async fn ask_one(parent: &impl IsA<gtk::Widget>, heading: &str, current: &str, action: &str) -> Option<String> {
    let entry = gtk::Entry::builder().text(current).activates_default(true).build();
    let dialog = adw::AlertDialog::new(Some(heading), None);
    dialog.add_responses(&[("cancel", "Cancel"), ("ok", action)]);
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("cancel");
    dialog.set_extra_child(Some(&entry));
    entry.connect_changed(glib::clone!(
        #[weak]
        dialog,
        move |e| {
            let t = e.text();
            dialog.set_response_enabled("ok", valid(&t));
        }
    ));
    // select the name without its extension, like other file managers
    let stem = util::split_ext(current).0.chars().count() as i32;
    entry.connect_map(move |e| {
        e.grab_focus();
        e.select_region(0, stem);
    });
    let answer = dialog.choose_future(Some(parent)).await;
    let name = entry.text().trim().to_string();
    (answer == "ok" && valid(&name) && name != current).then_some(name)
}

fn valid(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && !n.contains('/') && n != "." && n != ".."
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Replace,
    Number,
    Case,
}

struct Bulk {
    files: Vec<(gio::File, String)>,
    mode: RefCell<Mode>,
    find: gtk::Entry,
    replace: gtk::Entry,
    match_case: gtk::CheckButton,
    pattern: gtk::Entry,
    start: gtk::SpinButton,
    case: gtk::DropDown,
    keep_ext: gtk::CheckButton,
    preview: gtk::ListBox,
    apply: gtk::Button,
}

impl Bulk {
    fn new_name(&self, index: usize, old: &str) -> String {
        let (stem, ext) = util::split_ext(old);
        let keep = self.keep_ext.is_active();
        let (base, tail) = if keep { (stem, ext) } else { (old, "") };
        let out = match *self.mode.borrow() {
            Mode::Replace => {
                let find = self.find.text();
                if find.is_empty() {
                    base.to_string()
                } else if self.match_case.is_active() {
                    base.replace(find.as_str(), &self.replace.text())
                } else {
                    replace_ci(base, &find, &self.replace.text())
                }
            }
            Mode::Number => {
                let n = self.start.value() as usize + index;
                let width = (self.start.value() as usize + self.files.len() - 1).to_string().len().max(2);
                let num = format!("{n:0width$}");
                let p = self.pattern.text();
                if p.is_empty() {
                    num
                } else if p.contains('#') {
                    p.replace('#', &num)
                } else {
                    format!("{p} {num}")
                }
            }
            Mode::Case => match self.case.selected() {
                0 => base.to_lowercase(),
                1 => base.to_uppercase(),
                _ => title_case(base),
            },
        };
        format!("{out}{tail}")
    }

    /// New names, and whether each clashes with another or with an existing file.
    fn plan(&self) -> Vec<(String, bool)> {
        let names: Vec<String> = self.files.iter().enumerate().map(|(i, (_, n))| self.new_name(i, n)).collect();
        names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let dup = names.iter().enumerate().any(|(j, m)| j != i && m == n);
                let (file, old) = &self.files[i];
                let taken = n != old
                    && !self.files.iter().any(|(_, o)| o == n)
                    && file.parent().map(|p| p.child(n).query_exists(gio::Cancellable::NONE)).unwrap_or(false);
                (n.clone(), dup || taken || !valid(n))
            })
            .collect()
    }

    fn refresh(&self) {
        while let Some(c) = self.preview.first_child() {
            self.preview.remove(&c);
        }
        let plan = self.plan();
        let mut changes = 0;
        let mut bad = false;
        for ((_, old), (new, conflict)) in self.files.iter().zip(&plan) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.set_margin_top(6);
            row.set_margin_bottom(6);
            row.set_margin_start(10);
            row.set_margin_end(10);
            let o = gtk::Label::new(Some(old));
            o.set_xalign(0.0);
            o.set_hexpand(true);
            o.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            o.add_css_class("dim-label");
            let arrow = gtk::Label::new(Some("→"));
            arrow.add_css_class("dim-label");
            let n = gtk::Label::new(Some(new));
            n.set_xalign(0.0);
            n.set_hexpand(true);
            n.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            if new != old {
                n.add_css_class("jungey-rename-new");
                changes += 1;
            }
            if *conflict {
                n.add_css_class("conflict");
                n.set_tooltip_text(Some("This name is already taken or isn't allowed"));
                bad = true;
            }
            row.append(&o);
            row.append(&arrow);
            row.append(&n);
            self.preview.append(&row);
        }
        self.apply.set_sensitive(changes > 0 && !bad);
        self.apply.set_label(&if changes > 0 { format!("Rename {changes}") } else { "Rename".into() });
    }
}

fn replace_ci(s: &str, find: &str, with: &str) -> String {
    let lower = s.to_lowercase();
    let needle = find.to_lowercase();
    if needle.is_empty() || lower.len() != s.len() {
        return s.replace(find, with);
    }
    let mut out = String::new();
    let mut i = 0;
    while let Some(pos) = lower[i..].find(&needle) {
        out.push_str(&s[i..i + pos]);
        out.push_str(with);
        i += pos + needle.len();
    }
    out.push_str(&s[i..]);
    out
}

fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start = c.is_whitespace() || c == '-' || c == '_';
    }
    out
}

/// Open Bulk Rename for `files` (in the order shown). `done` gets the renamed files.
pub fn bulk(parent: &impl IsA<gtk::Widget>, files: Vec<gio::File>, done: impl Fn(Result<Vec<gio::File>, glib::Error>) + 'static) {
    let files: Vec<(gio::File, String)> = files
        .into_iter()
        .filter_map(|f| {
            let n = f.basename()?.to_string_lossy().to_string();
            Some((f, n))
        })
        .collect();
    let count = files.len();

    let entry = |placeholder: &str| gtk::Entry::builder().placeholder_text(placeholder).hexpand(true).build();
    let find = entry("Find");
    let replace = entry("Replace with");
    let match_case = gtk::CheckButton::with_label("Match case");
    let pattern = entry("Name, # for the number (e.g. Photo #)");
    let start = gtk::SpinButton::with_range(0.0, 99999.0, 1.0);
    start.set_value(1.0);
    let case = gtk::DropDown::from_strings(&["lowercase", "UPPERCASE", "Title Case"]);
    let keep_ext = gtk::CheckButton::with_label("Keep extensions");
    keep_ext.set_active(true);
    let preview = gtk::ListBox::new();
    preview.add_css_class("boxed-list");
    preview.set_selection_mode(gtk::SelectionMode::None);
    let apply = gtk::Button::with_label("Rename");
    apply.add_css_class("suggested-action");

    let stack = adw::ViewStack::new();
    let replace_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    replace_page.append(&find);
    replace_page.append(&replace);
    replace_page.append(&match_case);
    stack.add_titled_with_icon(&replace_page, Some("replace"), "Replace", "edit-find-replace-symbolic");
    let number_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    number_page.append(&pattern);
    let start_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    start_row.append(&gtk::Label::new(Some("Start at")));
    start_row.append(&start);
    number_page.append(&start_row);
    stack.add_titled_with_icon(&number_page, Some("number"), "Number", "view-list-ordered-symbolic");
    let case_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    case_page.append(&case);
    stack.add_titled_with_icon(&case_page, Some("case"), "Case", "format-text-rich-symbolic");

    let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    body.set_margin_top(12);
    body.set_margin_bottom(18);
    body.set_margin_start(18);
    body.set_margin_end(18);
    body.append(&switcher);
    body.append(&stack);
    body.append(&keep_ext);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&preview)
        .vexpand(true)
        .min_content_height(220)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    body.append(&scroller);

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_show_start_title_buttons(false);
    let cancel = gtk::Button::with_label("Cancel");
    header.pack_start(&cancel);
    header.pack_end(&apply);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&body));

    let dialog = adw::Dialog::builder()
        .title(format!("Rename {count} Items"))
        .content_width(620)
        .content_height(600)
        .child(&view)
        .build();

    let bulk = Rc::new(Bulk { files, mode: RefCell::new(Mode::Replace), find, replace, match_case, pattern, start, case, keep_ext, preview, apply });

    let refresh = {
        let bulk = bulk.clone();
        move || bulk.refresh()
    };
    for e in [&bulk.find, &bulk.replace, &bulk.pattern] {
        let r = refresh.clone();
        e.connect_changed(move |_| r());
    }
    for c in [&bulk.match_case, &bulk.keep_ext] {
        let r = refresh.clone();
        c.connect_toggled(move |_| r());
    }
    let r = refresh.clone();
    bulk.start.connect_value_changed(move |_| r());
    let r = refresh.clone();
    bulk.case.connect_selected_notify(move |_| r());
    {
        let bulk = bulk.clone();
        stack.connect_visible_child_name_notify(move |s| {
            let mode = match s.visible_child_name().as_deref() {
                Some("number") => Mode::Number,
                Some("case") => Mode::Case,
                _ => Mode::Replace,
            };
            bulk.mode.replace(mode);
            bulk.refresh();
        });
    }
    bulk.refresh();

    cancel.connect_clicked(glib::clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));
    let done = Rc::new(done);
    bulk.apply.connect_clicked(glib::clone!(
        #[weak]
        dialog,
        #[strong]
        bulk,
        move |_| {
            let pairs: Vec<(gio::File, String)> = bulk
                .files
                .iter()
                .zip(bulk.plan())
                .filter(|((_, old), (new, _))| old != new)
                .map(|((f, _), (new, _))| (f.clone(), new))
                .collect();
            dialog.close();
            let done = done.clone();
            glib::spawn_future_local(async move {
                let res = gio::spawn_blocking(move || apply_renames(pairs)).await;
                done(res.unwrap_or_else(|_| Err(crate::ops::err("Rename failed"))));
            });
        }
    ));
    dialog.present(Some(parent));
}

/// Two passes through temporary names, so swaps like a<->b work. If anything
/// fails, the files that were already touched get their old names back.
fn apply_renames(pairs: Vec<(gio::File, String)>) -> Result<Vec<gio::File>, glib::Error> {
    let none = gio::Cancellable::NONE;
    let tag = std::process::id();
    let undo = |temps: &[(gio::File, String)]| {
        for (t, orig) in temps {
            let _ = t.set_display_name(orig, none);
        }
    };
    let mut temps: Vec<(gio::File, String)> = Vec::new();
    for (i, (file, _)) in pairs.iter().enumerate() {
        let orig = file.basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_default();
        match file.set_display_name(&format!(".jungey-rename-{tag}-{i}"), none) {
            Ok(t) => temps.push((t, orig)),
            Err(e) => {
                undo(&temps);
                return Err(e);
            }
        }
    }
    let mut out = Vec::new();
    for (k, ((temp, _), (_, new))) in temps.iter().zip(&pairs).enumerate() {
        match temp.set_display_name(new, none) {
            Ok(f) => out.push(f),
            Err(e) => {
                undo(&temps[k..]);
                return Err(e);
            }
        }
    }
    Ok(out)
}
