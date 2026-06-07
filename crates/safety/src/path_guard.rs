//! Canonicalize + verify. The only constructor of [`SafePath`].

use std::error::Error;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::denylist::protection_reason;

/// A path that has been canonicalized and verified non-protected.
///
/// There is intentionally no public constructor other than [`guard`], so a
/// value of this type is evidence that the path passed the denylist *at the
/// moment it was produced*. Re-run [`guard`] before mutating (TOCTOU).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafePath(PathBuf);

impl SafePath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl fmt::Display for SafePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// Why a path could not be turned into a [`SafePath`].
#[derive(Debug)]
pub enum GuardError {
    /// The input contained a `..` component before canonicalization.
    Traversal(PathBuf),
    /// The path could not be canonicalized (usually: does not exist).
    Unresolvable(PathBuf, std::io::Error),
    /// The canonical path is on the protected denylist.
    Protected { path: PathBuf, reason: &'static str },
}

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::Traversal(p) => {
                write!(f, "refused: path contains `..` traversal: {}", p.display())
            }
            GuardError::Unresolvable(p, e) => {
                write!(f, "refused: cannot resolve {}: {e}", p.display())
            }
            GuardError::Protected { path, reason } => {
                write!(f, "refused: {} is protected ({reason})", path.display())
            }
        }
    }
}

impl Error for GuardError {}

/// Canonicalize `input` (resolving every symlink), reject `..` traversal, then
/// verify the resolved path is not protected.
///
/// `home` must be canonicalized (see [`crate::canonical_home`]). This is the
/// sole gateway to a [`SafePath`]; callers must re-invoke it immediately before
/// any destructive action because canonicalization is a point-in-time check.
pub fn guard(input: &Path, home: &Path) -> Result<SafePath, GuardError> {
    // Reject `..` before touching the filesystem — a canonicalize of a crafted
    // path could otherwise resolve somewhere unexpected.
    if input
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(GuardError::Traversal(input.to_path_buf()));
    }

    let canonical = std::fs::canonicalize(input)
        .map_err(|e| GuardError::Unresolvable(input.to_path_buf(), e))?;

    if let Some(reason) = protection_reason(&canonical, home) {
        return Err(GuardError::Protected {
            path: canonical,
            reason,
        });
    }

    Ok(SafePath(canonical))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Canonicalize a freshly created tempdir so comparisons line up on macOS.
    fn canon(p: &Path) -> PathBuf {
        fs::canonicalize(p).unwrap()
    }

    #[test]
    fn rejects_traversal_without_touching_fs() {
        let home = PathBuf::from("/Users/tester");
        let err = guard(Path::new("/Users/tester/../etc/passwd"), &home).unwrap_err();
        assert!(matches!(err, GuardError::Traversal(_)));
    }

    #[test]
    fn rejects_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let home = canon(dir.path());
        let err = guard(&home.join("does-not-exist"), &home).unwrap_err();
        assert!(matches!(err, GuardError::Unresolvable(_, _)));
    }

    #[test]
    fn accepts_real_file_under_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = canon(dir.path());
        let f = home.join("Library/Caches/app");
        fs::create_dir_all(&f).unwrap();
        let blob = f.join("blob.bin");
        fs::write(&blob, b"junk").unwrap();
        let safe = guard(&blob, &home).unwrap();
        assert_eq!(safe.as_path(), canon(&blob));
    }

    #[test]
    fn symlink_escaping_home_is_resolved_then_blocked() {
        // A symlink living under our fake home but pointing at the home ROOT
        // must be blocked, because canonicalization resolves it to the (
        // protected) home directory itself.
        let dir = tempfile::tempdir().unwrap();
        let home = canon(dir.path());
        let link = home.join("Library/Caches/escape");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&home, &link).unwrap();
        let err = guard(&link, &home).unwrap_err();
        assert!(matches!(err, GuardError::Protected { .. }), "got {err:?}");
    }
}
