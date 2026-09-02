//! The dry-run plan: pure data describing what *would* be cleaned.
//!
//! A `Plan` is produced by the scanner and never mutates the filesystem.
//! Constructing one is always safe; only [`crate::executor`] can act on it.

use std::path::{Path, PathBuf};

use safety::{SafeDir, SafePath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposal {
    /// Move to the system Trash (recoverable). The default.
    Trash,
    /// Irreversible removal. Only honored with explicit per-run consent.
    Permanent,
}

#[derive(Debug)]
pub struct PlannedAction {
    pub path: SafePath,
    pub size_bytes: u64,
    pub disposal: Disposal,
    pub category: String,
}

/// A directory vouched for by [`safety::guard_dir`] and destined for the Trash.
///
/// Deliberately carries no [`Disposal`]: a recursive irreversible removal is
/// not a capability this tool has, and a type that cannot express it is a
/// stronger guarantee than a branch that declines to take it. The `SafeDir`
/// brings the tree's real recursive entry count and size with it, which is
/// what the mass-delete threshold and the audit record are shown.
#[derive(Debug)]
pub struct PlannedDirAction {
    pub dir: SafeDir,
    pub category: String,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub actions: Vec<PlannedAction>,
    /// Directory actions. Always empty in anything the scanner produces —
    /// pinned by a test — and only ever filled by a command layer that took
    /// each directory from an explicit per-row grant.
    pub dirs: Vec<PlannedDirAction>,
    /// Count of candidates skipped because they failed the safety guard.
    pub skipped_protected: usize,
}

/// A plan touching more than this many entries is treated as a mass delete and
/// requires explicit confirmation (SAFETY CONTRACT item 5).
pub const MASS_DELETE_COUNT: usize = 100;

/// A plan freeing more than this many bytes is also treated as a mass delete.
pub const MASS_DELETE_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5 GiB

impl Plan {
    /// Names this plan would remove: one per file action, and for each
    /// directory action the directory's own name plus every entry beneath it.
    ///
    /// The load-bearing line for SAFETY CONTRACT item 5: one directory action
    /// must never count as one item, because it stands for the whole tree.
    pub fn count(&self) -> usize {
        self.actions.len()
            + self
                .dirs
                .iter()
                .fold(0usize, |n, d| n.saturating_add(d.dir.entries() + 1))
    }

    pub fn total_bytes(&self) -> u64 {
        let files: u64 = self.actions.iter().map(|a| a.size_bytes).sum();
        self.dirs
            .iter()
            .fold(files, |b, d| b.saturating_add(d.dir.bytes()))
    }

    /// Whether this plan crosses a mass-delete threshold and therefore needs an
    /// explicit confirmation before it may execute.
    ///
    /// Any directory action makes the answer yes, before the numbers are
    /// consulted. SAFETY CONTRACT item 5 says recursive removals require
    /// confirmation — not "large" ones — and the numeric thresholds alone
    /// cannot enforce that: `DirLimits::default().max_bytes` equals
    /// `MASS_DELETE_BYTES` and `guard_dir` refuses on `>`, so a single tree can
    /// never exceed the byte threshold, and one with fewer than
    /// `MASS_DELETE_COUNT` entries would slip under both.
    pub fn requires_confirmation(&self) -> bool {
        !self.dirs.is_empty()
            || self.count() > MASS_DELETE_COUNT
            || self.total_bytes() > MASS_DELETE_BYTES
    }
}

/// One file to move aside, reversibly.
///
/// Deliberately **not** a [`PlannedAction`], and [`StashPlan`] is deliberately
/// not a [`Plan`]. Nothing converts between them, so a plan built to move a
/// login item aside cannot be handed to `executor::execute`, and a disposal
/// plan cannot be handed to `executor::stash`. The separation is the guarantee:
/// a move is not a disposal, and no field of either type can express the other.
///
/// Note what is absent: no `Disposal`. A moved-aside file is never removed, so
/// there is no variant to choose and no permanent branch to decline.
#[derive(Debug, Clone)]
pub struct PlannedMove {
    /// The path as `guard` resolved it — what will actually be acted on.
    path: SafePath,
    /// The path **as it was listed to the user**, before `guard` canonicalized
    /// it.
    ///
    /// Load-bearing, and easy to mistake for redundancy. `guard` resolves
    /// symlinks, so a plist that was *already* a link arrives as its target —
    /// which is not itself a link and is its own canonical spelling, so no
    /// check downstream can tell. Keeping the original spelling and requiring
    /// the two to be equal is what refuses it.
    ///
    /// Be precise about what that buys, because it is a weaker claim than the
    /// Uninstaller's and Privacy's "byte-equal to the row that was shown". What
    /// the type guarantees is **this acts on the file the caller named** —
    /// `path` is derived from `as_listed` by canonicalization inside the only
    /// constructor, so if they are equal they are the same path. It does *not*
    /// know what the user was shown. Keeping the displayed row and the
    /// constructed path in sync remains the caller's obligation, and the
    /// command layer must not read this as already handled.
    as_listed: PathBuf,
    size_bytes: u64,
    /// Which module authorized this, carried into the audit note.
    category: String,
}

impl PlannedMove {
    /// The only way to build one, and the reason the fields above are private.
    ///
    /// `as_listed` carries the same safety weight as `path` — it is what
    /// refuses a plist that was already a symlink — but unlike [`SafePath`] it
    /// is an ordinary `PathBuf` and cannot be unforgeable by construction. With
    /// public fields, a caller could back-fill it *from* the guarded path and
    /// turn the equality check into a tautology, reinstating the exact defect
    /// it exists to prevent, with every test still green.
    ///
    /// So the constructor does the guard itself, from the listed path. There is
    /// no way to supply the two independently.
    pub fn new(
        as_listed: PathBuf,
        home: &Path,
        size_bytes: u64,
        category: String,
    ) -> Result<Self, safety::GuardError> {
        let path = safety::guard(&as_listed, home)?;
        Ok(Self {
            path,
            as_listed,
            size_bytes,
            category,
        })
    }

    pub fn path(&self) -> &SafePath {
        &self.path
    }
    pub fn as_listed(&self) -> &Path {
        &self.as_listed
    }
    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
    pub fn category(&self) -> &str {
        &self.category
    }
}

/// A set of files to move aside, or to put back.
///
/// It has no thresholds and no mass-delete gate, because it removes nothing:
/// the confirmation those exist for is about how much would be *lost*, and the
/// answer here is always none.
#[derive(Debug, Clone, Default)]
pub struct StashPlan {
    pub moves: Vec<PlannedMove>,
}

impl StashPlan {
    pub fn count(&self) -> usize {
        self.moves.len()
    }
}
