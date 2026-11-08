//! File operations. Everything here blocks, so callers run it via `gio::spawn_blocking`.
//! Nothing is ever overwritten: name clashes get "name (2).ext".

use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;

use crate::util::{self, ArchiveKind};

type Res<T> = Result<T, glib::Error>;
const NONE: Option<&gio::Cancellable> = gio::Cancellable::NONE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transfer {
    Copy,
    Move,
}

/// Copy or move `files` into `dest`. Returns the new locations.
pub fn transfer(files: &[gio::File], dest: &gio::File, how: Transfer) -> Res<Vec<gio::File>> {
    let mut out = Vec::new();
    for src in files {
        let name = src.basename().ok_or_else(|| err("A file has no name"))?;
        if how == Transfer::Move && src.parent().as_ref() == Some(dest) {
            out.push(src.clone()); // already here
            continue;
        }
        if dest.has_prefix(src) || dest == src {
            return Err(err("Can't put a folder inside itself"));
        }
        let target = util::unique_child(dest, &name.to_string_lossy());
        match how {
            Transfer::Copy => copy_recursive(src, &target)?,
            Transfer::Move => move_one(src, &target)?,
        }
        out.push(target);
    }
    Ok(out)
}

fn move_one(src: &gio::File, target: &gio::File) -> Res<()> {
    let flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE;
    match src.move_(target, flags, NONE, None) {
        Ok(()) => Ok(()),
        // across drives: copy, then remove the original
        Err(e) if e.matches(gio::IOErrorEnum::WouldRecurse) || e.matches(gio::IOErrorEnum::NotSupported) => {
            copy_recursive(src, target)?;
            delete_recursive(src)
        }
        Err(e) => Err(e),
    }
}

fn copy_recursive(src: &gio::File, target: &gio::File) -> Res<()> {
    let kind = src.query_file_type(gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, NONE);
    if kind == gio::FileType::Directory {
        target.make_directory(NONE)?;
        let children = src.enumerate_children("standard::name", gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, NONE)?;
        while let Some(info) = children.next_file(NONE)? {
            let name = info.name();
            copy_recursive(&src.child(&name), &target.child(&name))?;
        }
        Ok(())
    } else {
        src.copy(target, gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::ALL_METADATA, NONE, None)
    }
}

/// Permanent delete. Only used to finish a cross-drive move, and for the Trash's
/// own "Delete Permanently" / "Empty Trash".
pub fn delete_recursive(file: &gio::File) -> Res<()> {
    let kind = file.query_file_type(gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, NONE);
    if kind == gio::FileType::Directory && file.is_native() {
        let children = file.enumerate_children("standard::name", gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, NONE)?;
        while let Some(info) = children.next_file(NONE)? {
            delete_recursive(&file.child(info.name()))?;
        }
    }
    file.delete(NONE)
}

pub fn trash(files: &[gio::File]) -> Res<usize> {
    for f in files {
        f.trash(NONE)?;
    }
    Ok(files.len())
}

/// Put trashed items back where they came from.
pub fn restore(files: &[gio::File]) -> Res<usize> {
    for f in files {
        let info = f.query_info("trash::orig-path", gio::FileQueryInfoFlags::NONE, NONE)?;
        let orig = info
            .attribute_byte_string("trash::orig-path")
            .ok_or_else(|| err("Don't know where this item came from"))?;
        let orig = gio::File::for_path(orig.as_str());
        let parent = orig.parent().ok_or_else(|| err("Original folder is gone"))?;
        if !parent.query_exists(NONE) {
            parent.make_directory_with_parents(NONE)?;
        }
        let name = orig.basename().unwrap();
        let target = util::unique_child(&parent, &name.to_string_lossy());
        f.move_(&target, gio::FileCopyFlags::NOFOLLOW_SYMLINKS, NONE, None)?;
    }
    Ok(files.len())
}

/// Undo a "Move to Trash": find the newest trashed item for each original path and put it back.
pub fn untrash(originals: &[std::path::PathBuf]) -> Res<usize> {
    let trash = gio::File::for_uri("trash:///");
    let children = trash.enumerate_children(
        "standard::name,trash::orig-path,trash::deletion-date",
        gio::FileQueryInfoFlags::NONE,
        NONE,
    )?;
    let mut newest: std::collections::HashMap<std::path::PathBuf, (String, gio::File)> = Default::default();
    while let Some(info) = children.next_file(NONE)? {
        let Some(orig) = info.attribute_byte_string("trash::orig-path") else { continue };
        let orig = std::path::PathBuf::from(orig.as_str());
        if !originals.contains(&orig) {
            continue;
        }
        let when = info.attribute_string("trash::deletion-date").map(|s| s.to_string()).unwrap_or_default();
        let item = trash.child(info.name());
        if newest.get(&orig).map(|(w, _)| when > *w).unwrap_or(true) {
            newest.insert(orig, (when, item));
        }
    }
    let items: Vec<gio::File> = newest.into_values().map(|(_, f)| f).collect();
    restore(&items)
}

