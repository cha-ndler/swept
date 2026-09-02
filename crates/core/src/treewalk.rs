//! One tree measurement, shared by every module that can offer a directory.
//!
//! This is not a general-purpose size walk — [`crate::spacelens`] is that, and
//! it answers a different question. This one measures a tree **that a human may
//! be about to be offered**, so it does two jobs at once:
//!
//! 1. it sums the tree the way a disposal would experience it, and
//! 2. it decides, in advance, whether [`safety::guard_dir`] is *certain* to
//!    refuse the tree — so a row that could only ever fail is shown and never
//!    offered, rather than presented as a choice that dies at the sheet.
//!
//! It exists as its own module because the second job is a safety predicate,
//! and a second copy of it would eventually drift from this one. Drift here is
//! not symmetric: a copy that is *stricter* than `guard_dir` only hides rows,
//! while a copy that is *laxer* offers a checkbox that the executor will refuse
//! — and the whole point of the flag is that the refusal is known first.
//! M4 (the Uninstaller) and M5 (Privacy) both need it, so there is one.
//!
//! # What it never does
//!
//! Never follows a symlink, never descends into a protected subtree, never
//! opens a file, and never mutates anything. It counts **every name**,
//! including each name of a hard-linked file — the opposite of `spacelens`,
//! and for the reason `uninstall` gives: a disposal unlinks names.

use std::path::{Path, PathBuf};

use safety::denylist::is_protected;
use safety::DirLimits;

/// Hard recursion bound for a per-row size walk.
///
/// Deliberately equal to `safety::DirLimits::default().max_depth`: a row a scan
/// was willing to size is a row `guard_dir` could plausibly be asked to vouch
/// for later, and the two disagreeing about how deep is reasonable would be a
/// surprise at exactly the wrong moment. Symlinks are never followed, so only a
/// genuinely deep tree reaches it.
pub const MAX_ROW_DEPTH: usize = 32;

/// Prefix for the withheld reason of a row `guard_dir` is certain to refuse.
pub const UNDISPOSABLE_REASON: &str = "this tool cannot remove it: ";

/// What a measurement is judged against.
///
/// `dir_limits` is the bound *disposal* will apply through `guard_dir`. It is a
/// field rather than a constant only so a fixture can reach it — 50,000 files
/// is not a tempdir test — and every caller is expected to pin it equal to
/// [`DirLimits::default`], because if the two diverge the `undisposable` flag
/// lies in the dangerous direction.
#[derive(Debug, Clone)]
pub struct Bounds {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// The bounds `guard_dir` will apply at disposal time.
    pub dir_limits: DirLimits,
    /// Entry budget for the whole run, shared across every row.
    pub max_examined: usize,
}

/// What a per-row size walk found, beyond the figures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub size_bytes: u64,
    pub file_count: u64,
    /// The figure is a floor: something could not be seen.
    pub size_is_floor: bool,
    /// Why `guard_dir` is certain to refuse this tree, if it is.
    pub undisposable: Option<&'static str>,
}

/// What the walk saw on the way, separate from what it summed.
#[derive(Default)]
struct Seen {
    floor: bool,
    /// Every name beneath the root — files, directories, symlinks — which is
    /// what `guard_dir` counts against `max_entries`. Distinct from
    /// `file_count`, which excludes directories because a directory is not a
    /// file the user will lose.
    names: u64,
    protected: bool,
    too_deep: bool,
}

