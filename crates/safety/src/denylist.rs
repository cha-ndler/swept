//! Protected-path denylist. Checked first; always wins over the allowlist.
//!
//! Encodes SAFETY CONTRACT item 2. Inputs are expected to be **absolute and
//! already canonicalized** (see [`crate::path_guard::guard`]).

use std::path::{Component, Path};

/// Absolute, system-wide roots that must never be modified.
const PROTECTED_ABS_ROOTS: &[&str] = &[
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/Library",
    "/Applications",
];

/// Home-relative roots that must never be modified.
const PROTECTED_HOME_SUBPATHS: &[&str] = &["Library/Keychains", "Library/Mail"];

/// Returns `Some(reason)` if `path` is protected and must never be touched.
///
/// `path` and `home` must both be absolute and canonicalized. A non-absolute
/// path is treated as protected (fail closed), as is any path containing a
/// `..` component or a `.git` component.
pub fn protection_reason(path: &Path, home: &Path) -> Option<&'static str> {
    // Fail closed on anything that isn't a clean absolute path.
    if !path.is_absolute() {
        return Some("path is not absolute");
    }

    // No `..` may survive into a checked path (defense-in-depth; `guard` also
    // rejects this before canonicalization).
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Some("path contains a parent-dir (`..`) component");
    }

    // Any component named `.git` => inside a git repository.
    if path
        .components()
        .any(|c| matches!(c, Component::Normal(s) if s == ".git"))
    {
        return Some("path is inside a .git directory");
    }

    // Filesystem root.
    if path == Path::new("/") {
        return Some("filesystem root");
    }

    // System roots (the root itself and anything beneath it).
    for root in PROTECTED_ABS_ROOTS {
        let r = Path::new(root);
        if path == r || path.starts_with(r) {
            return Some("system-protected root");
        }
    }

    // Home directory root itself (but not its allowlisted children).
    if path == home {
        return Some("home directory root");
    }

    // Protected home subpaths (keychains, mail).
    for sub in PROTECTED_HOME_SUBPATHS {
        let p = home.join(sub);
        if path == p || path.starts_with(&p) {
            return Some("protected home subpath (keychains/mail)");
        }
    }

    None
}

/// Convenience boolean wrapper around [`protection_reason`].
pub fn is_protected(path: &Path, home: &Path) -> bool {
    protection_reason(path, home).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        PathBuf::from("/Users/tester")
    }

    #[test]
    fn blocks_filesystem_root() {
        assert!(is_protected(Path::new("/"), &home()));
    }

    #[test]
    fn blocks_each_system_root() {
        for r in [
            "/System",
            "/usr",
            "/bin",
            "/sbin",
            "/Library",
            "/Applications",
        ] {
            assert!(
                is_protected(Path::new(r), &home()),
                "{r} should be protected"
            );
            let child = format!("{r}/something/deep");
            assert!(
                is_protected(Path::new(&child), &home()),
                "{child} should be protected"
            );
        }
    }

    #[test]
    fn blocks_home_root_but_not_children() {
        assert!(is_protected(&home(), &home()));
        assert!(!is_protected(&home().join("Library/Caches/foo"), &home()));
    }

    #[test]
    fn blocks_keychains_and_mail() {
        assert!(is_protected(
            &home().join("Library/Keychains/login.keychain-db"),
            &home()
        ));
        assert!(is_protected(
            &home().join("Library/Mail/V10/inbox"),
            &home()
        ));
    }

    #[test]
    fn blocks_dot_git_anywhere() {
        assert!(is_protected(
            &home().join("Projects/app/.git/config"),
            &home()
        ));
        assert!(is_protected(&home().join("Projects/app/.git"), &home()));
    }

    #[test]
    fn blocks_parent_dir_and_relative() {
        assert!(is_protected(Path::new("/Users/tester/../root/x"), &home()));
        assert!(is_protected(Path::new("relative/path"), &home()));
    }

    #[test]
    fn allows_a_normal_cache_file() {
        assert_eq!(
            protection_reason(&home().join("Library/Caches/app/blob"), &home()),
            None
        );
    }
}
