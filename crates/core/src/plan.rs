//! The dry-run plan: pure data describing what *would* be cleaned.
//!
//! A `Plan` is produced by the scanner and never mutates the filesystem.
//! Constructing one is always safe; only [`crate::executor`] can act on it.

use safety::SafePath;

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

#[derive(Debug, Default)]
pub struct Plan {
    pub actions: Vec<PlannedAction>,
    /// Count of candidates skipped because they failed the safety guard.
    pub skipped_protected: usize,
}

/// A plan touching more than this many entries is treated as a mass delete and
/// requires explicit confirmation (SAFETY CONTRACT item 5).
pub const MASS_DELETE_COUNT: usize = 100;

/// A plan freeing more than this many bytes is also treated as a mass delete.
pub const MASS_DELETE_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5 GiB

impl Plan {
    pub fn count(&self) -> usize {
        self.actions.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.actions.iter().map(|a| a.size_bytes).sum()
    }

    /// Whether this plan crosses a mass-delete threshold and therefore needs an
    /// explicit confirmation before it may execute.
    pub fn requires_confirmation(&self) -> bool {
        self.count() > MASS_DELETE_COUNT || self.total_bytes() > MASS_DELETE_BYTES
    }
}
