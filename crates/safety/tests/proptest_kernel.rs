//! Property-based tests for the safety kernel's load-bearing invariants.
//!
//! These fuzz the *pure* decision functions (`denylist::is_protected`,
//! `allowlist::is_allowed`) over thousands of generated paths. The point is to
//! prove the invariants hold for inputs we never thought to write by hand:
//! anything under a protected root is refused; `.git`/`..` is always refused;
//! and a clean file inside an allowlisted root is allowed and not protected
//! ("allowlist ⊄ denylist" for safe paths).

use std::path::PathBuf;

use proptest::prelude::*;
use safety::allowlist::{default_roots, discovery_roots, is_allowed};
use safety::denylist::is_protected;

fn home() -> PathBuf {
    PathBuf::from("/Users/tester")
}

/// Path segments that are always "safe": non-empty, lowercase alphanumerics,
/// so they can never be `.git`, `..`, or empty.
fn safe_segments(max: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z0-9]{1,8}", 1..max)
}

proptest! {
    /// Anything at or beneath a system-protected root is protected, no matter
    /// what arbitrary tail is appended.
    #[test]
    fn system_roots_are_always_protected(
        tail in prop::collection::vec("[a-zA-Z0-9_.\\-]{1,10}", 0..6)
    ) {
        for root in ["/System", "/usr", "/bin", "/sbin", "/Library", "/Applications"] {
            let mut p = PathBuf::from(root);
            for seg in &tail {
                p.push(seg);
            }
            prop_assert!(is_protected(&p, &home()), "{} must be protected", p.display());
        }
    }

    /// Any path containing a `.git` component is protected, wherever it sits.
    #[test]
    fn dot_git_anywhere_is_protected(
        pre in prop::collection::vec("[a-z]{1,6}", 0..4),
        post in prop::collection::vec("[a-z]{1,6}", 0..4),
    ) {
        let mut p = home();
        for s in &pre { p.push(s); }
        p.push(".git");
        for s in &post { p.push(s); }
        prop_assert!(is_protected(&p, &home()), "{} must be protected", p.display());
    }

    /// Any path containing a `..` component is protected (fail-closed).
    #[test]
    fn parent_dir_traversal_is_protected(
        pre in prop::collection::vec("[a-z]{1,6}", 0..4),
        post in prop::collection::vec("[a-z]{1,6}", 0..4),
    ) {
        let mut p = home();
        for s in &pre { p.push(s); }
        p.push("..");
        for s in &post { p.push(s); }
        prop_assert!(is_protected(&p, &home()), "{} must be protected", p.display());
    }

    /// A clean file inside an allowlisted root is BOTH allowed AND not protected.
    /// This is the "allowlist is not a subset of the denylist" guarantee for the
    /// paths the tool actually acts on.
    #[test]
    fn safe_paths_in_allowed_roots_are_clean(
        root_idx in 0usize..default_roots(&home()).len(),
        segs in safe_segments(6),
    ) {
        let roots = default_roots(&home());
        let mut p = roots[root_idx].clone();
        for s in &segs { p.push(s); }
        prop_assert!(!is_protected(&p, &home()), "{} must NOT be protected", p.display());
        prop_assert!(is_allowed(&p, &roots), "{} must be allowed", p.display());
    }

    /// Conversely, a safe path that is NOT under any allowlist root is rejected
    /// by the allowlist (cleanup is confined).
    #[test]
    fn safe_paths_outside_roots_are_not_allowed(segs in safe_segments(5)) {
        // Documents is deliberately not an allowlist root.
        let mut p = home().join("Documents");
        for s in &segs { p.push(s); }
        prop_assert!(!is_allowed(&p, &default_roots(&home())), "{} must not be allowed", p.display());
    }

    /// No ANCESTOR of a protected location is ever unprotected, all the way up
    /// to the filesystem root.
    ///
    /// `~/Library` is the motivating case: component-wise `starts_with` means
    /// it never matches the absolute `/Library` root, so before this invariant
    /// existed the guard accepted it and only the allowlist stood in the way.
    /// Any directory that would take Keychains or Mail down with it must be
    /// refused by the denylist itself.
    #[test]
    fn ancestors_of_protected_locations_are_protected(user in "[a-z][a-z0-9]{0,7}") {
        let home = PathBuf::from("/Users").join(&user);
        for sub in ["Library/Keychains", "Library/Mail"] {
            let full = home.join(sub);
            // `ancestors()` yields the path itself, then every parent up to "/".
            for ancestor in full.ancestors() {
                prop_assert!(
                    is_protected(ancestor, &home),
                    "{} is an ancestor of {} and must be protected",
                    ancestor.display(), full.display()
                );
            }
        }
    }

    /// Seeing is not disposing: **nothing** reachable from a discovery root is
    /// disposable by the default allowlist, whatever tail is appended.
    ///
    /// This is the M1 spine as a property. Read-only walkers (Large & Old
    /// Files, Space Lens, the Uninstaller's bundle enumeration) range over
    /// `discovery_roots`, which reaches `~/Documents` and `/Applications`. The
    /// only thing standing between "we can see it" and "we can remove it" is
    /// this disjointness plus an explicit per-path grant — so if the two lists
    /// ever overlap, a whole category of files silently becomes cleanable
    /// without anyone having chosen it.
    #[test]
    fn discovery_scope_is_disjoint_from_the_disposal_scope(
        tail in prop::collection::vec("[a-zA-Z0-9_.\\-]{1,10}", 0..6)
    ) {
        let disposal = default_roots(&home());
        for root in discovery_roots(&home()) {
            let mut p = root.clone();
            for seg in &tail {
                p.push(seg);
            }
            prop_assert!(
                !is_allowed(&p, &disposal),
                "{} is discoverable and must NOT be disposable by default",
                p.display()
            );
        }
    }

    /// ...and the rule must not leak downwards: the allowlisted siblings that
    /// live under the same parent stay cleanable.
    #[test]
    fn allowlisted_siblings_under_library_stay_clean(segs in safe_segments(4)) {
        for root in ["Library/Caches", "Library/Logs"] {
            let mut p = home().join(root);
            for s in &segs { p.push(s); }
            prop_assert!(!is_protected(&p, &home()), "{} must stay cleanable", p.display());
        }
    }
}
