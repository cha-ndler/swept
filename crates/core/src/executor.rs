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
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use safety::{allowlist, guard, guard_dir, DirLimits, SafeDir, SafePath};

use crate::audit::{now_ms, AuditEntry, AuditLog, Disposition, Phase};
use crate::plan::{Disposal, Plan, StashPlan};

/// Where disposed files go. Abstracted so tests avoid the real system Trash.
///
/// [`Sink::delete`] is **files only, by construction**. See its doc comment —
/// that is a safety property, not an implementation detail.
pub trait Sink {
    /// Move to the Trash (recoverable).
    ///
    /// This one accepts a directory, and since M4 that is a legitimate use: a
    /// `PlannedDirAction` — a tree walked in full by `safety::guard_dir` and
    /// named by an explicit grant — is moved here as one recoverable unit,
    /// with the freshly-walked recursive size and entry count on its audit
    /// record.
    ///
    /// It also has **no** directory backstop for the *file* path, and that
    /// asymmetry is deliberate rather than an oversight. `trash::delete` and
    /// `fs::rename` both accept a directory, and neither has a file-only
    /// variant — so if a directory were swapped onto a file's name after
    /// `authorize` inspected it, a whole tree would move to the Trash. That
    /// outcome is recoverable and fully audited, which is what makes it
    /// tolerable where the same race on [`Sink::delete`] would not be. The one
    /// dishonesty it can produce: the audit record would carry the planned
    /// *file's* `size_bytes` for a tree.
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
    /// - Grants in this list name **files**. A file action that names a
    ///   directory is refused whatever authorized it — see [`authorize`];
    ///   directories have their own list below and their own action shape.
    /// - The list is capped at [`MAX_GRANTS`] (shared with `granted_dirs`) and
    ///   over-long lists refuse the whole run rather than being truncated.
    /// - Every grant-authorized disposal is audited with a distinguishing note.
    pub granted: Vec<SafePath>,
    /// Directories the user picked out individually — the Uninstaller's
    /// leftover trees. The same bounds as `granted`, plus:
    ///
    /// - Entries are [`SafeDir`]s, so each tree was walked in full by
    ///   [`safety::guard_dir`] and found free of protected paths at every
    ///   depth. There is no other constructor, so the walk happened.
    /// - Matching is **exact** and confers nothing on children or parents.
    /// - There is **no allowlist route** for a directory: a tree inside
    ///   `~/Library/Caches` still needs a grant. See [`authorize_dir`].
    /// - A directory action carries no `Disposal`, so it can only ever be
    ///   trashed. `allow_permanent` does not apply to it.
    /// - The cap is `granted.len() + granted_dirs.len()`, so a selection cannot
    ///   double its bound by splitting itself across the two lists.
    pub granted_dirs: Vec<SafeDir>,
}

/// Upper bound on [`Consent::granted`] and [`Consent::granted_dirs`] combined.
///
/// Grants come from a human ticking boxes in a list, so this is far above any
/// plausible hand-picked selection while still ruling out a caller that tries
/// to hand over a whole walk's worth of paths as "individually chosen".
pub const MAX_GRANTS: usize = 1_000;

/// What a run did — or, in a dry run, would do.
///
/// `planned` and `executed` count *actions*, and a directory action is one
/// action standing for a whole tree. The magnitude a preview should show a
/// human comes from [`Plan::count`] and [`Plan::total_bytes`], which see the
/// tree; never from this struct's action counts.
#[derive(Debug, Default)]
pub struct ExecReport {
    pub planned: usize,
    pub executed: usize,
    pub refused: usize,
    pub bytes_executed: u64,
    /// Names beneath the directory actions a dry run previewed, so a caller
    /// that only has this struct can still say how many files those actions
    /// stand for.
    pub entries_planned: u64,
    /// Names removed by directory actions, so a caller can say how many files
    /// one "directory" stood for. Files count in `executed`, not here.
    pub entries_executed: u64,
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

