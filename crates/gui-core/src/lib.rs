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
use macclean_core::scanner::{scan, ScanConfig};

/// Scan/clean filters as the frontend sends them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Filters {
    /// Only consider files not modified in the last N days.
    pub older_than_days: Option<u64>,
    /// Only consider files at least this many bytes.
    pub min_size_bytes: Option<u64>,
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
    ScanReport::from_plan(&scan(&build_config(home, filters)))
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
    consent: Consent,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<CleanSummary, String> {
    let mut plan = scan(&build_config(home, filters));
    if let Some(cats) = categories {
        plan.actions
            .retain(|a| cats.iter().any(|c| c == &a.category));
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

/// Real-app clean: scan with `filters`, restrict to `categories` (empty = all),
/// then move matches to the Trash via the consent-gated executor, recording to
/// the default audit log. Never deletes permanently.
pub fn clean(
    filters: &Filters,
    categories: Vec<String>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    let path = default_audit_path().map_err(|e| e.to_string())?;
    let mut audit = AuditLog::open(&path).map_err(|e| e.to_string())?;
    let cats = if categories.is_empty() {
        None
    } else {
        Some(categories.as_slice())
    };
    clean_with_sink(
        &home,
        filters,
        cats,
        gui_consent(confirm_mass_delete),
        &SystemSink,
        &mut audit,
    )
}
