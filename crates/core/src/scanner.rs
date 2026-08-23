//! Discover cleanup candidates inside allowlisted locations.
//!
//! The scanner is read-only. Every file it considers is run through the safety
//! [`guard`] (which canonicalizes and denylist-checks) and then verified to lie
//! within the scoped allowlist. Anything that fails either check is counted in
//! `skipped_protected` and dropped from the plan.

use std::fs::Metadata;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use safety::{allowlist, guard};
use walkdir::WalkDir;

use crate::plan::{Disposal, Plan, PlannedAction};

pub struct ScanConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// Canonical roots to scan. Must themselves be within the allowlist.
    pub roots: Vec<PathBuf>,
    /// Only plan files at least this old (by mtime). `None` = no age filter.
    ///
    /// A safety/quality control: recently-touched caches are often still in
    /// use, so by default a user can restrict cleanup to genuinely stale files.
    pub min_age: Option<Duration>,
    /// Only plan files at least this many bytes. `None` = no size filter.
    ///
    /// Powers the "large files finder": restrict cleanup to the items actually
    /// worth reclaiming space from.
    pub min_size: Option<u64>,
}

impl ScanConfig {
    /// A config that scans the default allowlist roots for `home`.
    pub fn with_default_roots(home: PathBuf) -> Self {
        let roots = allowlist::default_roots(&home);
        Self {
            home,
            roots,
            min_age: None,
            min_size: None,
        }
    }

    /// Restrict the plan to files whose mtime is at least `min_age` in the past.
    pub fn older_than(mut self, min_age: Duration) -> Self {
        self.min_age = Some(min_age);
        self
    }

    /// Restrict the plan to files of at least `min_size` bytes.
    pub fn min_size(mut self, min_size: u64) -> Self {
        self.min_size = Some(min_size);
        self
    }
}

/// How far a scan has got. Cumulative and monotonically non-decreasing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Progress {
    /// Files looked at so far, including ones that were filtered out.
    pub examined: usize,
    /// Files added to the plan so far.
    pub planned: usize,
    /// Bytes added to the plan so far.
    pub bytes: u64,
}

/// Report progress at most every this many files. A scan of a real home looks
/// at ~165k files; a callback per file would swamp the IPC channel and dominate
/// the scan's own cost, so updates are batched.
const PROGRESS_EVERY: usize = 2_000;

/// Walk the configured roots and build a [`Plan`]. Never mutates anything.
pub fn scan(cfg: &ScanConfig) -> Plan {
    scan_with_progress(cfg, &mut |_| {})
}

/// [`scan`], reporting progress as it goes.
///
/// `on_progress` is called periodically and once more at the end, so the final
/// call always describes the returned plan. Planning is unaffected: this and
/// [`scan`] produce identical plans for identical inputs.
pub fn scan_with_progress(cfg: &ScanConfig, on_progress: &mut dyn FnMut(Progress)) -> Plan {
    let mut plan = Plan::default();
    let mut progress = Progress::default();
    let allowed = allowlist::default_roots(&cfg.home);
    let now = SystemTime::now();

    for root in &cfg.roots {
        if !root.exists() {
            continue;
        }
        // Do not follow symlinks while walking; `guard` will canonicalize each
        // candidate and reject anything that escapes the allowlist.
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            progress.examined += 1;
            if progress.examined % PROGRESS_EVERY == 0 {
                on_progress(progress);
            }
            let safe = match guard(entry.path(), &cfg.home) {
                Ok(s) => s,
                Err(_) => {
                    plan.skipped_protected += 1;
                    continue;
                }
            };
            if !allowlist::is_allowed(safe.as_path(), &allowed) {
                plan.skipped_protected += 1;
                continue;
            }
            // A file we cannot stat is one we cannot assess — never plan it.
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            // Age filter: skip files that aren't old enough to be considered junk.
            if let Some(min_age) = cfg.min_age {
                if !is_at_least(&meta, min_age, now) {
                    continue;
                }
            }
            // Size filter: skip files below the threshold (large-files finder).
            if let Some(min_size) = cfg.min_size {
                if meta.len() < min_size {
                    continue;
                }
            }
            let category = crate::categories::classify(safe.as_path(), &cfg.home)
                .map(|c| c.id.to_string())
                .unwrap_or_else(|| "other".to_string());
            progress.planned += 1;
            progress.bytes += meta.len();
            plan.actions.push(PlannedAction {
                path: safe,
                size_bytes: meta.len(),
                disposal: Disposal::Trash,
                category,
            });
        }
    }

    // Always finish with an update describing the plan actually returned, so a
    // caller never ends on a stale intermediate count.
    on_progress(progress);
    plan
}

/// True if the file's mtime is at least `min_age` before `now`. Fail-safe: if the
/// mtime is unreadable or in the future, returns false (i.e. do not clean it).
fn is_at_least(meta: &Metadata, min_age: Duration, now: SystemTime) -> bool {
    match meta.modified() {
        Ok(mtime) => now
            .duration_since(mtime)
            .map(|age| age >= min_age)
            .unwrap_or(false),
        Err(_) => false,
    }
}
