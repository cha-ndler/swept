//! Scoped allowlist. Cleanup is confined to these known-safe locations.
//!
//! Encodes SAFETY CONTRACT item 3. The denylist is always checked first and
//! overrides the allowlist; membership here only makes a path *eligible*, never
//! *guaranteed* safe.

use std::path::{Path, PathBuf};

/// The default set of locations mac-cleaner is allowed to scan and clean.
///
/// Deliberately conservative: per-user caches, logs, Xcode derived data, and
/// the user Trash. Anything outside these requires explicit user confirmation
/// and is never scanned by default.
pub fn default_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Library/Caches"),
        home.join("Library/Logs"),
        home.join("Library/Developer/Xcode/DerivedData"),
        home.join(".Trash"),
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

    #[test]
    fn confines_to_roots() {
        let home = PathBuf::from("/Users/tester");
        let roots = default_roots(&home);
        assert!(is_allowed(&home.join("Library/Caches/app/x"), &roots));
        assert!(is_allowed(&home.join(".Trash/old"), &roots));
        assert!(!is_allowed(&home.join("Documents/important.txt"), &roots));
        assert!(!is_allowed(&home.join("Library/Mail/x"), &roots));
    }
}
