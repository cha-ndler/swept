//! Large & Old Files — a read-only walk of the *discovery* scope.
//!
//! This is the first thing in the codebase that looks outside
//! `allowlist::default_roots`, and it is the reason the discovery/disposal
//! split exists:
//!
//! > Widen what we can see. Never widen what we can dispose of — escalate
//! > per-path with explicit consent instead.
//!
//! So this module **cannot authorize anything**. It yields plain [`PathBuf`]s
//! and never constructs a [`safety::SafePath`]; turning one of these results
//! into a disposal takes a separate, explicit, per-path grant that runs through
//! [`safety::guard`] at that point. Nothing here is pre-selected, and nothing
//! here is included in a default clean.
//!
//! # Honest, not fail-closed
//!
//! Unlike [`safety::guard_dir`], which refuses a whole tree if it cannot read
//! part of it, this walk *reports* what it could not see and carries on. The
//! difference is what the two are for: `guard_dir` vouches for a destruction
//! and must be certain, while this only decides what to show a human. Refusing
//! to display anything because one directory was TCC-gated would make the
//! feature useless on a stock Mac.
//!
//! What it must never do is under-report *silently* — hence
//! [`LargeOldReport::skipped_unreadable`] and [`LargeOldReport::truncated`],
//! which the UI is expected to surface.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use safety::allowlist;
use safety::denylist::is_protected;
use walkdir::WalkDir;

/// Default size floor: 100 MiB. Small enough to find real space, large enough
/// that the list is short enough for a human to actually read.
pub const DEFAULT_MIN_SIZE: u64 = 100 * 1024 * 1024;

/// Default number of rows kept. The report still reports the true totals, so
/// keeping fewer never hides how much matched.
pub const DEFAULT_MAX_RESULTS: usize = 500;

/// Default bound on how many entries the walk will look at before stopping and
/// saying so. A stock home is ~165k files; half a million is generous headroom
/// while still guaranteeing the walk ends.
pub const DEFAULT_MAX_EXAMINED: usize = 500_000;

pub struct LargeOldConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// Roots to look in. Defaults to [`allowlist::discovery_roots`].
    pub roots: Vec<PathBuf>,
    /// Only report files at least this many bytes.
    pub min_size: u64,
    /// Only report files whose mtime is at least this far in the past.
    pub min_age: Option<Duration>,
    /// Keep at most this many rows (largest first).
    pub max_results: usize,
    /// Stop walking after this many entries and set `truncated`.
    pub max_examined: usize,
}

impl LargeOldConfig {
    pub fn new(home: PathBuf) -> Self {
        let roots = allowlist::discovery_roots(&home);
        Self {
            home,
            roots,
            min_size: DEFAULT_MIN_SIZE,
            min_age: None,
            max_results: DEFAULT_MAX_RESULTS,
            max_examined: DEFAULT_MAX_EXAMINED,
        }
    }

    pub fn min_size(mut self, bytes: u64) -> Self {
        self.min_size = bytes;
        self
    }

    pub fn older_than(mut self, age: Duration) -> Self {
        self.min_age = Some(age);
        self
    }
}

/// One file the walk found. Deliberately *not* a `SafePath`: this is something
/// to show a human, not something anyone may act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub size_bytes: u64,
    /// Modification time in epoch milliseconds, if it could be read.
    pub modified_ms: Option<u64>,
}

// Ordered by size, then path — the path keeps it deterministic when sizes tie,
// which matters because the heap below discards the minimum.
impl Ord for Found {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.size_bytes
            .cmp(&other.size_bytes)
            .then_with(|| self.path.cmp(&other.path))
    }
}

