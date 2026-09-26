//! Empty Trash: the second, permanent stage after a recoverable clean.
//!
//! Everything else the GUI does moves files *into* the Trash, so a mistake can
//! be put back. This is the one gesture that gives that up, and it is shaped so
//! that it cannot be mistaken for anything else:
//!
//! - **Its own command.** It is never a category in [`crate::clean`], which
//!   refuses a request naming the Trash, so it can never ride along with a
//!   recoverable clean in one click.
//! - **Explicit acknowledgement.** The request must carry
//!   `acknowledged_unrecoverable: true` — the user ticked a box saying the files
//!   cannot be recovered and that they proceed at their own risk. Absent means
//!   not given, and the run is refused and recorded.
//! - **Bound to exactly what was shown.** A clean's figures tolerate churn
//!   because caches change by themselves; the Trash does not, and a count and
//!   size cannot tell a swap from no change at all. So the request carries the
//!   plan's fingerprint (see `EmptyTrashPlan::fingerprint`) and any change —
//!   a file added, removed, replaced or rewritten — refuses the run.
//! - **The consent is built here and nowhere else.** [`permanent_consent`] is
//!   private: no other command can obtain `allow_permanent`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use swept_core::audit::AuditLog;
use swept_core::emptytrash::{self, EMPTIED_CATEGORY};
use swept_core::executor::{execute, prune_emptied_trash_dirs, Consent, Sink, SystemSink};
use swept_core::plan::strictly_inside_trash;

use crate::{default_audit_path, default_home, refuse_and_record, Expected};

/// What is in the Trash right now. Read-only.
#[derive(Debug, Clone, Serialize)]
pub struct TrashContents {
    /// False when the Trash exists but cannot be read — no Full Disk Access.
    /// Every figure below is then zero and means nothing.
    pub readable: bool,
    /// Top-level entries: what Finder would show.
    pub items: usize,
    /// Files that would be removed.
    pub files: usize,
    /// Folders that would be removed once empty.
    pub folders: usize,
    pub bytes: u64,
    /// Top-level entries that would be left whole: a link, a repository, a
    /// locked or unreadable file somewhere inside.
    pub left_behind: usize,
    /// Whether files and folders together, or the size, cross the mass-delete
    /// threshold, so the frontend sends `confirm_mass_delete` from the figures
    /// it displayed.
    pub requires_confirmation: bool,
    /// Identifies exactly what would be removed. A request must echo it back.
    pub fingerprint: String,
}

/// The executor's mass-delete gate counts files; pruned folders are removed
/// irreversibly too, so here they count alongside them.
fn needs_confirmation(p: &emptytrash::EmptyTrashPlan) -> bool {
    p.files.requires_confirmation()
        || p.files.count() + p.prune.len() > swept_core::plan::MASS_DELETE_COUNT
}

/// Read what emptying the Trash would do, without doing it.
pub fn trash_contents(home: &Path) -> TrashContents {
    let p = emptytrash::plan(home);
    TrashContents {
        readable: !p.unreadable_root,
        items: p.items,
        files: p.files.count(),
        folders: p.prune.len(),
        bytes: p.files.total_bytes(),
        left_behind: p.left_behind,
        requires_confirmation: needs_confirmation(&p),
        fingerprint: p.fingerprint(),
    }
}

/// The frontend's request to empty the Trash.
///
/// `deny_unknown_fields`, and neither `expected` nor `fingerprint` has a
/// default: a request that does not say what it was shown is not a request
/// this command understands.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyTrashRequest {
    /// The files-plus-folders count and bytes the dialog showed. Used for the
    /// refusal messages and the empty check; the binding is `fingerprint`.
    pub expected: Expected,
    /// [`TrashContents::fingerprint`] from the preview the dialog showed.
    pub fingerprint: String,
    /// The user ticked "I understand these files will be permanently deleted".
    #[serde(default)]
    pub acknowledged_unrecoverable: bool,
    /// The user confirmed a mass delete (derived from the displayed figures).
    #[serde(default)]
    pub confirm_mass_delete: bool,
}

/// What emptying the Trash did. Its own type, so the result can never be shown
/// with a clean's "moved to the Trash" wording.
#[derive(Debug, Clone, Serialize)]
pub struct EmptyTrashSummary {
    pub files_deleted: usize,
    pub folders_removed: usize,
    pub refused: usize,
    /// Logical size of the files removed. Not necessarily disk space freed:
    /// hard links and clones share their blocks.
    pub bytes_deleted: u64,
    /// Top-level entries deliberately left whole: a link, a repository, or
    /// something unreadable inside. Refused files are counted in `refused`.
    pub left_behind: usize,
}

