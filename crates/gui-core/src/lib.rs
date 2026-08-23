//! Frontend-facing command layer for the mac-cleaner GUI.
//!
//! Thin, **tested** wrappers over `macclean-core` that take serde-friendly
//! inputs from the UI and return serializable DTOs. The Tauri shell's
//! `#[tauri::command]` functions delegate straight to these, so all GUI
//! behaviour is covered by ordinary `cargo test` (no webview needed).
//!
//! Crucially, this layer adds **no new deletion logic** — `clean_with_sink`
//! routes through `macclean-core`'s consent-gated `executor::execute`, so the
//! dry-run default, Trash-first disposal, mass-delete confirmation, and audit
//! log all still apply exactly as in the CLI.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{execute, Consent, Sink, SystemSink};
use macclean_core::loginitems::{self, LoginItem};
use macclean_core::report::ScanReport;
use macclean_core::scanner::{scan, scan_with_progress, Progress, ScanConfig};

/// Scan/clean filters as the frontend sends them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Filters {
    /// Only consider files not modified in the last N days.
    pub older_than_days: Option<u64>,
    /// Only consider files at least this many bytes.
    pub min_size_bytes: Option<u64>,
}

/// What the confirmation sheet actually showed the user.
///
/// The plan is rebuilt at execute time (the disk may have changed since the
/// preview), so the freshly-scanned plan is checked against these numbers. If
/// it has grown materially the user's consent no longer describes it, and we
/// refuse rather than quietly remove more than they agreed to.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Expected {
    pub count: usize,
    pub bytes: u64,
}

/// Growth allowance between preview and execute.
///
/// Caches genuinely churn in the seconds a user spends reading the sheet, so
/// exact equality would refuse constantly. These absorb ordinary churn while
/// still catching a plan that has become materially different from the one that
/// was confirmed.
const CHURN_ITEMS: usize = 25;
const CHURN_BYTES: u64 = 64 * 1024 * 1024;

fn grew_beyond(fresh_count: usize, fresh_bytes: u64, expected: Expected) -> bool {
    let count_cap = expected
        .count
        .saturating_add(CHURN_ITEMS.max(expected.count / 10));
    let bytes_cap = expected
        .bytes
        .saturating_add(CHURN_BYTES.max(expected.bytes / 10));
    fresh_count > count_cap || fresh_bytes > bytes_cap
}

/// The serializable outcome of a clean run, for the UI to display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CleanSummary {
    pub dry_run: bool,
    pub executed: usize,
    pub refused: usize,
    pub bytes_freed: u64,
}

/// Resolve and canonicalize the real home directory for the running app.
///
/// Used by the Tauri shell; the testable functions above take `home` explicitly
/// so tests never touch the real filesystem.
pub fn default_home() -> std::io::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| std::io::Error::other("cannot determine home directory"))?;
    safety::canonical_home(&home)
}

/// Build a [`ScanConfig`] for `home` from UI filters.
pub fn build_config(home: &Path, filters: &Filters) -> ScanConfig {
    let mut cfg = ScanConfig::with_default_roots(home.to_path_buf());
    if let Some(days) = filters.older_than_days {
        cfg = cfg.older_than(Duration::from_secs(days.saturating_mul(86_400)));
    }
    if let Some(bytes) = filters.min_size_bytes {
        cfg = cfg.min_size(bytes);
    }
    cfg
}

/// Read-only: build a scan report for the UI.
pub fn scan_report(home: &Path, filters: &Filters) -> ScanReport {
    ScanReport::from_plan_without_items(&scan(&build_config(home, filters)))
}

/// Read-only: build a scan report, reporting progress as the walk proceeds.
///
/// The GUI runs this off the UI thread and forwards each update to the webview,
/// so a multi-second scan shows real movement instead of a static skeleton.
pub fn scan_report_with_progress(
    home: &Path,
    filters: &Filters,
    on_progress: &mut dyn FnMut(Progress),
) -> ScanReport {
    let plan = scan_with_progress(&build_config(home, filters), on_progress);
    ScanReport::from_plan_without_items(&plan)
}

/// Read-only: list login items for the UI.
pub fn list_login_items(home: &Path) -> Vec<LoginItem> {
    loginitems::scan_dir(&loginitems::default_dir(home))
}

/// Consent for the GUI: always move to the Trash (recoverable), never permanent.
/// `confirm_mass_delete` carries the user's explicit confirmation from the modal.
pub fn gui_consent(confirm_mass_delete: bool) -> Consent {
    Consent {
        execute: true,
        allow_permanent: false,
        confirmed_mass_delete: confirm_mass_delete,
    }
}

/// Default audit-log path for the app (parent created if missing).
pub fn default_audit_path() -> std::io::Result<PathBuf> {
    let dir = default_home()?.join("Library/Application Support/macclean");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("audit.jsonl"))
}

