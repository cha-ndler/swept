//! Append-only audit log (SAFETY CONTRACT item 6).
//!
//! Every planned (dry-run) and executed action is written as one JSON object
//! per line (JSONL) with an absolute path and size. The file is opened in
//! append mode and flushed after each record, so a crash mid-run still leaves a
//! complete trail of what happened before it.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Recorded during a dry run; nothing was changed.
    Planned,
    /// Recorded when an action was actually carried out.
    Executed,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Trash,
    Permanent,
    /// Re-validation failed at execution time; left untouched.
    Refused,
    /// **Moved aside, reversibly.** The file still exists — nothing was
    /// removed. Only the name it was reachable under changed, and this app can
    /// put it back. Distinct from `Trash` because the Trash is a destination
    /// the *system* owns and empties; this one is a folder beside the original
    /// that only ever holds what was deliberately set aside.
    Stashed,
    /// Put back under the name it had. The mirror of `Stashed`.
    Restored,
}

#[derive(Serialize, Debug)]
pub struct AuditEntry {
    pub epoch_ms: u64,
    pub phase: Phase,
    pub disposition: Disposition,
    /// Absolute, canonical path.
    pub path: String,
    pub size_bytes: u64,
    /// Names beneath a directory target, when the record is for one. One log
    /// line standing for thousands of files must say so as data, not only in
    /// prose. Absent for a file, so every file record serializes exactly as it
    /// always has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An append-only sink for [`AuditEntry`] records.
pub struct AuditLog {
    file: std::fs::File,
}

impl AuditLog {
    /// Open (creating if needed) an audit log in append mode.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file })
    }

    /// Append one record and flush.
    pub fn record(&mut self, entry: &AuditEntry) -> io::Result<()> {
        let line = serde_json::to_string(entry).map_err(io::Error::other)?;
        writeln!(self.file, "{line}")?;
        self.file.flush()
    }
}

/// Milliseconds since the Unix epoch (0 if the clock is before 1970).
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
