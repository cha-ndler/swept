//! Scoped allowlist. Cleanup is confined to these known-safe locations.
//!
//! Encodes SAFETY CONTRACT item 3. The denylist is always checked first and
//! overrides the allowlist; membership here only makes a path *eligible*, never
//! *guaranteed* safe.
//!
//! # Two scopes, deliberately different sizes
//!
//! There are two lists here and they are **not** interchangeable:
//!
//! - [`default_roots`] is the **disposal** scope. It answers "may this be
//!   removed?" and is consulted by the executor immediately before mutating.
//! - [`discovery_roots`] is the **read-only** scope. It answers "may this be
//!   looked at and shown to the user?" and is consulted only by read-only
//!   walkers.
//!
//! `discovery_roots` is much wider — it includes `~/Documents` and even
//! `/Applications` — because features like Large & Old Files and Space Lens
//! must be able to *see* far more of the disk than the tool may ever *touch*.
//! The rule that keeps that safe:
//!
//! > Widen what we can see. Never widen what we can dispose of — escalate
//! > per-path with explicit consent instead.
//!
//! Mechanically, discovery yields plain [`PathBuf`]s. It never produces a
//! [`crate::SafePath`], so nothing a discovery walk emits can reach the
//! executor without passing through [`crate::guard`] first — which is exactly
//! what keeps `/Applications` (on the denylist) undeletable no matter how
//! prominently a discovery walk displays it.

use std::path::{Path, PathBuf};

/// The default set of locations Swept is allowed to scan and clean.
///
/// Deliberately conservative: per-user caches, logs, Xcode derived data, and
/// the user Trash. Anything outside these requires explicit user confirmation
/// and is never scanned by default.
///
/// **This is the disposal boundary.** Adding an entry here grants unattended
/// removal rights over everything beneath it, so it is pinned by
/// [`tests::the_disposal_scope_is_pinned`] — widening it must be a deliberate
/// edit to that assertion, never a side effect of another change.
pub fn default_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Library/Caches"),
        home.join("Library/Logs"),
        home.join("Library/Developer/Xcode/DerivedData"),
        home.join(".Trash"),
    ]
}

/// Locations that read-only features may **look at**. Never a disposal scope.
///
/// Consumed by size/age walkers (Large & Old Files, Space Lens) which display
/// what they find and select nothing by default. Disposing of anything found
/// here requires an explicit per-path grant from the user — see
/// `Consent::granted` in the executor — and every such path is still run
/// through [`crate::guard`] first, so denylisted locations stay refused.
///
/// `/Applications` is included on purpose even though the denylist refuses
/// everything beneath it: the Uninstaller needs to *enumerate* app bundles in
/// order to find their leftovers elsewhere. Reading it is safe precisely
/// because this function cannot mint a [`crate::SafePath`].
pub fn discovery_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Documents"),
        home.join("Downloads"),
        home.join("Desktop"),
        home.join("Movies"),
        home.join("Music"),
        home.join("Pictures"),
        home.join("Library/Application Support"),
        PathBuf::from("/Applications"),
    ]
}

/// True if `path` is inside one of `roots` (or equal to a root).
///
/// `path` and `roots` are expected to be canonical.
pub fn is_allowed(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|r| path == r.as_path() || path.starts_with(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/Users/tester")
    }

    #[test]
    fn confines_to_roots() {
        let home = PathBuf::from("/Users/tester");
        let roots = default_roots(&home);
        assert!(is_allowed(&home.join("Library/Caches/app/x"), &roots));
        assert!(is_allowed(&home.join(".Trash/old"), &roots));
        assert!(!is_allowed(&home.join("Documents/important.txt"), &roots));
        assert!(!is_allowed(&home.join("Library/Mail/x"), &roots));
    }

    /// A canary on the disposal boundary.
    ///
    /// `default_roots` is the set of places this tool may remove files from
    /// without asking about each one. M1 introduced a second, much wider list
    /// next door; this assertion is what makes accidentally widening the wrong
    /// one a failing test rather than a silent escalation of privilege.
    #[test]
    fn the_disposal_scope_is_pinned() {
        let expected: Vec<PathBuf> = [
            "Library/Caches",
            "Library/Logs",
            "Library/Developer/Xcode/DerivedData",
            ".Trash",
        ]
        .iter()
        .map(|s| home().join(s))
        .collect();
        assert_eq!(
            default_roots(&home()),
            expected,
            "the disposal scope changed; if that is intended, edit this test deliberately"
        );
    }

    #[test]
    fn discovery_sees_the_places_disposal_may_not() {
        let d = discovery_roots(&home());
        for expected in [
            "Documents",
            "Downloads",
            "Desktop",
            "Movies",
            "Music",
            "Pictures",
            "Library/Application Support",
        ] {
            assert!(
                d.contains(&home().join(expected)),
                "discovery should be able to look at ~/{expected}"
            );
        }
        assert!(
            d.contains(&PathBuf::from("/Applications")),
            "the uninstaller needs to enumerate app bundles"
        );
    }

    /// The load-bearing one: seeing is not disposing.
    ///
    /// Every discovery root must be outside the disposal allowlist, so a path
    /// surfaced by a read-only walk is refused by the executor unless the user
    /// grants it individually.
    #[test]
    fn nothing_discovery_finds_is_disposable_by_default() {
        let disposal = default_roots(&home());
        for root in discovery_roots(&home()) {
            assert!(
                !is_allowed(&root, &disposal),
                "{} must not be disposable by default",
                root.display()
            );
            assert!(
                !is_allowed(&root.join("some/nested/file.bin"), &disposal),
                "nothing under {} may be disposable by default",
                root.display()
            );
        }
    }
}
