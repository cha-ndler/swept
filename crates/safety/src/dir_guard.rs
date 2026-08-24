//! Bounded, fail-closed guard for *recursive* disposal.
//!
//! [`crate::guard`] answers "may this file be removed?". It is sufficient
//! precisely as long as every disposal target is a single file, which is true
//! today — the scanner plans files only. The moment something plans a
//! directory, one action stands for an unknown number of files, and a check on
//! the directory's own path stops being enough: the dangerous content is
//! *inside* it.
//!
//! [`guard_dir`] is the answer. It walks the tree first and refuses unless it
//! can say, having looked, exactly what is in there.
//!
//! # Fail closed, everywhere
//!
//! Every uncertainty is a refusal, not a shrug:
//!
//! - an unreadable subdirectory means we cannot know what the tree contains,
//!   so the whole tree is refused — not skipped and silently under-counted;
//! - a tree bigger, deeper, or busier than the caller's limits is refused
//!   rather than truncated, because a truncated walk describes a *different*
//!   tree than the one about to be removed;
//! - anything the denylist objects to, at any depth, refuses the whole tree.
//!
//! The last one is what makes this the `.git` guard the safety contract asks
//! for: [`protection_reason`] already refuses any path with a `.git` component,
//! so a source repository buried anywhere inside a candidate directory takes
//! the entire directory off the table.
//!
//! # What it deliberately does not do
//!
//! A [`SafeDir`] is a point-in-time statement, exactly like [`crate::SafePath`].
//! The tree can change between the walk and the removal, and no amount of
//! walking fixes that. It is not a substitute for the caller re-checking
//! immediately before acting; it is what makes an informed confirmation
//! possible at all, by producing the real count and size to put in front of the
//! user (SAFETY CONTRACT item 5).

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::denylist::protection_reason;
use crate::guard;
use crate::path_guard::GuardError;

/// Bounds on how much tree [`guard_dir`] will vouch for.
///
/// These are refusal thresholds, not truncation points. Exceeding one aborts
/// the walk with an error.
#[derive(Clone, Copy, Debug)]
pub struct DirLimits {
    /// Maximum number of entries (files, directories, symlinks) beneath the root.
    pub max_entries: usize,
    /// Maximum total size of the regular files beneath the root.
    pub max_bytes: u64,
    /// Maximum directory levels below the root that may be descended.
    pub max_depth: usize,
}

impl Default for DirLimits {
    /// Generous enough for an application's leftover tree, small enough that
    /// anything wildly outside that shape gets a second look from a human.
    fn default() -> Self {
        Self {
            max_entries: 50_000,
            max_bytes: 5 * 1024 * 1024 * 1024, // 5 GiB, matching MASS_DELETE_BYTES
            max_depth: 32,
        }
    }
}

/// A directory that has been walked in full and found safe to remove
/// recursively — together with what the walk actually found.
///
/// As with [`crate::SafePath`], the only constructor is [`guard_dir`], so
/// holding one is evidence the walk happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeDir {
    path: PathBuf,
    entries: usize,
    bytes: u64,
}

impl SafeDir {
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    /// Entries found beneath the root. Feeds the mass-delete confirmation, so
    /// the user is told how many files one "directory" action stands for.
    pub fn entries(&self) -> usize {
        self.entries
    }