impl PartialOrd for Found {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
pub struct LargeOldReport {
    /// The largest matches, biggest first, capped at `max_results`.
    pub items: Vec<Found>,
    /// How many files matched in total — may exceed `items.len()`.
    pub matched: usize,
    /// Total size of everything that matched, not just what is listed.
    pub matched_bytes: u64,
    /// Entries looked at, including ones that were filtered out.
    pub examined: usize,
    /// True if the walk stopped at `max_examined` before finishing.
    pub truncated: bool,
    /// Directories that could not be read — almost always TCC, occasionally
    /// permissions. Surfaced so the UI can say the figure is a floor.
    pub skipped_unreadable: usize,
    /// Files skipped because more than one name points at their data.
    ///
    /// Removing one name of a hard-linked file reclaims nothing until the last
    /// one goes, so listing it would promise space that will not appear. Same
    /// reasoning as excluding symlinks — counted rather than dropped silently.
    pub skipped_hardlinked: usize,
}

impl LargeOldReport {
    /// True if the report describes less than the whole disk, for any reason.
    /// The UI must say so when this is set.
    pub fn is_partial(&self) -> bool {
        self.truncated
            || self.skipped_unreadable > 0
            || self.skipped_hardlinked > 0
            || self.matched > self.items.len()
    }
}

/// Walk the discovery roots and report the largest (optionally oldest) files.
///
/// Never mutates anything, never follows a symlink, and never returns a path
/// the denylist objects to.
pub fn find(cfg: &LargeOldConfig) -> LargeOldReport {
    let mut report = LargeOldReport::default();
    // Min-heap: the smallest kept item is always on top, so exceeding the cap
    // discards the smallest rather than the most recent.
    let mut keep: BinaryHeap<Reverse<Found>> = BinaryHeap::new();
    let now = SystemTime::now();

    for root in &cfg.roots {
        // Canonicalize the root itself, for two reasons — and *not* the obvious
        // one. A symlinked root is descended either way (`read_dir` follows it,
        // and walkdir stats the root with `fs::metadata`), so this is not what
        // makes iCloud-Drive-style `~/Documents` work.
        //
        // What it actually buys:
        //   1. `is_protected` below is a component-wise check on the path as
        //      given, so an uncanonicalized root named `Downloads` would sail
        //      past it while resolving somewhere the denylist forbids.
        //   2. Every emitted path is then canonical, which the disposal path
        //      relies on: it refuses any selection whose spelling is not
        //      already its own canonical form, and that is what stops a symlink
        //      swapped in after the walk from redirecting a grant.
        let Ok(root) = std::fs::canonicalize(root) else {
            continue; // Missing root: nothing to report, not an error.
        };
        if is_protected(&root, &cfg.home) {
            continue;
        }

        let walk = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            // Prune protected subtrees rather than filtering their files one by
            // one: this is what drops `/Applications`, every `.git` working
            // tree, and keychains/mail in a single decision at the top of the
            // subtree instead of at every leaf. It also means the walk never
            // spends time inside a tree whose contents could never be granted.
            .filter_entry(|e| !is_protected(e.path(), &cfg.home));

        for entry in walk {
            if report.examined >= cfg.max_examined {
                report.truncated = true;
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    // Unreadable directory (usually TCC). Recorded, not fatal —
                    // see the module docs on why this differs from `guard_dir`.
                    report.skipped_unreadable += 1;
                    continue;
                }
            };
            report.examined += 1;

            // Regular files only. A symlink is not "large" — the bytes belong
            // to its target, and removing the link reclaims none of them.
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                report.skipped_unreadable += 1;
                continue;
            };
            if meta.len() < cfg.min_size {
                continue;
            }
            // A hard-linked file's bytes are not reclaimed by removing one of
            // its names. The module already excludes symlinks for exactly this
            // reason; `nlink > 1` is the same situation with the link pointing
            // the other way.
            if meta.nlink() > 1 {
                report.skipped_hardlinked += 1;
                continue;
            }
            if let Some(min_age) = cfg.min_age {
                if !is_at_least(&meta, min_age, now) {
                    continue;
                }
            }

            report.matched += 1;
            report.matched_bytes = report.matched_bytes.saturating_add(meta.len());

            let found = Found {
                path: entry.path().to_path_buf(),
                size_bytes: meta.len(),
                modified_ms: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64),
            };
            keep.push(Reverse(found));
            if keep.len() > cfg.max_results {
                keep.pop();
            }
        }
    }

    // Largest first. `into_sorted_vec` sorts ascending by the heap's own
    // ordering, which here is `Reverse<Found>` — so ascending-by-Reverse is
    // already descending-by-size. Reversing again would list the smallest
    // first, which is the one order this feature must not use.
    report.items = keep.into_sorted_vec().into_iter().map(|r| r.0).collect();
    report
}

/// True if the file's mtime is at least `min_age` before `now`. Fail-safe: an
/// unreadable or future mtime reports false, so it is simply not listed.
fn is_at_least(meta: &std::fs::Metadata, min_age: Duration, now: SystemTime) -> bool {
    match meta.modified() {
        Ok(mtime) => now
            .duration_since(mtime)
            .map(|age| age >= min_age)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Convenience for callers that only have a `&Path`.
///
/// Canonicalizes `home` first. `denylist::protection_reason` compares against
/// it component-wise, so a non-canonical home silently disables the
/// keychains/mail and home-root rules for the entire walk — every real caller
/// happens to pass a canonical path today, which is precisely the kind of
/// accident that stops being true later.
pub fn find_in(home: &Path, min_size: u64, min_age: Option<Duration>) -> LargeOldReport {
    let home = safety::canonical_home(home).unwrap_or_else(|_| home.to_path_buf());
    let mut cfg = LargeOldConfig::new(home).min_size(min_size);
    cfg.min_age = min_age;
    find(&cfg)
}
