//! Discover cleanup candidates inside allowlisted locations.
//!
//! The scanner is read-only. Every file it considers is run through the safety
//! [`guard`] (which canonicalizes and denylist-checks) and then verified to lie
//! within the scoped allowlist. Anything that fails either check is counted in
//! `skipped_protected` and dropped from the plan.

use std::path::PathBuf;

use safety::{allowlist, guard};
use walkdir::WalkDir;

use crate::plan::{Disposal, Plan, PlannedAction};

pub struct ScanConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// Canonical roots to scan. Must themselves be within the allowlist.
    pub roots: Vec<PathBuf>,
}

impl ScanConfig {
    /// A config that scans the default allowlist roots for `home`.
    pub fn with_default_roots(home: PathBuf) -> Self {
        let roots = allowlist::default_roots(&home);
        Self { home, roots }
    }
}

/// Walk the configured roots and build a [`Plan`]. Never mutates anything.
pub fn scan(cfg: &ScanConfig) -> Plan {
    let mut plan = Plan::default();
    let allowed = allowlist::default_roots(&cfg.home);

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
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let category = allowlist::category_for(safe.as_path(), &cfg.home);
            plan.actions.push(PlannedAction {
                path: safe,
                size_bytes: size,
                disposal: Disposal::Trash,
                category,
            });
        }
    }

    plan
}