    /// Total size of the regular files beneath the root. Symlinks contribute
    /// nothing: removing a symlink reclaims the link, not its target.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Why a directory could not be vouched for.
#[derive(Debug)]
pub enum GuardDirError {
    /// The root itself failed the ordinary path guard.
    Root(GuardError),
    /// The root exists but is not a directory.
    NotADirectory(PathBuf),
    /// Something in the tree is protected — `.git`, keychains, mail, and so on.
    Protected {
        path: PathBuf,
        reason: &'static str,
    },
    /// A part of the tree could not be read, so its contents are unknown.
    Unreadable(PathBuf, std::io::Error),
    /// A subdirectory resolves outside the root (a mount point or firmlink).
    Escapes {
        root: PathBuf,
        entry: PathBuf,
    },
    TooManyEntries {
        limit: usize,
    },
    TooLarge {
        limit: u64,
    },
    TooDeep {
        limit: usize,
    },
}

impl fmt::Display for GuardDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardDirError::Root(e) => write!(f, "{e}"),
            GuardDirError::NotADirectory(p) => {
                write!(f, "refused: {} is not a directory", p.display())
            }
            GuardDirError::Protected { path, reason } => write!(
                f,
                "refused: the tree contains {} which is protected ({reason})",
                path.display()
            ),
            GuardDirError::Unreadable(p, e) => write!(
                f,
                "refused: cannot read {}, so the tree's contents are unknown: {e}",
                p.display()
            ),
            GuardDirError::Escapes { root, entry } => write!(
                f,
                "refused: {} resolves outside {}",
                entry.display(),
                root.display()
            ),
            GuardDirError::TooManyEntries { limit } => {
                write!(f, "refused: the tree holds more than {limit} entries")
            }
            GuardDirError::TooLarge { limit } => {
                write!(f, "refused: the tree is larger than {limit} bytes")
            }
            GuardDirError::TooDeep { limit } => {
                write!(f, "refused: the tree is deeper than {limit} levels")
            }
        }
    }
}

impl Error for GuardDirError {}