/// Measure `path` as a disposal would experience it.
///
/// `examined` is a run-wide budget the caller carries across rows, so one
/// pathological cache cannot starve the rest of the scan silently — exhausting
/// it sets `size_is_floor`.
pub fn measure(path: &Path, bounds: &Bounds, examined: &mut usize) -> Measured {
    fn walk(
        path: &Path,
        depth: usize,
        bounds: &Bounds,
        examined: &mut usize,
        seen: &mut Seen,
    ) -> (u64, u64) {
        if depth > MAX_ROW_DEPTH {
            // `guard_dir` refuses a directory this deep. A *file* this deep
            // would have been allowed — its parent is one level up — so this
            // over-refuses by at most one level, in the safe direction.
            seen.floor = true;
            seen.too_deep = true;
            return (0, 0);
        }
        if *examined >= bounds.max_examined {
            seen.floor = true;
            return (0, 0);
        }
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            seen.floor = true;
            return (0, 0);
        };
        if meta.file_type().is_symlink() {
            // Owns none of its target's bytes, and is one name to unlink.
            return (0, 1);
        }
        if meta.is_file() {
            return (meta.len(), 1);
        }
        if !meta.is_dir() {
            return (0, 0);
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            seen.floor = true;
            return (0, 0);
        };
        let mut bytes = 0u64;
        let mut count = 0u64;
        for entry in entries {
            *examined += 1;
            let Ok(entry) = entry else {
                seen.floor = true;
                continue;
            };
            seen.names = seen.names.saturating_add(1);
            let child = entry.path();
            // A protected subtree inside the row — a vendored `.git`, most
            // plausibly — is not measured, because it is not disposable. And it
            // makes the whole row undisposable: `guard_dir` refuses the entire
            // tree, so the row must not be offered.
            if is_protected(&child, &bounds.home) {
                seen.floor = true;
                seen.protected = true;
                continue;
            }
            let (b, c) = walk(&child, depth + 1, bounds, examined, seen);
            bytes = bytes.saturating_add(b);
            count = count.saturating_add(c);
        }
        (bytes, count)
    }

    let mut seen = Seen::default();
    let (size_bytes, file_count) = walk(path, 0, bounds, examined, &mut seen);
    let limits = bounds.dir_limits;
    let undisposable = if seen.protected {
        Some("the tree contains a protected path (a .git checkout, most likely)")
    } else if seen.too_deep {
        Some("the tree is deeper than a disposal may reach")
    } else if seen.names > limits.max_entries as u64 {
        Some("the tree holds more entries than a disposal may remove at once")
    } else if size_bytes > limits.max_bytes {
        Some("the tree is larger than a disposal may remove at once")
    } else {
        None
    };
    Measured {
        size_bytes,
        file_count,
        size_is_floor: seen.floor,
        undisposable,
    }
}

/// A measurement that could not be completed says nothing trustworthy about
/// what the tree holds.
pub const INCOMPLETE_MEASURE: &str = "the tree could not be measured completely, so what it \
     holds cannot be stated truthfully";

/// Whether a measured row may be offered at all.
///
/// Two reasons to withhold, and the second is easy to miss. A tree `guard_dir`
/// is certain to refuse is shown and never offered — that is what
/// `undisposable` is for. But a tree whose *measurement* was cut short is just
/// as unofferable, for a different reason: its `size_bytes` and `file_count`
/// are floors, and offering it would put a figure in front of a human that is
/// not the figure they would be acting on. It also defeats the `max_bytes` arm
/// of `undisposable` — an under-summed tree cannot exceed a threshold — so a
/// row `guard_dir` will refuse could otherwise be offered, which is exactly the
/// drift this module exists to prevent. Contract item 5 wants the count and
/// size shown for a recursive removal to be true, not conservative.
pub fn offer(m: &Measured) -> (bool, Option<String>) {
    if let Some(why) = m.undisposable {
        return (false, Some(format!("{UNDISPOSABLE_REASON}{why}")));
    }
    if m.size_is_floor {
        return (
            false,
            Some(format!("{UNDISPOSABLE_REASON}{INCOMPLETE_MEASURE}")),
        );
    }
    (true, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole reason the `undisposable` flag can be trusted.
    ///
    /// If this walk were willing to go deeper than `guard_dir` will, a row
    /// would be offered that disposal is certain to refuse.
    #[test]
    fn the_row_depth_bound_matches_what_disposal_will_allow() {
        assert_eq!(MAX_ROW_DEPTH, DirLimits::default().max_depth);
    }
}
