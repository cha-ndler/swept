//! Plan the emptying of the user Trash. Read-only: this module never mutates.
//!
//! Every other cleanup in Swept *moves* things into `~/.Trash`, which is what
//! makes it recoverable. Emptying the Trash is the one gesture that gives that
//! up, so it has its own planner rather than a flag on the scanner:
//!
//! - **Scope is the Trash and nothing else.** One root, `~/.Trash`, and no
//!   age or size filter — the user is asked about everything in it.
//! - **Item-level, never half an item.** Each top-level entry of the Trash is
//!   one thing the user threw away. If anything beneath it cannot be removed
//!   safely — a symlink, a special file, a `.git`, a file the scan refused, a
//!   folder it could not read — the whole item is left, never partly deleted.
//! - **Files first, folders after.** Files become [`Disposal::Permanent`]
//!   actions for [`crate::executor::execute`]; the folders they leave empty
//!   become [`PlannedPrune`]s, deepest first, for
//!   [`crate::executor::prune_emptied_trash_dirs`]. The Trash folder itself
//!   is never among them.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::plan::{user_trash_root, Disposal, Plan, PlannedPrune};
use crate::scanner::{scan, ScanConfig};

/// The category every emptying action carries. It reaches the audit log on
/// each permanent line, so the log says which gesture authorized it.
pub const EMPTIED_CATEGORY: &str = "trash-emptied";

/// What emptying the Trash would do.
#[derive(Debug, Default)]
pub struct EmptyTrashPlan {
    /// Files to remove permanently; each `Disposal::Permanent`.
    pub files: Plan,
    /// Folders to remove once empty, deepest first. Never the Trash itself.
    pub prune: Vec<PlannedPrune>,
    /// Top-level entries in the Trash, planned or not.
    pub items: usize,
    /// Top-level entries that will be left whole, and why is in the module doc.
    pub left_behind: usize,
    /// The Trash exists but could not be read — on macOS, no Full Disk Access.
    /// A plan with this set is a hole, not an empty Trash.
    pub unreadable_root: bool,
}

impl EmptyTrashPlan {
    /// Identify exactly what this plan would remove.
    ///
    /// A count and a byte total cannot bind a consent: Put Back one file, throw
    /// away another of the same size, and the figures are unchanged while the
    /// Trash holds something the user was never shown. So the request carries
    /// this instead, and the run refuses unless its fresh plan has the same one.
    ///
    /// Covers every planned file by path, device, inode, size and modification
    /// time — so a file replaced under the same name and size differs too — and
    /// every folder to be pruned by path and inode. Read at call time, not from
    /// the scan, so a preview and a run see the disk the same way.
    ///
    /// `DefaultHasher::new()` uses fixed keys, so this is stable within one
    /// build, which is all it has to be: the preview and the run it binds are
    /// served by the same process.
    pub fn fingerprint(&self) -> String {
        use std::hash::{DefaultHasher, Hash, Hasher};
        use std::os::unix::fs::MetadataExt;

        let mut files: Vec<&Path> = self
            .files
            .actions
            .iter()
            .map(|a| a.path.as_path())
            .collect();
        files.sort();
        let mut dirs: Vec<&Path> = self.prune.iter().map(|d| d.path()).collect();
        dirs.sort();

        let mut h = DefaultHasher::new();
        for p in files {
            p.hash(&mut h);
            match std::fs::symlink_metadata(p) {
                Ok(m) => (m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec()).hash(&mut h),
                Err(_) => "gone".hash(&mut h),
            }
        }
        "folders".hash(&mut h);
        for p in dirs {
            p.hash(&mut h);
            match std::fs::symlink_metadata(p) {
                Ok(m) => (m.dev(), m.ino()).hash(&mut h),
                Err(_) => "gone".hash(&mut h),
            }
        }
        format!("{:016x}", h.finish())
    }
}

/// Plan the emptying of `home`'s Trash. `home` must be canonical.
pub fn plan(home: &Path) -> EmptyTrashPlan {
    let root = user_trash_root(home);
    let mut out = EmptyTrashPlan::default();

    // A Trash that is a symlink points somewhere this gesture was never about.
    // Nothing beneath it is planned.
    match std::fs::symlink_metadata(&root) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return out,
        Err(_) => {
            out.unreadable_root = true;
            return out;
        }
    }
    let entries = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return out,
        Err(_) => {
            out.unreadable_root = true;
            return out;
        }
    };
    let mut tops: Vec<PathBuf> = Vec::new();
    for e in entries {
        match e {
            Ok(e) => tops.push(e.path()),
            // An entry the listing itself could not produce is unknowable; it
            // is not planned, and counts as left.
            Err(_) => {
                out.items += 1;
                out.left_behind += 1;
            }
        }
    }
    out.items += tops.len();

    let files = scan(&ScanConfig {
        home: home.to_path_buf(),
        roots: vec![root.clone()],
        min_age: None,
        min_size: None,
    });
    let planned: HashSet<PathBuf> = files
        .actions
        .iter()
        .map(|a| a.path.as_path().to_path_buf())
        .collect();

    let mut keep: Vec<PathBuf> = Vec::new();
    for top in &tops {
        match vet_item(top, home, &planned) {
            Some(mut dirs) => {
                keep.push(top.clone());
                out.prune.append(&mut dirs);
            }
            None => out.left_behind += 1,
        }
    }

    out.files = files;
    out.files
        .actions
        .retain(|a| keep.iter().any(|k| a.path.as_path().starts_with(k)));
    for a in &mut out.files.actions {
        a.disposal = Disposal::Permanent;
        a.category = EMPTIED_CATEGORY.to_string();
    }
    // Deepest first, so each folder is empty by the time its turn comes.
    out.prune
        .sort_by_key(|d| std::cmp::Reverse(d.path().components().count()));
    out
}

/// Vet one top-level item. `Some(folders)` if every file beneath it is planned
/// and every folder is prunable; `None` if any part of it must stay.
fn vet_item(top: &Path, home: &Path, planned: &HashSet<PathBuf>) -> Option<Vec<PlannedPrune>> {
    let mut dirs = Vec::new();
    for entry in WalkDir::new(top).follow_links(false) {
        let entry = entry.ok()?;
        let ft = entry.file_type();
        if ft.is_file() {
            if !planned.contains(entry.path()) {
                return None;
            }
        } else if ft.is_dir() {
            dirs.push(PlannedPrune::new(entry.path(), home).ok()?);
        } else {
            // A symlink, socket, FIFO or device. Never followed, never removed.
            return None;
        }
    }
    Some(dirs)
}