/// True if `path` is `root` or lies beneath it. Both must be canonical.
///
/// Split out because the escape it defends against — a mount point or firmlink
/// inside the tree — cannot be constructed in a unit test without root
/// privileges, so the predicate is tested directly instead.
fn is_inside(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

/// Walk `input` in full and, if every entry passes, vouch for removing it
/// recursively.
///
/// `home` must be canonicalized (see [`crate::canonical_home`]). Never mutates
/// anything. See the module docs for the fail-closed rules.
pub fn guard_dir(input: &Path, home: &Path, limits: DirLimits) -> Result<SafeDir, GuardDirError> {
    // The root must clear everything a single-file target clears: no `..`,
    // canonicalized, denylist-checked.
    let root = guard(input, home).map_err(GuardDirError::Root)?;
    let root = root.into_path_buf();

    let meta =
        std::fs::symlink_metadata(&root).map_err(|e| GuardDirError::Unreadable(root.clone(), e))?;
    if !meta.is_dir() {
        return Err(GuardDirError::NotADirectory(root));
    }

    let mut entries = 0usize;
    let mut bytes = 0u64;

    // An explicit stack rather than recursion: a pathological tree should hit
    // `max_depth` and be refused, never overflow the stack on the way there.
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.clone(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > limits.max_depth {
            return Err(GuardDirError::TooDeep {
                limit: limits.max_depth,
            });
        }

        let listing =
            std::fs::read_dir(&dir).map_err(|e| GuardDirError::Unreadable(dir.clone(), e))?;

        for item in listing {
            // A single unreadable entry is enough to make the tree unknown.
            let item = item.map_err(|e| GuardDirError::Unreadable(dir.clone(), e))?;
            let path = item.path();

            entries += 1;
            if entries > limits.max_entries {
                return Err(GuardDirError::TooManyEntries {
                    limit: limits.max_entries,
                });
            }

            // The denylist, at every depth. This is what refuses a tree with a
            // `.git` anywhere inside it.
            if let Some(reason) = protection_reason(&path, home) {
                return Err(GuardDirError::Protected { path, reason });
            }

            let meta = std::fs::symlink_metadata(&path)
                .map_err(|e| GuardDirError::Unreadable(path.clone(), e))?;
            let file_type = meta.file_type();

            if file_type.is_symlink() {
                // Not followed, and not a way out of the tree: a recursive
                // removal unlinks the symlink itself, never its target. It also
                // contributes no bytes, because removing it reclaims none.
                continue;
            }

            if file_type.is_dir() {
                // Prove the walk cannot leave the root. Symlinks are already
                // handled above, so this is here for mount points and macOS
                // firmlinks, which `starts_with` on the raw path would miss.
                let canonical = std::fs::canonicalize(&path)
                    .map_err(|e| GuardDirError::Unreadable(path.clone(), e))?;
                if !is_inside(&canonical, &root) {
                    return Err(GuardDirError::Escapes {
                        root,
                        entry: canonical,
                    });
                }
                stack.push((path, depth + 1));
            } else {
                bytes = bytes.saturating_add(meta.len());
                if bytes > limits.max_bytes {
                    return Err(GuardDirError::TooLarge {
                        limit: limits.max_bytes,
                    });
                }
            }
        }
    }

    Ok(SafeDir {
        path: root,
        entries,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// SAFETY CONTRACT item 7: a throwaway tempdir, canonicalized because
    /// `/var/folders` is itself a symlink on macOS.
    fn fixture_home() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        (dir, home)
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// Every path in the tree, relative and sorted — a cheap way to assert that
    /// a walk changed nothing.
    fn listing(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                out.push(p.strip_prefix(root).unwrap().display().to_string());
                if p.is_dir() {
                    stack.push(p);
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_clean_tree_is_vouched_for_with_its_real_size() {
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(&root.join("a.bin"), b"12345"); // 5
        write(&root.join("nested/b.bin"), b"6789"); // 4
        write(&root.join("nested/deeper/c.bin"), b"0"); // 1

        let safe = guard_dir(&root, &home, DirLimits::default()).unwrap();

        assert_eq!(safe.as_path(), root.as_path());
        assert_eq!(safe.bytes(), 10, "the real recursive size, not the root's");
        // 3 files + 2 directories.
        assert_eq!(safe.entries(), 5);
    }

    #[test]
    fn an_empty_directory_is_fine() {
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/empty");
        fs::create_dir_all(&root).unwrap();

        let safe = guard_dir(&root, &home, DirLimits::default()).unwrap();
        assert_eq!(safe.entries(), 0);
        assert_eq!(safe.bytes(), 0);
    }

    #[test]
    fn a_git_repository_anywhere_in_the_tree_refuses_all_of_it() {
        // The headline case. A source repository buried inside a candidate
        // directory takes the whole directory off the table — the alternative
        // is removing someone's uncommitted work along with a cache.
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(&root.join("harmless.bin"), b"junk");
        write(
            &root.join("vendor/project/.git/HEAD"),
            b"ref: refs/heads/main",
        );

        let err = guard_dir(&root, &home, DirLimits::default()).unwrap_err();
        assert!(
            matches!(err, GuardDirError::Protected { reason, .. } if reason.contains(".git")),
            "got {err:?}"
        );
    }

    #[test]
    fn a_dot_git_file_is_refused_too_not_just_a_directory() {
        // A git worktree or submodule has `.git` as a *file* containing a
        // gitdir pointer. Refusing only directories named `.git` would walk
        // straight past it.
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(
            &root.join("wt/.git"),
            b"gitdir: /elsewhere/.git/worktrees/wt",
        );

        let err = guard_dir(&root, &home, DirLimits::default()).unwrap_err();
        assert!(
            matches!(err, GuardDirError::Protected { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn an_unreadable_subdirectory_refuses_the_whole_tree() {
        // Fail closed. Skipping the unreadable part would mean vouching for a
        // tree while reporting a size that excludes an unknown amount of it.
        use std::os::unix::fs::PermissionsExt;

        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(&root.join("visible.bin"), b"junk");
        let locked = root.join("locked");
        fs::create_dir_all(&locked).unwrap();
        write(&locked.join("hidden.bin"), b"secret");

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = guard_dir(&root, &home, DirLimits::default());
        // Restore before asserting so the tempdir can always clean itself up.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            matches!(result, Err(GuardDirError::Unreadable(_, _))),
            "an unreadable subtree must refuse the whole tree, got {result:?}"
        );
    }

    #[test]
    fn more_entries_than_the_limit_is_refused() {
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        for i in 0..5 {
            write(&root.join(format!("f{i}.bin")), b"x");
        }

        let limits = DirLimits {
            max_entries: 2,
            ..Default::default()
        };
        let err = guard_dir(&root, &home, limits).unwrap_err();
        assert!(
            matches!(err, GuardDirError::TooManyEntries { limit: 2 }),
            "got {err:?}"
        );
    }

    #[test]
    fn a_tree_larger_than_the_limit_is_refused() {
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(&root.join("big.bin"), &[0u8; 64]);

        let limits = DirLimits {
            max_bytes: 10,
            ..Default::default()
        };
        let err = guard_dir(&root, &home, limits).unwrap_err();
        assert!(
            matches!(err, GuardDirError::TooLarge { limit: 10 }),
            "got {err:?}"
        );
    }

    #[test]
    fn a_tree_deeper_than_the_limit_is_refused() {
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(&root.join("one/two/three/four/deep.bin"), b"x");

        let limits = DirLimits {
            max_depth: 2,
            ..Default::default()
        };
        let err = guard_dir(&root, &home, limits).unwrap_err();
        assert!(
            matches!(err, GuardDirError::TooDeep { limit: 2 }),
            "got {err:?}"
        );
    }

    #[test]
    fn the_limits_are_refusals_not_truncations() {
        // The distinction that matters: an over-limit tree must produce an
        // error, never a SafeDir describing the part that fit. A truncated
        // walk describes a different tree than the one about to be removed.
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        for i in 0..10 {
            write(&root.join(format!("f{i}.bin")), &[0u8; 8]);
        }

        for limits in [
            DirLimits {
                max_entries: 3,
                ..Default::default()
            },
            DirLimits {
                max_bytes: 16,
                ..Default::default()
            },
        ] {
            assert!(
                guard_dir(&root, &home, limits).is_err(),
                "over-limit must refuse, not report a partial tree"
            );
        }
    }

    #[test]
    fn a_file_is_not_a_directory() {
        let (_g, home) = fixture_home();
        let f = home.join("Library/Caches/app/a.bin");
        write(&f, b"junk");

        let err = guard_dir(&f, &home, DirLimits::default()).unwrap_err();
        assert!(
            matches!(err, GuardDirError::NotADirectory(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn a_protected_root_is_refused_before_any_walking() {
        let (_g, home) = fixture_home();
        fs::create_dir_all(home.join("Library/Mail/V10")).unwrap();

        for candidate in [home.join("Library/Mail"), home.clone()] {
            let err = guard_dir(&candidate, &home, DirLimits::default()).unwrap_err();
            assert!(
                matches!(err, GuardDirError::Root(_)),
                "{} must be refused by the root guard, got {err:?}",
                candidate.display()
            );
        }
    }

    #[test]
    fn a_symlink_inside_the_tree_is_neither_followed_nor_counted() {
        // A recursive removal unlinks the symlink, never its target, so a
        // symlink is not a route out of the tree — and it reclaims no bytes,
        // so counting its target's size would overstate what is freed.
        let (_g, home) = fixture_home();
        let outside = home.join("Documents/precious");
        write(&outside.join("big.bin"), &[0u8; 4096]);

        let root = home.join("Library/Caches/app");
        write(&root.join("real.bin"), b"1234"); // 4 bytes
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let safe = guard_dir(&root, &home, DirLimits::default()).unwrap();

        assert_eq!(safe.bytes(), 4, "the symlink's target must not be counted");
        assert_eq!(safe.entries(), 2, "the symlink itself is still an entry");
        assert!(outside.join("big.bin").exists());
    }

    #[test]
    fn is_inside_rejects_siblings_and_prefix_lookalikes() {
        // Component-wise, so `/a/bcache` is not inside `/a/b`. Tested directly
        // because the escape it guards against — a mount point or firmlink
        // inside the tree — cannot be created in a unit test without root.
        let root = Path::new("/fixture/Library/Caches");
        assert!(is_inside(root, root));
        assert!(is_inside(Path::new("/fixture/Library/Caches/app/x"), root));
        assert!(!is_inside(Path::new("/fixture/Library/CachesOther"), root));
        assert!(!is_inside(Path::new("/fixture/Library"), root));
        assert!(!is_inside(Path::new("/elsewhere"), root));
    }

    #[test]
    fn guard_dir_mutates_nothing() {
        let (_g, home) = fixture_home();
        let root = home.join("Library/Caches/app");
        write(&root.join("a.bin"), b"12345");
        write(&root.join("nested/b.bin"), b"6789");

        let before = listing(&home);
        guard_dir(&root, &home, DirLimits::default()).unwrap();
        // And once more where it refuses, since an aborted walk is the path
        // most likely to leave something behind.
        let _ = guard_dir(
            &root,
            &home,
            DirLimits {
                max_entries: 1,
                ..Default::default()
            },
        );
        assert_eq!(before, listing(&home), "the guard is read-only");
    }
}
