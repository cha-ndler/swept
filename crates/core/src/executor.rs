//! Consent-gated executor. The only place files actually move or disappear.
//!
//! Invariants enforced here:
//! - **Dry-run default** (item 1): with [`Consent::execute`] = false, nothing is
//!   mutated; planned actions are written to the audit log and we return.
//! - **No unconfirmed mass delete** (item 5): a plan over threshold is refused
//!   unless `confirmed_mass_delete` is set.
//! - **Trash, not unlink** (item 4): disposal is to Trash unless the action is
//!   `Permanent` *and* `allow_permanent` is set; otherwise it falls back to
//!   Trash (fail safe).
//! - **Re-validate before mutating** (item 2, TOCTOU): every path is run through
//!   [`guard`] again immediately before disposal.
//! - **Durable audit** (item 6): audit write failures abort the run rather than
//!   being swallowed, and irreversible deletes are recorded *before* the unlink.
//!
//! The actual filesystem effect is behind the [`Sink`] trait so tests can run
//! against a throwaway directory and never touch the real Trash.

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use safety::{allowlist, guard};

use crate::audit::{now_ms, AuditEntry, AuditLog, Disposition, Phase};
use crate::plan::{Disposal, Plan};

/// Where disposed files go. Abstracted so tests avoid the real system Trash.
pub trait Sink {
    fn trash(&self, path: &Path) -> io::Result<()>;
    fn delete(&self, path: &Path) -> io::Result<()>;
}

/// Production sink: real macOS Trash, real `unlink`.
pub struct SystemSink;

impl Sink for SystemSink {
    fn trash(&self, path: &Path) -> io::Result<()> {
        trash::delete(path).map_err(|e| io::Error::other(e.to_string()))
    }

    fn delete(&self, path: &Path) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        if meta.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }
}

/// Test sink: "trash" = move into a directory; "delete" = real removal (used
/// only against tempdir fixtures).
pub struct DirSink {
    pub trash_dir: PathBuf,
}

impl Sink for DirSink {
    fn trash(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(&self.trash_dir)?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("path has no file name"))?;
        std::fs::rename(path, self.trash_dir.join(name))
    }

    fn delete(&self, path: &Path) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        if meta.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }
}

/// Explicit, opt-in authorization for destructive work. `Default` is the fully
/// safe state: a dry run with nothing permitted.
#[derive(Clone, Copy, Debug, Default)]
pub struct Consent {
    /// Carry out actions. When false, this is a dry run.
    pub execute: bool,
    /// Permit irreversible deletion for `Permanent` actions.
    pub allow_permanent: bool,
    /// The user explicitly confirmed a mass delete.
    pub confirmed_mass_delete: bool,
}

#[derive(Debug, Default)]
pub struct ExecReport {
    pub planned: usize,
    pub executed: usize,
    pub refused: usize,
    pub bytes_executed: u64,
    /// True if this run only previewed (no mutations).
    pub dry_run: bool,
}

#[derive(Debug)]
pub enum ExecError {
    MassDeleteUnconfirmed {
        count: usize,
        bytes: u64,
    },
    /// The audit log could not be written. We refuse to act without a record.
    Audit(io::Error),
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::MassDeleteUnconfirmed { count, bytes } => write!(
                f,
                "refused: mass delete of {count} items ({bytes} bytes) needs explicit confirmation"
            ),
            ExecError::Audit(e) => {
                write!(
                    f,
                    "refused: cannot write audit log (aborting to stay safe): {e}"
                )
            }
        }
    }
}

impl Error for ExecError {}

/// Carry out (or, by default, preview) a plan under the given consent.
pub fn execute(
    plan: &Plan,
    consent: Consent,
    home: &Path,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<ExecReport, ExecError> {
    let mut report = ExecReport::default();

    // --- Dry-run default: record intentions, change nothing. ---
    if !consent.execute {
        report.dry_run = true;
        for a in &plan.actions {
            record(
                audit,
                Phase::Planned,
                disposition_for(a.disposal, false),
                a.path.as_path(),
                a.size_bytes,
                None,
            )?;
            report.planned += 1;
        }
        return Ok(report);
    }

    // --- No unconfirmed mass delete. ---
    if plan.requires_confirmation() && !consent.confirmed_mass_delete {
        return Err(ExecError::MassDeleteUnconfirmed {
            count: plan.count(),
            bytes: plan.total_bytes(),
        });
    }

    let allowed = allowlist::default_roots(home);

    for a in &plan.actions {
        report.planned += 1;

        // Re-validate immediately before mutating (TOCTOU defense): the path may
        // have changed since the scan. A previously-safe path that now resolves
        // into a protected location is refused, not deleted.
        let safe = match guard(a.path.as_path(), home) {
            Ok(s) => s,
            Err(e) => {
                refuse(
                    &mut report,
                    audit,
                    a.path.as_path(),
                    a.size_bytes,
                    &e.to_string(),
                )?;
                continue;
            }
        };
        if !allowlist::is_allowed(safe.as_path(), &allowed) {
            refuse(
                &mut report,
                audit,
                safe.as_path(),
                a.size_bytes,
                "outside allowlist at execution time",
            )?;
            continue;
        }

        let permanent = matches!(a.disposal, Disposal::Permanent) && consent.allow_permanent;
        let disposition = disposition_for(a.disposal, permanent);

        // Item 6: record an irreversible delete BEFORE it happens, so a crash
        // mid-unlink still leaves a durable record. (Trash is recoverable, so we
        // record it after success.)
        if permanent {
            record(
                audit,
                Phase::Executed,
                disposition,
                safe.as_path(),
                a.size_bytes,
                None,
            )?;
        }

        let outcome = if permanent {
            sink.delete(safe.as_path())
        } else {
            sink.trash(safe.as_path())
        };

        match outcome {
            Ok(()) => {
                report.executed += 1;
                report.bytes_executed += a.size_bytes;
                if !permanent {
                    record(
                        audit,
                        Phase::Executed,
                        disposition,
                        safe.as_path(),
                        a.size_bytes,
                        None,
                    )?;
                }
            }
            Err(e) => {
                // For a permanent action we already logged the intent above;
                // append a correcting refusal so the trail is honest.
                refuse(
                    &mut report,
                    audit,
                    safe.as_path(),
                    a.size_bytes,
                    &e.to_string(),
                )?;
            }
        }
    }

    Ok(report)
}

fn disposition_for(disposal: Disposal, permanent_granted: bool) -> Disposition {
    match disposal {
        Disposal::Permanent if permanent_granted => Disposition::Permanent,
        _ => Disposition::Trash,
    }
}

fn record(
    audit: &mut AuditLog,
    phase: Phase,
    disposition: Disposition,
    path: &Path,
    size_bytes: u64,
    note: Option<String>,
) -> Result<(), ExecError> {
    audit
        .record(&AuditEntry {
            epoch_ms: now_ms(),
            phase,
            disposition,
            path: path.display().to_string(),
            size_bytes,
            note,
        })
        .map_err(ExecError::Audit)
}

fn refuse(
    report: &mut ExecReport,
    audit: &mut AuditLog,
    path: &Path,
    size: u64,
    note: &str,
) -> Result<(), ExecError> {
    report.refused += 1;
    record(
        audit,
        Phase::Executed,
        Disposition::Refused,
        path,
        size,
        Some(note.to_string()),
    )
}