pub fn empty_trash() -> Res<()> {
    let trash = gio::File::for_uri("trash:///");
    let children = trash.enumerate_children("standard::name", gio::FileQueryInfoFlags::NONE, NONE)?;
    while let Some(info) = children.next_file(NONE)? {
        trash.child(info.name()).delete(NONE)?;
    }
    Ok(())
}

pub fn new_folder(dir: &gio::File, name: &str) -> Res<gio::File> {
    let f = util::unique_child(dir, name);
    f.make_directory(NONE)?;
    Ok(f)
}

/// Zip the files (all in `dir`) into a new archive next to them.
pub fn compress(dir: &gio::File, files: &[gio::File]) -> Res<gio::File> {
    let cwd = dir.path().ok_or_else(|| err("Can only compress files on this computer"))?;
    let name = match files {
        [one] => format!("{}.zip", util::split_ext(&one.basename().unwrap().to_string_lossy()).0),
        _ => "Archive.zip".to_string(),
    };
    let archive = util::unique_child(dir, &name);
    let mut args: Vec<std::ffi::OsString> = vec!["zip".into(), "-r".into(), "-q".into(), "-y".into()];
    args.push(archive.path().unwrap().into_os_string());
    args.push("--".into());
    args.extend(files.iter().filter_map(|f| f.basename()).map(|b| b.into_os_string()));
    run(&cwd, &args)?;
    Ok(archive)
}

/// Extract into a folder named after the archive; if everything sits in one
/// top-level folder, that folder is used directly (no "x/x/" nesting).
pub fn extract(archive: &gio::File) -> Res<gio::File> {
    let path = archive.path().ok_or_else(|| err("Can only extract files on this computer"))?;
    let dir = archive.parent().unwrap();
    let name = archive.basename().unwrap().to_string_lossy().to_string();
    let kind = util::archive_kind(&name).ok_or_else(|| err("Not an archive this app can open"))?;
    let out = util::unique_child(&dir, util::split_ext(&name).0);
    out.make_directory(NONE)?;
    let out_path = out.path().unwrap();
    let args: Vec<std::ffi::OsString> = match kind {
        ArchiveKind::Zip => vec!["unzip".into(), "-q".into(), path.into_os_string(), "-d".into(), out_path.clone().into_os_string()],
        ArchiveKind::Tar => vec!["tar".into(), "-xf".into(), path.into_os_string(), "-C".into(), out_path.clone().into_os_string()],
    };
    if let Err(e) = run(&out_path, &args) {
        let _ = delete_recursive(&out);
        return Err(e);
    }
    let entries: Vec<_> = std::fs::read_dir(&out_path).map(|r| r.flatten().collect()).unwrap_or_default();
    if let [only] = entries.as_slice() {
        if only.path().is_dir() {
            // step the wrapper aside first, so "proj.zip" holding "proj/" becomes "proj", not "proj (2)"
            let aside = util::unique_child(&dir, &format!(".jungey-extract-{}", std::process::id()));
            out.move_(&aside, gio::FileCopyFlags::NONE, NONE, None)?;
            let lifted = util::unique_child(&dir, &only.file_name().to_string_lossy());
            aside.child(only.file_name()).move_(&lifted, gio::FileCopyFlags::NONE, NONE, None)?;
            aside.delete(NONE)?;
            return Ok(lifted);
        }
    }
    Ok(out)
}

fn run(cwd: &std::path::Path, args: &[std::ffi::OsString]) -> Res<()> {
    let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDERR_PIPE | gio::SubprocessFlags::STDOUT_SILENCE);
    launcher.set_cwd(cwd);
    let argv: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_os_str()).collect();
    let p = launcher.spawn(&argv)?;
    let (_, stderr) = p.communicate_utf8(None, NONE)?;
    if !p.is_successful() {
        let msg = stderr.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        return Err(err(&msg.unwrap_or_else(|| format!("{} failed", args[0].to_string_lossy()))));
    }
    Ok(())
}

/// Total size and file count under the given files (local files only).
pub fn measure(files: &[gio::File]) -> (u64, u64) {
    fn walk(p: &std::path::Path, size: &mut u64, count: &mut u64) {
        let Ok(meta) = std::fs::symlink_metadata(p) else { return };
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    walk(&e.path(), size, count);
                }
            }
        } else {
            *size += meta.len();
            *count += 1;
        }
    }
    let (mut size, mut count) = (0, 0);
    for f in files {
        if let Some(p) = f.path() {
            walk(&p, &mut size, &mut count);
        }
    }
    (size, count)
}

pub fn err(msg: &str) -> glib::Error {
    glib::Error::new(gio::IOErrorEnum::Failed, msg)
}
