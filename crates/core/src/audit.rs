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
    ///
    /// Refuses as the super-user, and this is the place that matters rather
    /// than a courtesy duplicate of the check in [`crate::executor::execute`].
    /// `create(true)` is what brings the file into existence, so **this** call
    /// is where a privileged run leaves a root-owned `audit.jsonl` behind —
    /// after which every ordinary run aborts, because this project refuses to
    /// act when it cannot record. Refusing at the executor would be too late:
    /// the caller has already opened the log by then.
    ///
    /// Every front door reaches here — the CLI's `--audit PATH` included, which
    /// is why the check is not in `default_audit_path` alone.
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_as(path, crate::privilege::effective_uid())
    }

    /// [`AuditLog::open`], with the effective user supplied rather than read.
    ///
    /// Private for the same reason the executor's seam is: a test process
    /// cannot become root, and a refusal nothing exercises is the shape of bug
    /// this project treats as worse than no refusal at all.
    fn open_as(path: &Path, euid: u32) -> io::Result<Self> {
        if let Some(why) = crate::privilege::refusal(euid) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, why));
        }
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

#[cfg(test)]
mod privilege_tests {
    use super::*;
    use crate::privilege::SUPER_USER;

    /// The property the executor's own refusal cannot claim: **the file is
    /// never brought into existence.** `create(true)` here is what leaves a
    /// root-owned `audit.jsonl` behind after one `sudo` run, and a root-owned
    /// audit log stops every later ordinary run, because this project aborts
    /// rather than act without a record. So one privileged run would disable
    /// the tool for the person who owns the files.
    #[test]
    fn a_privileged_run_never_creates_the_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        let err = match AuditLog::open_as(&path, SUPER_USER) {
            Err(e) => e,
            Ok(_) => panic!("a privileged run opened the audit log"),
        };
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            !path.exists(),
            "the audit log was created despite the refusal"
        );
    }

    /// The refusal has to reach the person, not just the caller: this is the
    /// text a `sudo swept clean` prints.
    #[test]
    fn the_refusal_carries_the_reason_through_the_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = match AuditLog::open_as(&dir.path().join("audit.jsonl"), SUPER_USER) {
            Err(e) => e,
            Ok(_) => panic!("a privileged run opened the audit log"),
        };
        assert!(err.to_string().contains("run it as yourself"), "{err}");
    }

    /// Pins that the test above is about the uid and not about the fixture:
    /// the same path, the same call, an ordinary user, and the file appears.
    #[test]
    fn an_ordinary_user_gets_a_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        let Ok(mut log) = AuditLog::open_as(&path, 501) else {
            panic!("an ordinary user was refused a log")
        };
        log.record(&AuditEntry {
            epoch_ms: 0,
            phase: Phase::Planned,
            disposition: Disposition::Trash,
            path: "/fixture/a.bin".into(),
            size_bytes: 1,
            entries: None,
            note: None,
        })
        .unwrap();
        assert!(path.exists());
        assert!(std::fs::read_to_string(&path).unwrap().contains("a.bin"));
    }
}
