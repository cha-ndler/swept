//! Trust kernel for mac-cleaner.
//!
//! This crate is the single chokepoint that every destructive operation must
//! pass through. It performs three jobs, in this order of authority:
//!
//! 1. [`denylist`] — refuse anything under a protected system/user path.
//!    Checked *first* and always wins.
//! 2. [`path_guard`] — canonicalize (resolving symlinks), reject `..`
//!    traversal, then re-run the denylist on the resolved path. The only way
//!    to obtain a [`SafePath`] is through [`guard`].
//! 3. [`allowlist`] — confine cleanup to known-safe locations.
//!
//! Nothing in this crate deletes, moves, or truncates files. It only decides
//! whether a path is *eligible* to be acted on. The executor (in the `core`
//! crate) re-runs [`guard`] immediately before every mutation as a TOCTOU
//! defense.

pub mod allowlist;
pub mod denylist;
pub mod path_guard;

pub use path_guard::{guard, GuardError, SafePath};

use std::io;
use std::path::{Path, PathBuf};

/// Canonicalize a home directory for use in protection checks.
///
/// All denylist comparisons assume canonical paths (on macOS `/Users/...` and
/// `/var/folders/...` are themselves symlinked), so the home directory passed
/// to [`guard`] must be canonicalized the same way the targets are.
pub fn canonical_home(home: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(home)
}
