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

use safety::{allowlist, guard, SafePath};

use crate::audit::{now_ms, AuditEntry, AuditLog, Disposition, Phase};
use crate::plan::{Disposal, Plan};

/// Where disposed files go. Abstracted so tests avoid the real system Trash.
///
/// [`Sink::delete`] is **files only, by construction**. See its doc comment —
/// that is a safety property, not an implementation detail.
pub trait Sink {
    /// Move to the Trash (recoverable).
    ///
    /// This one has **no** directory backstop, and the asymmetry is deliberate
    /// rather than an oversight. `trash::delete` and `fs::rename` both accept a
    /// directory, and neither has a file-only variant — so if a directory were
    /// swapped onto the name after `authorize` inspected it, a whole tree would
    /// move to the Trash. That outcome is recoverable and fully audited, which
    /// is what makes it tolerable where the same race on [`Sink::delete`] would
    /// not be. The one dishonesty it can produce: the audit record would carry
    /// the planned *file's* `size_bytes` for a tree.
    fn trash(&self, path: &Path) -> io::Result<()>;

    /// Irreversibly remove a single *file*.
    ///
    /// Implementations must never recurse into a directory. `authorize`
    /// already refuses directory targets, but that check and this call cannot
    /// be made atomic — something could `rename` a directory onto the name in
    /// between. Using `remove_file` unconditionally makes the race harmless:
    /// `unlink(2)` returns `EPERM` for a directory and removes nothing, so the
    /// worst outcome is a refusal instead of a recursive unlink of a tree
    /// nobody vetted.
    ///
    /// One caveat, stated because a safety property in the trust boundary
    /// should not overstate: `unlink(2)` documents that `EPERM` for a directory
    /// applies when the effective user is **not** the super-user. Running this
    /// tool under `sudo` is neither required nor prevented, and nothing here
    /// checks `geteuid`. Even then the exposure is a stray directory-entry
    /// removal rather than a recursive unlink — but it is not the flat
    /// guarantee the non-root case gives.
    fn delete(&self, path: &Path) -> io::Result<()>;
}

/// Production sink: real macOS Trash, real `unlink`.
pub struct SystemSink;

impl Sink for SystemSink {
    fn trash(&self, path: &Path) -> io::Result<()> {
        trash::delete(path).map_err(|e| io::Error::other(e.to_string()))
    }

    fn delete(&self, path: &Path) -> io::Result<()> {
        // Files only — never `remove_dir_all`. See the trait doc: this is the
        // fail-closed backstop for the unavoidable check/use race.
        std::fs::remove_file(path)
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
        // Files only — never `remove_dir_all`. See the trait doc: this is the
        // fail-closed backstop for the unavoidable check/use race.
        std::fs::remove_file(path)
    }
}

/// Explicit, opt-in authorization for destructive work. `Default` is the fully
/// safe state: a dry run with nothing permitted.
///
/// Deliberately not `Copy`: `granted` is a list of individually-authorized
/// paths, and a type you can duplicate by accident is the wrong shape for
/// something that carries permission.
#[derive(Clone, Debug, Default)]
pub struct Consent {
    /// Carry out actions. When false, this is a dry run.
    pub execute: bool,
    /// Permit irreversible deletion for `Permanent` actions.
    pub allow_permanent: bool,
    /// The user explicitly confirmed a mass delete.
    pub confirmed_mass_delete: bool,
    /// Paths the user picked out individually, one by one, from a read-only
    /// discovery walk (Large & Old Files and friends).
    ///
    /// This is the *only* way to dispose of something outside
    /// [`allowlist::default_roots`], and it is deliberately narrow:
    ///
    /// - Entries are [`SafePath`]s, so each has already survived the denylist —
    ///   a grant can never resurrect `/Applications` or `~/Library/Mail`.
    /// - Matching is **exact**. A grant authorizes one path and confers nothing
    ///   on its children, so granting a directory does not grant its contents.
    /// - Grants must name **files**, though not as a special rule: the executor
    ///   refuses *every* directory target, granted or allowlisted, because
    ///   nothing plans directory actions yet. See [`authorize`].
    /// - The list is capped at [`MAX_GRANTS`] and over-long lists refuse the
    ///   whole run rather than being truncated.
    /// - Every grant-authorized disposal is audited with a distinguishing note.
    pub granted: Vec<SafePath>,
}