/// The only place a GUI consent permits irreversible removal. Private.
fn permanent_consent(confirm_mass_delete: bool) -> Consent {
    Consent {
        execute: true,
        allow_permanent: true,
        confirmed_mass_delete: confirm_mass_delete,
        granted: Vec::new(),
        granted_dirs: Vec::new(),
    }
}

/// Empty the Trash at `home` through `sink`, recording to `audit`.
pub fn empty_trash_with_sink(
    home: &Path,
    req: &EmptyTrashRequest,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<EmptyTrashSummary, String> {
    if !req.acknowledged_unrecoverable {
        return refuse_and_record(
            audit,
            "refused: emptying the Trash needs you to acknowledge that the files cannot be \
             recovered."
                .to_string(),
        );
    }
    if req.expected.count == 0 {
        return refuse_and_record(
            audit,
            "refused: the Trash was shown as empty, so there is nothing you agreed to remove."
                .to_string(),
        );
    }
    let plan = emptytrash::plan(home);
    if plan.unreadable_root {
        return refuse_and_record(
            audit,
            "refused: Swept cannot read the Trash. Give it Full Disk Access in System \
             Settings, or empty the Trash in Finder."
                .to_string(),
        );
    }
    // Confinement, checked on the plan the run will actually use rather than
    // trusted from the planner: every file inside the Trash, every one tagged
    // as this gesture's.
    if let Some(a) =
        plan.files.actions.iter().find(|a| {
            !strictly_inside_trash(a.path.as_path(), home) || a.category != EMPTIED_CATEGORY
        })
    {
        return refuse_and_record(
            audit,
            format!(
                "refused: {} is not in the Trash; nothing was removed.",
                a.path.as_path().display()
            ),
        );
    }
    // Exactly what was shown, not merely no more of it. See the module doc.
    if plan.fingerprint() != req.fingerprint {
        let (count, bytes) = (
            plan.files.count() + plan.prune.len(),
            plan.files.total_bytes(),
        );
        return refuse_and_record(
            audit,
            format!(
                "refused: the Trash changed since you reviewed it. It now holds {count} items \
                 ({bytes} bytes); you confirmed {} items ({} bytes), and even equal figures \
                 can hide a different file. Review it again before emptying.",
                req.expected.count, req.expected.bytes
            ),
        );
    }
    // Folders count here too: the executor's gate only sees files.
    if needs_confirmation(&plan) && !req.confirm_mass_delete {
        return refuse_and_record(
            audit,
            format!(
                "refused: permanently removing {} files and {} folders needs the mass-delete \
                 confirmation.",
                plan.files.count(),
                plan.prune.len()
            ),
        );
    }

    let consent = permanent_consent(req.confirm_mass_delete);
    let files =
        execute(&plan.files, consent.clone(), home, sink, audit).map_err(|e| e.to_string())?;
    let folders = prune_emptied_trash_dirs(&plan.prune, &consent, home, sink, audit)
        .map_err(|e| e.to_string())?;

    Ok(EmptyTrashSummary {
        files_deleted: files.executed,
        folders_removed: folders.removed,
        refused: files.refused + folders.refused,
        bytes_deleted: files.bytes_executed,
        left_behind: plan.left_behind,
    })
}

/// [`empty_trash_with_sink`] with its own audit log at `audit_path`.
pub fn empty_trash_at(
    home: &Path,
    audit_path: &Path,
    req: &EmptyTrashRequest,
    sink: &dyn Sink,
) -> Result<EmptyTrashSummary, String> {
    let mut audit = AuditLog::open(audit_path).map_err(|e| e.to_string())?;
    empty_trash_with_sink(home, req, sink, &mut audit)
}

/// Empty the real Trash. The Tauri command's body.
pub fn empty_trash(req: &EmptyTrashRequest) -> Result<EmptyTrashSummary, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    let path = default_audit_path().map_err(|e| e.to_string())?;
    empty_trash_at(&home, &path, req, &SystemSink)
}

/// [`trash_contents`] for the real home. The Tauri command's body.
pub fn real_trash_contents() -> Result<TrashContents, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    Ok(trash_contents(&home))
}
