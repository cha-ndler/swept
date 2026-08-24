//! Protected-path denylist. Checked first; always wins over the allowlist.
//!
//! Encodes SAFETY CONTRACT item 2. Inputs are expected to be **absolute and
//! already canonicalized** (see [`crate::path_guard::guard`]).

use std::path::{Component, Path};

/// Case-insensitive component equality.
fn component_eq_ci(a: &Component<'_>, b: &Component<'_>) -> bool {
    a.as_os_str()
        .as_encoded_bytes()
        .eq_ignore_ascii_case(b.as_os_str().as_encoded_bytes())
}

/// `Path::starts_with`, but ignoring ASCII case.
///
/// macOS volumes are case-insensitive by default, and `fs::canonicalize` does
/// **not** case-normalize: it returns the spelling it was given. So `/USR/bin`
/// and `~/Library/mail` survive canonicalization unchanged while naming exactly
/// the same files as `/usr/bin` and `~/Library/Mail`. A byte-exact denylist
/// would wave them through. Comparing case-insensitively closes that gap; it
/// can only ever protect *more* paths, never fewer.
///
/// Still component-wise, so `/usrlocal` does not match `/usr` and `.Trashes`
/// does not match `.Trash`.
///
/// Folds **ASCII only**. Every current denylist entry is ASCII, so this covers
/// them completely; a non-ASCII home spelled in a different case (`/Users/JOSÉ`
/// vs `/Users/josé`) would not be recognized as the same path. That is no worse
/// than the byte-exact comparison this replaced, but it is not full Unicode
/// case folding — worth revisiting if a non-ASCII entry is ever added.
fn starts_with_ci(path: &Path, prefix: &Path) -> bool {
    let mut got = path.components();
    for want in prefix.components() {
        match got.next() {
            Some(have) if component_eq_ci(&have, &want) => {}
            _ => return false,
        }
    }
    true
}

/// Case-insensitive path equality (component-wise).
fn eq_ci(a: &Path, b: &Path) -> bool {
    let mut ai = a.components();
    let mut bi = b.components();
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if component_eq_ci(&x, &y) => continue,
            _ => return false,
        }
    }
}

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
    //
    // Case-insensitively, for the same reason every other rule in this file is:
    // macOS volumes are case-insensitive by default, so a repository whose
    // directory is spelled `.GIT` is a working repository that `git` will
    // happily use. A byte-exact check waved those through — and since
    // `dir_guard` turns one missed `.git` into a recursive removal of the tree
    // around it, that was the difference between refusing a checkout and
    // taking someone's uncommitted work with a cache.
    if path.components().any(
        |c| matches!(c, Component::Normal(s) if s.as_encoded_bytes().eq_ignore_ascii_case(b".git")),
    ) {
        return Some("path is inside a .git directory");
    }

    // Filesystem root.
    if path == Path::new("/") {
        return Some("filesystem root");
    }

    // System roots (the root itself and anything beneath it).
    for root in PROTECTED_ABS_ROOTS {
        let r = Path::new(root);
        if starts_with_ci(path, r) {
            return Some("system-protected root");
        }
    }

    // Home directory root itself (but not its allowlisted children).
    if eq_ci(path, home) {
        return Some("home directory root");
    }

    // Protected home subpaths (keychains, mail) — the location itself and
    // everything beneath it.
    for sub in PROTECTED_HOME_SUBPATHS {
        if starts_with_ci(path, &home.join(sub)) {
            return Some("protected home subpath (keychains/mail)");
        }
    }

    // ...and any directory that CONTAINS one of them.
    //
    // `Path::starts_with` is component-wise, so `~/Library` never matched the
    // absolute `/Library` entry above: it is a different path. Without this
    // rule `guard("~/Library")` succeeds, and only `allowlist::is_allowed`
    // keeps it out of reach — which is the wrong layer to depend on, because
    // the allowlist is a scope decision while the denylist is a safety one.
    // Removing such a directory recursively would take Keychains and Mail with
    // it, so refuse the ancestor itself.
    //
    // Exact-ancestor semantics only: this does not propagate downwards, so the
    // allowlisted siblings (`Library/Caches`, `Library/Logs`, ...) stay
    // cleanable.
    for sub in PROTECTED_HOME_SUBPATHS {
        if starts_with_ci(&home.join(sub), path) {
            return Some("directory contains a protected location");
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
    fn blocks_a_directory_that_contains_keychains_or_mail() {
        // `~/Library` is an ANCESTOR of two protected locations. Component-wise
        // `starts_with` means it never matched the absolute "/Library" entry, so
        // it used to pass the guard: only the allowlist kept it out of reach.
        // Disposing of it recursively would take Keychains and Mail with it, so
        // the denylist — not the allowlist — has to be the thing that says no.
        assert!(is_protected(&home().join("Library"), &home()));
    }

    #[test]
    fn still_allows_the_allowlisted_children_of_library() {
        // The ancestor rule must not leak downwards: these are the paths the
        // tool exists to clean.
        for p in [
            "Library/Caches/app/blob",
            "Library/Logs/session.log",
            "Library/Developer/Xcode/DerivedData/App-abc/Build",
        ] {
            assert_eq!(
                protection_reason(&home().join(p), &home()),
                None,
                "{p} must stay cleanable"
            );
        }
    }

    #[test]
    fn blocks_protected_paths_regardless_of_case() {
        // macOS volumes are case-insensitive by default and `fs::canonicalize`
        // does NOT case-normalize, so these name exactly the same files as
        // their canonically-cased spellings. A byte-exact denylist misses them.
        for p in [
            "/USR/bin",
            "/system/Library",
            "/Applications/../Applications", // also caught by the `..` rule
            "/APPLICATIONS/Foo.app",
        ] {
            assert!(is_protected(Path::new(p), &home()), "{p} must be protected");
        }
        for p in [
            "library/Keychains/login.keychain-db",
            "Library/mail/V10/inbox",
            "LIBRARY", // ancestor of Keychains/Mail, differently cased
        ] {
            assert!(
                is_protected(&home().join(p), &home()),
                "~/{p} must be protected"
            );
        }
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
    fn blocks_dot_git_whatever_its_case() {
        // macOS is case-insensitive by default, so `.GIT` names the very same
        // working repository as `.git`. Every other rule here folds case; this
        // one used not to, which made it the weakest link in the file.
        for spelling in [".GIT", ".Git", ".gIt"] {
            assert!(
                is_protected(&home().join("Projects/app").join(spelling), &home()),
                "{spelling} names a real repository and must be protected"
            );
            assert!(is_protected(
                &home().join("Projects/app").join(spelling).join("HEAD"),
                &home()
            ));
        }
        // ...but still component-wise and still exact: these are not `.git`.
        assert!(!is_protected(
            &home().join("Library/Caches/gitignore"),
            &home()
        ));
        assert!(!is_protected(&home().join("Library/Caches/.gitx"), &home()));
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