/// Clean using an explicit [`Sink`] and audit log. The sink is injectable so
/// tests run against a throwaway directory; the real app passes `SystemSink`.
///
/// `categories` restricts the action to those category ids (`None` = all matched
/// by `filters`); this is how the UI's per-category selection is honored — the
/// plan is filtered *before* it reaches the consent-gated executor, so nothing
/// outside the selection is ever acted on.
///
/// Returns the refusal reason (e.g. unconfirmed mass delete) as `Err` so the UI
/// can surface it instead of silently doing nothing.
pub fn clean_with_sink(
    home: &Path,
    filters: &Filters,
    categories: Option<&[String]>,
    expected: Option<Expected>,
    consent: Consent,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<CleanSummary, String> {
    let mut plan = scan(&build_config(home, filters));
    if let Some(cats) = categories {
        plan.actions
            .retain(|a| cats.iter().any(|c| c == &a.category));
    }
    // Bind the consent to a magnitude: refuse if the rebuilt plan is materially
    // bigger than the one the user was shown.
    if let Some(exp) = expected {
        if grew_beyond(plan.count(), plan.total_bytes(), exp) {
            return Err(format!(
                "refused: the disk changed since the preview. This would now remove \
                 {} items ({} bytes), but you confirmed {} items ({} bytes). \
                 Scan again and review before cleaning.",
                plan.count(),
                plan.total_bytes(),
                exp.count,
                exp.bytes
            ));
        }
    }
    match execute(&plan, consent, home, sink, audit) {
        Ok(report) => Ok(CleanSummary {
            dry_run: report.dry_run,
            executed: report.executed,
            refused: report.refused,
            bytes_freed: report.bytes_executed,
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Real-app clean: scan with `filters`, restrict to the selected `categories`,
/// then move matches to the Trash via the consent-gated executor, recording to
/// the default audit log. Never deletes permanently.
///
/// `categories` is always applied as a filter, including when it is empty:
/// **selecting nothing disposes of nothing.** This used to map an empty list to
/// `None` ("no filter", i.e. every category), which is fail-open — a UI that
/// lost its selection could present a confirmation sheet reading "Move 0 items"
/// and then carry out an unrestricted clean. The caller must name what it wants
/// removed.
pub fn clean(
    filters: &Filters,
    categories: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    let path = default_audit_path().map_err(|e| e.to_string())?;
    clean_at(
        &home,
        &path,
        filters,
        categories,
        expected,
        confirm_mass_delete,
        &SystemSink,
    )
}

/// The testable core of [`clean`]: everything except locating the real home and
/// audit log.
///
/// This exists so the `Vec<String> -> Option<&[String]>` decision — the one that
/// used to turn an empty selection into "every category" — sits behind a seam a
/// fixture test can reach. [`clean`] itself resolves the *real* home, so it can
/// never be exercised in a test, which is precisely how that fail-open survived.
pub fn clean_at(
    home: &Path,
    audit_path: &Path,
    filters: &Filters,
    categories: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
    sink: &dyn Sink,
) -> Result<CleanSummary, String> {
    let mut audit = AuditLog::open(audit_path).map_err(|e| e.to_string())?;
    clean_with_sink(
        home,
        filters,
        Some(categories.as_slice()),
        expected,
        gui_consent(confirm_mass_delete),
        sink,
        &mut audit,
    )
}

/// Which TCC-gated roots the app can actually read right now.
///
/// `~/.Trash` and `~/Library/Containers` are protected by macOS TCC. Without
/// access a scan still succeeds — it simply cannot see inside them, so it
/// reports less than is really there. That is the same class of problem as the
/// fixture fallback removed in v0.3: a figure the user trusts that does not
/// describe their disk. This probe exists so the UI can say so out loud rather
/// than quietly under-reporting.
///
/// The names are deliberately about *reading*, not about the Full Disk Access
/// toggle. We can observe whether a directory opened; we cannot observe the
/// user's TCC settings, and claiming otherwise would be a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Permissions {
    pub trash_readable: bool,
    pub containers_readable: bool,
    /// Every gated root opened successfully.
    pub all_readable: bool,
}

/// Read-only. Opens two directories and reports whether that worked. Never
/// constructs a `SafePath`, never reaches the executor, never writes.
pub fn probe_permissions(home: &Path) -> Permissions {
    let trash_readable = is_readable(&home.join(".Trash"));
    let containers_readable = is_readable(&home.join("Library/Containers"));
    Permissions {
        trash_readable,
        containers_readable,
        all_readable: trash_readable && containers_readable,
    }
}

/// A directory that is absent is not a permission problem — reporting one would
/// send the user to System Settings to fix something that is not broken.
fn is_readable(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(_) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}
