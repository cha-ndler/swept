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

/// Human-readable category for a path, based on which allowlist root contains
/// it. Used for grouping in the plan/preview.
pub fn category_for(path: &Path, home: &Path) -> String {
    if path.starts_with(home.join("Library/Caches")) {
        "cache".to_string()
    } else if path.starts_with(home.join("Library/Logs")) {
        "log".to_string()
    } else if path.starts_with(home.join("Library/Developer/Xcode/DerivedData")) {
        "xcode-derived-data".to_string()
    } else if path.starts_with(home.join(".Trash")) {
        "trash".to_string()
    } else {
        "other".to_string()
    }
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

    #[test]
    fn categorizes() {
        let home = PathBuf::from("/Users/tester");
        assert_eq!(category_for(&home.join("Library/Caches/a"), &home), "cache");
        assert_eq!(category_for(&home.join("Library/Logs/a"), &home), "log");
    }
}