/// Upper bound on [`Consent::granted`].
///
/// Grants come from a human ticking boxes in a list, so this is far above any
/// plausible hand-picked selection while still ruling out a caller that tries
/// to hand over a whole walk's worth of paths as "individually chosen".
pub const MAX_GRANTS: usize = 1_000;

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
    /// More individually-granted paths than [`MAX_GRANTS`]. Refused wholesale:
    /// a list this long is not a hand-picked selection, and truncating it would
    /// silently act on a different set than the caller asked for.
    TooManyGrants {
        count: usize,
        max: usize,
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
            ExecError::TooManyGrants { count, max } => write!(
                f,
                "refused: {count} individually-granted paths exceeds the limit of {max}"
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

    // --- Bound the grant list, in both modes. ---
    //
    // Checked before the dry-run branch on purpose: a preview that quietly
    // succeeds while the real run would be refused is a preview that lies.
    if consent.granted.len() > MAX_GRANTS {
        refuse_run(
            audit,
            &format!(
                "{} individually-granted paths exceeds the limit of {MAX_GRANTS}",
                consent.granted.len()
            ),
        )?;
        return Err(ExecError::TooManyGrants {
            count: consent.granted.len(),
            max: MAX_GRANTS,
        });
    }

    let allowed = allowlist::default_roots(home);

    // --- Dry-run default: record intentions, change nothing. ---
    //
    // The preview runs the same authorization the real thing would, because a
    // plan may now legitimately contain paths outside the disposal allowlist
    // (see `Consent::granted`). Reporting those as "would be trashed" and only
    // refusing them at execution time would make the preview overstate what is
    // about to happen — the same dishonesty, pointed the other way, that the
    // under-reporting work set out to fix.
    //
    // It does not re-`guard` here: canonicalization is a point-in-time check
    // worth paying for immediately before a mutation, and there is no mutation
    // to precede. The consequence is stated plainly rather than papered over —
    // a preview describes the disk as it was scanned, and the executor still
    // re-resolves everything before acting.
    if !consent.execute {
        report.dry_run = true;
        for a in &plan.actions {
            match authorize(&a.path, &allowed, &consent.granted) {
                Authorization::Refused(reason) => {
                    refuse(&mut report, audit, a.path.as_path(), a.size_bytes, reason)?;
                }
                auth => {
                    record(
                        audit,
                        Phase::Planned,
                        disposition_for(a.disposal, false),
                        a.path.as_path(),
                        a.size_bytes,
                        note_for(auth),
                    )?;
                    report.planned += 1;
                }
            }
        }
        return Ok(report);
    }

    // --- No unconfirmed mass delete. ---
    if plan.requires_confirmation() && !consent.confirmed_mass_delete {
        refuse_run(
            audit,
            &format!(
                "mass delete of {} items ({} bytes) needs explicit confirmation",
                plan.count(),
                plan.total_bytes()
            ),
        )?;
        return Err(ExecError::MassDeleteUnconfirmed {
            count: plan.count(),
            bytes: plan.total_bytes(),
        });
    }

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
        // Authorization sits *behind* the re-guard above, never in front of it:
        // a grant widens where we may act, it never bypasses the denylist.
        let auth = authorize(&safe, &allowed, &consent.granted);
        if let Authorization::Refused(reason) = auth {
            refuse(&mut report, audit, safe.as_path(), a.size_bytes, reason)?;
            continue;
        }
        let note = note_for(auth);

        // Grants widen *where* we may act, never *how*.
        //
        // Irreversible removal stays confined to the allowlist. That is the
        // safer way round for exactly the reason grants exist: the allowlist
        // covers caches and logs, which regenerate, while a grant covers a file
        // in ~/Documents that the user picked out of a list — the least
        // replaceable data this tool will ever touch, and the least vetted. A
        // granted `Permanent` action therefore falls back to the Trash.
        let permanent = matches!(a.disposal, Disposal::Permanent)
            && consent.allow_permanent
            && matches!(auth, Authorization::Allowlisted);
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
                note.clone(),
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
                        note,
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

/// Audit note marking a disposal that happened because the user pointed at this
/// exact path, rather than because policy said the location was cleanable.
///
/// Worth distinguishing in the log: the first is a judgement about one file
/// nobody else vetted, the second is the tool doing its documented job.
const GRANT_NOTE: &str = "user-granted path outside the allowlist";

/// Refusal reason for a directory target. See [`authorize`] for why this is a
/// blanket refusal rather than a `safety::guard_dir` gate.
const DIRECTORY_REFUSAL: &str =
    "directory target; recursive disposal is not enabled (needs directory-aware planning)";

/// Why a path may be disposed of — or why it may not.
#[derive(Clone, Copy)]
enum Authorization {
    /// Inside [`allowlist::default_roots`]: ordinary, policy-driven cleanup.
    Allowlisted,
    /// Outside it, but named individually by the user.
    Granted,
    Refused(&'static str),
}

/// Decide whether `safe` may be acted on. Called *after* the pre-mutation
/// re-guard, so `safe` has already survived the denylist.
fn authorize(safe: &SafePath, allowed: &[PathBuf], granted: &[SafePath]) -> Authorization {
    // Directory targets are refused outright, wherever the authorization would
    // have come from. One directory action stands for an unknown number of
    // files, so a check on the directory's own path is not a check on what is
    // about to be removed — the dangerous content is inside it.
    //
    // `safety::guard_dir` is the answer to that, and it now exists: it walks
    // the tree, refuses a `.git` at any depth, and fails closed on anything it
    // cannot read or that exceeds its bounds. What does not exist yet is a
    // *planner* that produces directory actions — `scanner.rs` plans files
    // only — so wiring `guard_dir` in here would add an unused destructive
    // capability to a tool whose contract says to refuse when in doubt. M4
    // introduces directory-aware planning and turns this refusal into a
    // `guard_dir` gate, in one reviewed change.
    //
    // Checked before authorization so the refusal reason names the real
    // problem rather than blaming the allowlist for it.
    match std::fs::symlink_metadata(safe.as_path()) {
        Ok(m) if m.is_dir() => return Authorization::Refused(DIRECTORY_REFUSAL),
        Ok(_) => {}
        // If we cannot tell what it is, we do not act on it.
        Err(_) => return Authorization::Refused("target could not be inspected"),
    }

    if allowlist::is_allowed(safe.as_path(), allowed) {
        return Authorization::Allowlisted;
    }

    // Exact equality, never `starts_with`. A grant names one path and confers
    // nothing on its children — otherwise granting a directory would smuggle in
    // a recursive removal of everything beneath it as a single "hand-picked
    // item". Comparing against `safe` (the freshly re-resolved path) rather
    // than the planned one also means a symlink swapped in after the user chose
    // cannot redirect an authorization onto a different file.
    if !granted.iter().any(|g| g.as_path() == safe.as_path()) {
        // Two different situations, and the log should say which. "No grants
        // were offered" is the ordinary confinement of cleanup to the
        // allowlist; "grants were offered and this path was not among them" is
        // a caller trying to dispose of something the user did not pick, which
        // is the one worth being able to find afterwards.
        return Authorization::Refused(if granted.is_empty() {
            "outside allowlist at execution time"
        } else {
            "outside allowlist and not among the granted paths"
        });
    }

    Authorization::Granted
}

/// The audit note an authorization deserves. `Refused` never reaches here —
/// refusals carry their own reason string through [`refuse`].
fn note_for(auth: Authorization) -> Option<String> {
    match auth {
        Authorization::Granted => Some(GRANT_NOTE.to_string()),
        _ => None,
    }
}

/// Sentinel path for an audit record about the run as a whole rather than one
/// file. Not a path, and deliberately not shaped like one, so nothing that
/// reads the log back (restore, in particular) can mistake it for a target.
pub const WHOLE_RUN: &str = "(whole run — no action taken)";

/// Record that an entire run was refused before it touched anything.
///
/// Item 6 wants a durable trace of refusals, and a wholesale refusal used to
/// leave none: the early `return Err(...)` happened before any `record` call,
/// so the most decisive thing the executor can do was the one thing the log
/// never mentioned.
/// Public so callers that refuse *before* reaching [`execute`] — the command
/// layer's own validation, for instance — record the refusal the same way and
/// under the same sentinel, rather than each inventing a shape for it.
pub fn record_run_refusal(audit: &mut AuditLog, reason: &str) -> Result<(), ExecError> {
    refuse_run(audit, reason)
}

fn refuse_run(audit: &mut AuditLog, reason: &str) -> Result<(), ExecError> {
    record(
        audit,
        Phase::Planned,
        Disposition::Refused,
        Path::new(WHOLE_RUN),
        0,
        Some(reason.to_string()),
    )
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