    // --- Bound the grant lists, in both modes. ---
    //
    // Checked before the dry-run branch on purpose: a preview that quietly
    // succeeds while the real run would be refused is a preview that lies.
    // One cap over both lists, so a selection cannot double its bound by
    // splitting itself between files and directories.
    let grants = consent.granted.len() + consent.granted_dirs.len();
    if grants > MAX_GRANTS {
        refuse_run(
            audit,
            &format!("{grants} individually-granted paths exceeds the limit of {MAX_GRANTS}"),
        )?;
        return Err(ExecError::TooManyGrants {
            count: grants,
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
                    refuse(
                        &mut report,
                        audit,
                        Phase::Planned,
                        a.path.as_path(),
                        a.size_bytes,
                        None,
                        reason,
                    )?;
                }
                auth => {
                    record(
                        audit,
                        Phase::Planned,
                        disposition_for(a.disposal, false),
                        a.path.as_path(),
                        a.size_bytes,
                        None,
                        note_for(auth, &a.category),
                    )?;
                    report.planned += 1;
                }
            }
        }
        // Directory actions preview under exactly the rule they execute
        // under: by grant, and by nothing else. No re-walk here, for the
        // reason given above — there is no mutation to precede.
        for d in &plan.dirs {
            match authorize_dir(&d.dir, &consent.granted_dirs) {
                Err(reason) => refuse(
                    &mut report,
                    audit,
                    Phase::Planned,
                    d.dir.as_path(),
                    d.dir.bytes(),
                    Some(d.dir.entries() as u64),
                    reason,
                )?,
                Ok(()) => {
                    record(
                        audit,
                        Phase::Planned,
                        Disposition::Trash,
                        d.dir.as_path(),
                        d.dir.bytes(),
                        Some(d.dir.entries() as u64),
                        Some(grant_note(GRANT_DIR_NOTE, &d.category)),
                    )?;
                    report.planned += 1;
                    report.entries_planned = report
                        .entries_planned
                        .saturating_add(d.dir.entries() as u64);
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
                    Phase::Executed,
                    a.path.as_path(),
                    a.size_bytes,
                    None,
                    &e.to_string(),
                )?;
                continue;
            }
        };
        // Authorization sits *behind* the re-guard above, never in front of it:
        // a grant widens where we may act, it never bypasses the denylist.
        let auth = authorize(&safe, &allowed, &consent.granted);
        if let Authorization::Refused(reason) = auth {
            refuse(
                &mut report,
                audit,
                Phase::Executed,
                safe.as_path(),
                a.size_bytes,
                None,
                reason,
            )?;
            continue;
        }
        let note = note_for(auth, &a.category);

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
                None,
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
                        None,
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
                    Phase::Executed,
                    safe.as_path(),
                    a.size_bytes,
                    None,
                    &e.to_string(),
                )?;
            }
        }
    }

    // --- Directory actions: by grant only, re-walked immediately before. ---
    for d in &plan.dirs {
        report.planned += 1;
        let planned = d.dir.as_path();
        let planned_entries = Some(d.dir.entries() as u64);

        // The TOCTOU re-walk. `guard_dir` runs the denylist on the root and on
        // every entry at every depth, so a `.git` that appeared, a component
        // swapped for a symlink, or a tree that outgrew `DirLimits` since the
        // plan was built is refused here rather than trashed.
        let fresh = match guard_dir(planned, home, DirLimits::default()) {
            Ok(f) => f,
            Err(e) => {
                refuse(
                    &mut report,
                    audit,
                    Phase::Executed,
                    planned,
                    d.dir.bytes(),
                    planned_entries,
                    &e.to_string(),
                )?;
                continue;
            }
        };
        // A root that now resolves elsewhere is not the directory that was
        // planned, whatever the walk found there.
        if fresh.as_path() != planned {
            // The one refusal a user most needs to reconstruct afterwards, so
            // the record says where the granted path now leads.
            refuse(
                &mut report,
                audit,
                Phase::Executed,
                planned,
                d.dir.bytes(),
                planned_entries,
                &format!(
                    "the directory now resolves elsewhere, to {}",
                    fresh.as_path().display()
                ),
            )?;
            continue;
        }
        // No growth. The mass-delete gate above measured the *planned*
        // figures; a tree that gained entries or bytes since then could cross
        // a threshold the user never confirmed. Shrinking is fine, and the
        // fresh figures are what get audited.
        if fresh.entries() > d.dir.entries() || fresh.bytes() > d.dir.bytes() {
            refuse(
                &mut report,
                audit,
                Phase::Executed,
                planned,
                fresh.bytes(),
                Some(fresh.entries() as u64),
                "the directory grew since it was planned, so the confirmed figures no longer \
                 describe it",
            )?;
            continue;
        }
        // Authorization sits behind the re-walk, exactly as for files: a grant
        // widens where we may act, it never bypasses the denylist. Matched
        // against the fresh path.
        if let Err(reason) = authorize_dir(&fresh, &consent.granted_dirs) {
            refuse(
                &mut report,
                audit,
                Phase::Executed,
                fresh.as_path(),
                fresh.bytes(),
                Some(fresh.entries() as u64),
                reason,
            )?;
            continue;
        }
        // Trash only. There is no permanent branch to fall into — the action
        // type cannot express one — and the move is recoverable, so it is
        // recorded after success like any other trash disposal.
        //
        // The residual, stated rather than assumed: the re-walk above is
        // O(tree), and between its last `read_dir` and the move below content
        // can still be added — a file, or for that matter a `.git` checkout —
        // and it goes to the Trash with the tree, uncounted, so the record's
        // `entries` and `size_bytes` understate what moved. This window cannot
        // be closed from user space. It is tolerable for the same three
        // reasons the file path's race is: the destination is recoverable,
        // `remove_dir_all` appears nowhere in this crate, and the record
        // names the path so the tree can be found and inspected.
        match sink.trash(fresh.as_path()) {
            Ok(()) => {
                report.executed += 1;
                report.bytes_executed = report.bytes_executed.saturating_add(fresh.bytes());
                report.entries_executed = report
                    .entries_executed
                    .saturating_add(fresh.entries() as u64);
                record(
                    audit,
                    Phase::Executed,
                    Disposition::Trash,
                    fresh.as_path(),
                    fresh.bytes(),
                    Some(fresh.entries() as u64),
                    Some(grant_note(GRANT_DIR_NOTE, &d.category)),
                )?;
            }
            Err(e) => {
                refuse(
                    &mut report,
                    audit,
                    Phase::Executed,
                    fresh.as_path(),
                    fresh.bytes(),
                    Some(fresh.entries() as u64),
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

/// Audit note for a directory disposed of by grant. Distinct from
/// [`GRANT_NOTE`] so the log tells one file from a whole tree at a glance.
///
/// It used to name the Uninstaller, which was true while that was the only
/// caller and became a falsehood the moment a second module planned a
/// directory action: a browser cache would have been logged as an uninstaller
/// leftover. The action's own category is appended instead, so the log says
/// which module — and, for Privacy, which consequence the user acknowledged —
/// authorized each line.
const GRANT_DIR_NOTE: &str = "user-granted directory, moved to the Trash as one recoverable unit";

/// The note a granted action carries, naming the category that authorized it.
fn grant_note(base: &str, category: &str) -> String {
    format!("{base} [{category}]")
}

/// Refusal reason for a *file* action that names a directory. See
/// [`authorize`] for why that is a blanket refusal rather than a
/// `safety::guard_dir` gate.
const DIRECTORY_REFUSAL: &str = "directory target; a file action cannot name a directory — \
     directories are planned as directory actions and need an explicit grant";

/// Refusal reason for a directory action nobody granted. See [`authorize_dir`].
const DIR_REQUIRES_GRANT: &str = "directory target; disposal requires an explicit per-path grant";

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
    // Directory targets are refused outright on *this* path, wherever the
    // authorization would have come from. One directory action stands for an
    // unknown number of files, so a check on the directory's own path is not
    // a check on what is about to be removed — the dangerous content is
    // inside it.
    //
    // Directories have their own path through the executor: a
    // `PlannedDirAction` carries a `SafeDir` from `safety::guard_dir` — the
    // tree walked in full, refused on a `.git` at any depth, failed closed on
    // anything unreadable or out of bounds — and is authorized by
    // `authorize_dir`, by explicit grant only. A *file* action that names a
    // directory is therefore always a mistake, and still refused here.
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

/// Decide whether a directory may be acted on: by explicit grant, and by
/// nothing else. Called *after* the pre-mutation re-walk, so `fresh` has
/// already survived the denylist at every depth.
///
/// There is deliberately no allowlist branch. A single file inside
/// `~/Library/Caches` is ordinary policy-driven cleanup; a whole directory
/// there is a tree nobody itemised, and the allowlist was never a statement
/// about trees. So a directory is disposable only because a human pointed at
/// exactly this path — and, as with file grants, exact equality confers
/// nothing on children or parents.
fn authorize_dir(fresh: &SafeDir, granted: &[SafeDir]) -> Result<(), &'static str> {
    if granted.iter().any(|g| g.as_path() == fresh.as_path()) {
        Ok(())
    } else {
        Err(DIR_REQUIRES_GRANT)
    }
}

/// The audit note an authorization deserves. `Refused` never reaches here —
/// refusals carry their own reason string through [`refuse`].
fn note_for(auth: Authorization, category: &str) -> Option<String> {
    match auth {
        Authorization::Granted => Some(grant_note(GRANT_NOTE, category)),
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

/// Record that an entire run was refused before it touched anything.
///
/// Item 6 wants a durable trace of refusals, and a wholesale refusal used to
/// leave none: the early `return Err(...)` happened before any `record` call,
/// so the most decisive thing the executor can do was the one thing the log
/// never mentioned.
fn refuse_run(audit: &mut AuditLog, reason: &str) -> Result<(), ExecError> {
    record(
        audit,
        Phase::Planned,
        Disposition::Refused,
        Path::new(WHOLE_RUN),
        0,
        None,
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
    entries: Option<u64>,
    note: Option<String>,
) -> Result<(), ExecError> {
    audit
        .record(&AuditEntry {
            epoch_ms: now_ms(),
            phase,
            disposition,
            path: path.display().to_string(),
            size_bytes,
            entries,
            note,
        })
        .map_err(ExecError::Audit)
}

/// Record a refusal and count it.
///
/// `phase` is the caller's, not a constant: a dry run must never write a line
/// claiming something was executed, and a refusal is no exception. (It used to
/// be — every preview refusal was logged as `executed`, and nothing pinned the
/// phase, so it went unnoticed until directory actions copied the pattern.)
fn refuse(
    report: &mut ExecReport,
    audit: &mut AuditLog,
    phase: Phase,
    path: &Path,
    size: u64,
    entries: Option<u64>,
    note: &str,
) -> Result<(), ExecError> {
    report.refused += 1;
    record(
        audit,
        phase,
        Disposition::Refused,
        path,
        size,
        entries,
        Some(note.to_string()),
    )
}

// ---------------------------------------------------------------------------
// Moving aside, and putting back
//
// The first mutation in this codebase that is neither a trash nor a disposal.
// It lives here rather than in a module of its own because the architecture
// says the executor is *the only mutator* — a `maintenance::` module that moved
// files would make that sentence false, and the sentence is how a reviewer
// knows where to look.
//
// What it deliberately does not share with disposal: no `Sink` (its two methods
// carry arguments about `unlink(2)` and `EPERM` that say nothing about a move,
// and every `execute` caller would inherit a capability none of them should
// have), no `Plan`, no `Consent`, and no bytes-freed figure — nothing is freed.
// ---------------------------------------------------------------------------

/// The most individually-picked plists one run may move.
///
/// The measured surface on a reference machine is five. More than sixty-four is
/// not a person ticking boxes, and an over-long list is refused wholesale
/// rather than truncated — truncating would act on a different set than the
/// caller asked for. Same reasoning as [`MAX_GRANTS`], smaller number, because
/// the population is smaller.
pub const MAX_STARTUP_GRANTS: usize = 64;

const STASH_NOTE: &str = "moved aside, reversibly — the file still exists at";
const RESTORE_NOTE: &str = "put back under the name it had, from";

#[derive(Debug)]
pub enum StashError {
    /// The home is not its canonical spelling, so the denylist's home-relative
    /// rules could not be trusted for this run.
    Home,
    /// The store is not a folder this app may use. Fail-closed, whole-run.
    Store(&'static str),
    TooManyGrants {
        count: usize,
        max: usize,
    },
    Audit(io::Error),
}

impl std::fmt::Display for StashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StashError::Home => write!(
                f,
                "the home directory is not its canonical spelling, so the denylist's \
                 home-relative rules could not be trusted for this run"
            ),
            StashError::Store(why) => write!(f, "the moved-aside folder {why}"),
            StashError::TooManyGrants { count, max } => write!(
                f,
                "{count} granted paths is more than the {max} this run allows"
            ),
            StashError::Audit(e) => write!(f, "the audit log could not be written: {e}"),
        }
    }
}

impl std::error::Error for StashError {}

/// Where a moved-aside file is created and where a name is removed.
///
/// Separate from [`Sink`] so no disposal caller inherits it, and a trait rather
/// than two free functions so the partial-failure paths are **testable** rather
/// than argued about. A link that succeeds followed by a removal that fails is
/// the one state this module must get right, and it cannot be provoked against
/// a real filesystem.
pub trait StashSink {
    /// Create a second name for the same file. Must **fail** if `to` exists,
    /// and must create nothing when it fails.
    fn link(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Remove one name. Never recursive.
    fn unlink(&self, path: &Path) -> io::Result<()>;
}

pub struct SystemStashSink;

impl StashSink for SystemStashSink {
    /// `hard_link` **is** the destination check.
    ///
    /// It fails with `EEXIST` and creates nothing, which is why there is no
    /// `if to.exists()` above it. A check-then-write is racy, and having one
    /// here would invite someone to later swap the atomic primitive for
    /// `rename`, which replaces an existing destination silently.
    fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
    }

    /// Files only, for the reason [`Sink::delete`] gives at length: the
    /// unavoidable check/use race fails closed on a directory.
    fn unlink(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
}

/// Consent for a reversible move. `Default` is a dry run granting nothing.
///
/// A separate type from [`Consent`], with no conversion in either direction.
/// That is what makes "a grant to move a plist aside cannot dispose of it" a
/// property of the types rather than a rule someone has to remember.
#[derive(Debug, Clone, Default)]
pub struct StashConsent {
    pub execute: bool,
    pub granted: Vec<SafePath>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StashReport {
    pub dry_run: bool,
    pub planned: usize,
    pub moved: usize,
    pub refused: usize,
}

/// The store must be a folder this app may use, checked once for the whole run.
///
/// Its parent being the LaunchAgents directory is not cosmetic: it is what lets
/// putting an item back need **no recorded state**, because the destination is
/// simply the store's own parent. A store anywhere else would need a manifest,
/// and a manifest is file content that names a path — the thing `uninstall` and
/// `privacy` both refuse to have.
fn validate_store(store: &Path, home: &Path) -> Result<PathBuf, StashError> {
    let agents = crate::loginitems::default_dir(home);
    let canonical_agents = std::fs::canonicalize(&agents)
        .map_err(|_| StashError::Store("is not beside a LaunchAgents folder this app can find"))?;

    match std::fs::symlink_metadata(store) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(StashError::Store("is a link to somewhere else"));
            }
            if !meta.is_dir() {
                return Err(StashError::Store("is not a folder"));
            }
            let canonical = std::fs::canonicalize(store)
                .map_err(|_| StashError::Store("could not be resolved"))?;
            if canonical != store {
                return Err(StashError::Store("is not its own canonical spelling"));
            }
            if canonical.parent() != Some(canonical_agents.as_path()) {
                return Err(StashError::Store("is not inside your LaunchAgents folder"));
            }
            Ok(canonical)
        }
        // Absent is fine: it is created on the first move, never by a scan.
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if store.parent() != Some(canonical_agents.as_path()) {
                return Err(StashError::Store("is not inside your LaunchAgents folder"));
            }
            Ok(store.to_path_buf())
        }
        Err(_) => Err(StashError::Store("could not be looked at")),
    }
}

/// What the store says about itself to someone who no longer has this app.
const STORE_NOTE_TEXT: &str = "\
These files were moved here by mac-cleaner so they would stop running when you
log in. Nothing was changed inside them and nothing was removed.

To put one back, drag it up one level into LaunchAgents, then log out and log
in again. That is all mac-cleaner does when you press Put back.

You can delete this note. You do not need mac-cleaner to undo any of this.
";

/// Move each planned file into `store`, reversibly.
///
/// The primitive is **link, verify, unlink** — never `rename`, which replaces
/// an existing destination silently, and never copy-then-remove, which has a
/// window in which the only whole copy is a partial one. The verify step
/// compares `(dev, ino)` of both names before either is removed, so the only
/// removal this module performs is of a name that provably shares an inode with
/// a second name created moments before.
///
/// **This module cannot lose a file's bytes.** Every failure lands on "nothing
/// happened" or "two names for one file", never on "no names".
pub fn stash(
    plan: &StashPlan,
    consent: StashConsent,
    store: &Path,
    home: &Path,
    sink: &dyn StashSink,
    audit: &mut AuditLog,
) -> Result<StashReport, StashError> {
    move_files(plan, consent, store, home, sink, audit, Direction::Aside)
}

/// Put each planned file back under the name it had.
///
/// The destination is the store's own parent, which is why this needs no
/// recorded state: nothing has to remember where a file came from, so no file's
/// contents ever name a destination.
pub fn restore(
    plan: &StashPlan,
    consent: StashConsent,
    store: &Path,
    home: &Path,
    sink: &dyn StashSink,
    audit: &mut AuditLog,
) -> Result<StashReport, StashError> {
    move_files(plan, consent, store, home, sink, audit, Direction::Back)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Aside,
    Back,
}

fn move_files(
    plan: &StashPlan,
    consent: StashConsent,
    store: &Path,
    home: &Path,
    sink: &dyn StashSink,
    audit: &mut AuditLog,
    direction: Direction,
) -> Result<StashReport, StashError> {
    match safety::canonical_home(home) {
        Ok(canonical) if canonical == home => {}
        _ => return Err(StashError::Home),
    }
    if consent.granted.len() > MAX_STARTUP_GRANTS {
        return Err(StashError::TooManyGrants {
            count: consent.granted.len(),
            max: MAX_STARTUP_GRANTS,
        });
    }
    let store = validate_store(store, home)?;
    let agents = std::fs::canonicalize(crate::loginitems::default_dir(home))
        .map_err(|_| StashError::Store("is not beside a LaunchAgents folder this app can find"))?;

    let (from_dir, to_dir) = match direction {
        Direction::Aside => (agents.clone(), store.clone()),
        Direction::Back => (store.clone(), agents.clone()),
    };

    let mut report = StashReport {
        dry_run: !consent.execute,
        ..Default::default()
    };
    let phase = if consent.execute {
        Phase::Executed
    } else {
        Phase::Planned
    };

    for m in &plan.moves {
        let path = m.path.as_path();
        if let Err(why) = vet(path, &from_dir, &consent) {
            report.refused += 1;
            record(
                audit,
                phase,
                Disposition::Refused,
                path,
                m.size_bytes,
                None,
                Some(why.to_string()),
            )
            .map_err(audit_error)?;
            continue;
        }

        // `file_name` off an already-guarded, already-canonical path: no
        // separator, nothing parsed, nothing a caller chose.
        let Some(name) = path.file_name() else {
            report.refused += 1;
            continue;
        };
        let dest = to_dir.join(name);

        if !consent.execute {
            report.planned += 1;
            record(
                audit,
                Phase::Planned,
                disposition_of(direction),
                path,
                m.size_bytes,
                None,
                Some(move_note(direction, &dest, &m.category)),
            )
            .map_err(audit_error)?;
            continue;
        }

        if direction == Direction::Aside && !store.exists() {
            if std::fs::create_dir_all(&store).is_err() {
                report.refused += 1;
                continue;
            }
            // A courtesy, not a safety property: `create_new`, so a note the
            // user has edited is never overwritten, and a failure to write it
            // is not fatal because the folder works without it.
            let _ = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(store.join(crate::loginitems::STORE_NOTE_NAME))
                .and_then(|mut f| {
                    use std::io::Write;
                    f.write_all(STORE_NOTE_TEXT.as_bytes())
                });
        }

        match link_verify_unlink(path, &dest, sink) {
            Outcome::Moved => {
                report.moved += 1;
                record(
                    audit,
                    Phase::Executed,
                    disposition_of(direction),
                    path,
                    m.size_bytes,
                    None,
                    Some(move_note(direction, &dest, &m.category)),
                )
                .map_err(audit_error)?;
            }
            Outcome::Untouched(why) => {
                report.refused += 1;
                record(
                    audit,
                    Phase::Executed,
                    Disposition::Refused,
                    path,
                    m.size_bytes,
                    None,
                    Some(why),
                )
                .map_err(audit_error)?;
            }
            // Two names for one file. Both lines are true: the copy exists, and
            // the original is still there, so the item still runs at login.
            // Neither line alone would describe the disk.
            Outcome::BothRemain(why) => {
                report.refused += 1;
                record(
                    audit,
                    Phase::Executed,
                    disposition_of(direction),
                    &dest,
                    m.size_bytes,
                    None,
                    Some(move_note(direction, &dest, &m.category)),
                )
                .map_err(audit_error)?;
                record(
                    audit,
                    Phase::Executed,
                    Disposition::Refused,
                    path,
                    m.size_bytes,
                    None,
                    Some(why),
                )
                .map_err(audit_error)?;
            }
        }
    }

    Ok(report)
}

enum Outcome {
    Moved,
    /// Nothing changed on disk.
    Untouched(String),
    /// The second name was created and the first could not be removed.
    BothRemain(String),
}

/// Link, verify, unlink. See [`stash`] for why it is these three, in this
/// order, and no others.
fn link_verify_unlink(from: &Path, to: &Path, sink: &dyn StashSink) -> Outcome {
    if let Err(e) = sink.link(from, to) {
        return Outcome::Untouched(format!("it could not be moved aside: {e}"));
    }

    // The window that exists nowhere else in this codebase: between creating
    // the second name and removing the first, something could have replaced the
    // source. Compare *identity*, not paths — and `symlink_metadata`, so a
    // symlink swapped in reports itself rather than whatever it points at.
    let same = match (
        std::fs::symlink_metadata(from),
        std::fs::symlink_metadata(to),
    ) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    };
    if !same {
        // Refuse to remove a name we cannot prove we copied. Roll the new one
        // back if we can; either way the source is left exactly as it is.
        let _ = sink.unlink(to);
        return Outcome::Untouched(
            "it was replaced while being moved, so it was left exactly as it is".to_string(),
        );
    }

    match sink.unlink(from) {
        Ok(()) => Outcome::Moved,
        Err(e) => Outcome::BothRemain(format!(
            "the copy was made but the original could not be removed, so this still runs at \
             login: {e}"
        )),
    }
}

/// Every check that must pass before a file is touched, in order. Each can only
/// narrow what came before it.
fn vet(path: &Path, from_dir: &Path, consent: &StashConsent) -> Result<(), &'static str> {
    if !consent.granted.iter().any(|g| g.as_path() == path) {
        return Err("nobody granted this path");
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| "it could not be looked at")?;
    if meta.file_type().is_symlink() {
        return Err("it is a link to somewhere else");
    }
    if !meta.is_file() {
        return Err("it is not a regular file");
    }
    if path.extension().and_then(|e| e.to_str()) != Some("plist") {
        return Err("it is not a .plist");
    }
    // The path must already BE its canonical self. `guard` canonicalizes, so a
    // symlinked plist arrives here resolved to its *target*, and this equality
    // is the only thing that refuses it.
    match std::fs::canonicalize(path) {
        Ok(c) if c == path => {}
        _ => return Err("it does not resolve to itself"),
    }
    // Exactly this directory, never `starts_with`: launchd does not recurse
    // into a subfolder, and neither should the offer.
    if path.parent() != Some(from_dir) {
        return Err("it is not in the folder this run acts on");
    }
    Ok(())
}

fn disposition_of(direction: Direction) -> Disposition {
    match direction {
        Direction::Aside => Disposition::Stashed,
        Direction::Back => Disposition::Restored,
    }
}

fn move_note(direction: Direction, other: &Path, category: &str) -> String {
    let base = match direction {
        Direction::Aside => STASH_NOTE,
        Direction::Back => RESTORE_NOTE,
    };
    format!("{base} {} [{category}]", other.display())
}

fn audit_error(e: ExecError) -> StashError {
    match e {
        ExecError::Audit(io) => StashError::Audit(io),
        other => StashError::Audit(io::Error::other(other.to_string())),
    }
}
