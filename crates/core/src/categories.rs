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
    /// May Smart Scan tick this for you, without you choosing it?
    ///
    /// Policy, and deliberately a field rather than a list somewhere else. Two
    /// modules already answer this question — `privacy::Row::smart_scan_eligible`
    /// is derived next to the rows it describes — and a second answer kept in
    /// the aggregator would drift from this one silently. Sitting in the same
    /// struct literal as `id` and `subpath` means adding a category cannot
    /// inherit an answer; it has to give one.
    ///
    /// Pinned by [`tests::the_smart_scan_default_set_is_pinned`], so widening
    /// the set is an edit to that assertion rather than a side effect.
    pub smart_scan_default: bool,
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
        smart_scan_default: true,
    },
    Category {
        id: "xcode-derived-data",
        name: "Xcode derived data",
        description: "Xcode build intermediates and indexes; rebuilt automatically.",
        subpath: "Library/Developer/Xcode/DerivedData",
        smart_scan_default: true,
    },
    Category {
        id: "user-caches",
        name: "Application caches",
        description: "Per-user application caches; apps recreate what they need.",
        subpath: "Library/Caches",
        smart_scan_default: true,
    },
    Category {
        id: "user-logs",
        name: "Logs",
        description: "Per-user application and system log files.",
        subpath: "Library/Logs",
        smart_scan_default: true,
    },
    Category {
        id: "trash",
        name: "Trash",
        description: "Files already sitting in the user Trash.",
        subpath: ".Trash",
        // **Not a default, and the reason is not caution.** The Trash is the
        // recovery mechanism for everything else this app does — every other
        // module disposes *into* it. A Smart Scan that empties it by default
        // destroys the undo for its own other modules in the same click.
        //
        // There is a mechanical objection too: moving something already in
        // `~/.Trash` to the Trash is a rename, so the bytes reported as freed
        // would not have been freed.
        smart_scan_default: false,
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

    // --- what Smart Scan may tick for you -----------------------------------

    /// The default set is pinned, in the shape `allowlist::the_disposal_scope_is_pinned`
    /// uses: widening it must be a deliberate edit to this assertion rather than
    /// a side effect of adding a category.
    #[test]
    fn the_smart_scan_default_set_is_pinned() {
        let defaults: Vec<&str> = registry()
            .iter()
            .filter(|c| c.smart_scan_default)
            .map(|c| c.id)
            .collect();
        assert_eq!(
            defaults,
            vec![
                "homebrew-downloads",
                "xcode-derived-data",
                "user-caches",
                "user-logs"
            ]
        );
    }

    /// **The Trash is not in it**, and this is the assertion that says why.
    ///
    /// It is the recovery mechanism for everything else the same gesture does:
    /// a Smart Scan that empties it by default destroys the undo for its own
    /// other modules, in one click. There is a mechanical oddity too — moving
    /// something already in `~/.Trash` to the Trash is a rename, so the bytes
    /// reported as freed would not be freed.
    #[test]
    fn the_trash_is_never_ticked_for_you() {
        let trash = by_id("trash").expect("the trash category exists");
        assert!(!trash.smart_scan_default);
    }

    /// A category cannot exist without answering the question. The field has no
    /// default and sits in the same struct literal as `id` and `subpath`, so
    /// adding a fifth allowlist root forces an explicit answer rather than
    /// inheriting one.
    #[test]
    fn every_category_states_whether_smart_scan_defaults_it() {
        assert_eq!(
            registry().len(),
            5,
            "the registry changed — re-read the pin above"
        );
        assert!(registry().iter().filter(|c| c.smart_scan_default).count() < registry().len());
    }
}
