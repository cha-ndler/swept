//! First-class cleaner categories.
//!
//! A labeling layer over the safety allowlist: it gives each cleanable file a
//! user-facing category (id, name, description) by longest-prefix match. This is
//! purely descriptive — it does **not** widen what gets cleaned. The set of
//! locations that may be touched is still governed entirely by
//! [`safety::allowlist`] and the denylist; a category whose `subpath` lies
//! inside an allowed root only refines the label.

use std::path::Path;

/// A named, user-facing class of cleanable files.
#[derive(Debug, Clone, Copy)]
pub struct Category {
    /// Stable machine id (used in `--json`).
    pub id: &'static str,
    /// Human-facing name.
    pub name: &'static str,
    /// Short explanation of what this is and why it's safe to remove.
    pub description: &'static str,
    /// Path relative to the home directory that this category covers.
    pub subpath: &'static str,
}

/// All known categories. More specific (deeper) subpaths are matched ahead of
/// their parents by [`classify`] regardless of array order, but they are listed
/// specific-first for readability.
static CATEGORIES: &[Category] = &[
    Category {
        id: "homebrew-downloads",
        name: "Homebrew downloads",
        description: "Cached Homebrew package downloads; re-downloaded on demand.",
        subpath: "Library/Caches/Homebrew",
    },
    Category {
        id: "xcode-derived-data",
        name: "Xcode derived data",
        description: "Xcode build intermediates and indexes; rebuilt automatically.",
        subpath: "Library/Developer/Xcode/DerivedData",
    },
    Category {
        id: "user-caches",
        name: "Application caches",
        description: "Per-user application caches; apps recreate what they need.",
        subpath: "Library/Caches",
    },
    Category {
        id: "user-logs",
        name: "Logs",
        description: "Per-user application and system log files.",
        subpath: "Library/Logs",
    },
    Category {
        id: "trash",
        name: "Trash",
        description: "Files already sitting in the user Trash.",
        subpath: ".Trash",
    },
];

/// The full registry (e.g. for a GUI to display all categories).
pub fn registry() -> &'static [Category] {
    CATEGORIES
}

/// The most specific category whose `subpath` contains `path` (canonical), or
/// `None` if none applies.
pub fn classify(path: &Path, home: &Path) -> Option<&'static Category> {
    CATEGORIES
        .iter()
        .filter(|c| path.starts_with(home.join(c.subpath)))
        .max_by_key(|c| Path::new(c.subpath).components().count())
}

/// Look up a category by its id.
pub fn by_id(id: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        PathBuf::from("/Users/tester")
    }

    #[test]
    fn most_specific_category_wins() {
        let h = home();
        let brew = h.join("Library/Caches/Homebrew/downloads/pkg.tar");
        assert_eq!(classify(&brew, &h).unwrap().id, "homebrew-downloads");
        let app = h.join("Library/Caches/app/blob");
        assert_eq!(classify(&app, &h).unwrap().id, "user-caches");
    }

    #[test]
    fn unknown_location_is_unclassified() {
        let h = home();
        assert!(classify(&h.join("Documents/file.txt"), &h).is_none());
    }

    #[test]
    fn registry_ids_unique_and_documented() {
        let mut ids: Vec<&str> = registry().iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), len, "category ids must be unique");
        assert!(registry()
            .iter()
            .all(|c| !c.name.is_empty() && !c.description.is_empty()));
    }
}
