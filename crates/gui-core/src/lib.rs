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

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{execute, Consent, Sink, SystemSink};
use macclean_core::largeold;
use macclean_core::loginitems::{self, LoginItem};
use macclean_core::plan::{Disposal, Plan, PlannedAction, PlannedDirAction};
use macclean_core::report::ScanReport;
use macclean_core::scanner::{scan, scan_with_progress, Progress, ScanConfig};
use macclean_core::spacelens;
use macclean_core::uninstall::{
    self, BundleId, Candidate, DisplayName, Kind, LeftoverReport, MatchedVia, Residence,
    UninstallConfig, UninstallError,
};
use safety::DirLimits;

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
    /// Names removed by directory actions. `executed` counts a directory as
    /// one action; this is how many files that one action stood for.
    pub entries_freed: u64,
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
///
/// `granted` is empty and stays empty until a module actually offers per-path
/// selection outside the allowlist (Large & Old Files, M2). The GUI's current
/// clean flow works entirely inside `allowlist::default_roots`, so there is
/// nothing to grant — and an empty list is the honest way to say so.
pub fn gui_consent(confirm_mass_delete: bool) -> Consent {
    Consent {
        execute: true,
        allow_permanent: false,
        confirmed_mass_delete: confirm_mass_delete,
        granted: Vec::new(),
        granted_dirs: Vec::new(),
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
            entries_freed: report.entries_executed,
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

// ---------------------------------------------------------------------------
// Large & Old Files
//
// The first feature that looks outside the disposal allowlist. Everything here
// keeps the M1 spine intact: the walk yields paths to *show*, and turning one
// into an action takes a separate per-path grant that runs through `guard` at
// that moment.
// ---------------------------------------------------------------------------

/// Refuse, and leave a record of having refused.
///
/// The command layer rejects several things before the executor is ever
/// reached — an empty selection, an item that is no longer what was listed, a
/// selection that drifted. Those used to return `Err` and write nothing, which
/// is the same gap `executor::record_run_refusal` exists to close: a frontend
/// sending a protected or foreign path is exactly the signal worth having in
/// the log, and it was the one thing the log never mentioned.
fn refuse_and_record(audit: &mut AuditLog, reason: String) -> Result<CleanSummary, String> {
    match macclean_core::executor::record_run_refusal(audit, &reason) {
        Ok(()) => Err(reason),
        // Still refusing either way; say that the record failed too rather than
        // reporting a clean refusal that left no trace.
        Err(e) => Err(format!(
            "{reason} (and the audit log could not be written: {e})"
        )),
    }
}

/// Byte tolerance for a hand-picked *selection*.
///
/// Deliberately **not** [`CHURN_BYTES`]. That allowance is 64 MiB because a
/// rebuilt cache scan genuinely churns in the seconds a user spends reading a
/// sheet. A selection has no such churn: it is a fixed list of files the user
/// pointed at, and Large & Old only shows files of 100 MiB and up — so a
/// 64 MiB slack would be wide enough for a materially different file to pass
/// as "the same one". The only legitimate drift is a file that appended while
/// the sheet was open.
const SELECTION_CHURN_BYTES: u64 = 1024 * 1024;

/// Why a selection no longer matches what was confirmed, if it does not.
///
/// The count must match **exactly**: unlike a rescan, the set here is the list
/// of paths the caller sent, so any difference means the UI and the backend
/// disagree about what was chosen — which is precisely the state in which
/// nothing should be acted on.
fn selection_drifted(fresh_count: usize, fresh_bytes: u64, expected: Expected) -> Option<String> {
    if fresh_count != expected.count {
        return Some("the selection is not the one you confirmed".to_string());
    }
    if fresh_bytes > expected.bytes.saturating_add(SELECTION_CHURN_BYTES) {
        return Some("the selected files grew since the preview".to_string());
    }
    None
}

/// One row in the Large & Old list.
///
/// Note what is absent: there is no `selected` field and no default selection.
/// These are never pre-ticked and never part of a Smart Scan default — the
/// whole point is that a human chooses each one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LargeOldItem {
    pub path: String,
    pub size_bytes: u64,
    pub modified_ms: Option<u64>,
}

/// What the Large & Old walk found, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LargeOldReportDto {
    pub items: Vec<LargeOldItem>,
    /// Total matches, which may exceed `items.len()` when the list is capped.
    pub matched: usize,
    pub matched_bytes: u64,
    pub examined: usize,
    pub truncated: bool,
    pub skipped_unreadable: usize,
    pub skipped_hardlinked: usize,
    pub skipped_unrepresentable: usize,
    /// True when the report describes less than the whole disk, for any
    /// reason. The UI must say so rather than presenting a figure as complete.
    pub partial: bool,
}

/// Read-only: find large (optionally old) files across the discovery scope.
pub fn large_and_old(
    home: &Path,
    min_size_bytes: u64,
    older_than_days: Option<u64>,
) -> LargeOldReportDto {
    // `saturating_mul`, matching `build_config`. An unchecked multiply here
    // panics in debug and *wraps* in release, and a wrapped age threshold turns
    // "older than N days" into a near-zero one — presenting freshly-modified
    // files to the user as old, which are then exactly the rows they grant.
    let min_age = older_than_days.map(|d| Duration::from_secs(d.saturating_mul(86_400)));
    let report = largeold::find_in(home, min_size_bytes, min_age);
    LargeOldReportDto {
        items: report
            .items
            .iter()
            // `to_str`, not `display()`. The disposal path identifies a
            // selection by requiring the string it receives back to equal the
            // path emitted here, so a lossy conversion would break that
            // identity check — and in a collision it could not detect the
            // break. The walk already drops non-UTF-8 names, making this
            // filter_map total rather than a second policy.
            .filter_map(|f| {
                f.path.to_str().map(|p| LargeOldItem {
                    path: p.to_string(),
                    size_bytes: f.size_bytes,
                    modified_ms: f.modified_ms,
                })
            })
            .collect(),
        matched: report.matched,
        matched_bytes: report.matched_bytes,
        examined: report.examined,
        truncated: report.truncated,
        skipped_unreadable: report.skipped_unreadable,
        skipped_hardlinked: report.skipped_hardlinked,
        skipped_unrepresentable: report.skipped_unrepresentable,
        partial: report.is_partial(),
    }
}

/// One rectangle in the Space Lens treemap.
///
/// Like [`LargeOldItem`] there is no `selected` field, but for a stronger
/// reason: there is no command that takes one of these. Space Lens is a picture
/// of the disk, and a picture is not a proposal — nothing in this DTO can be
/// handed back to the backend to act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpaceNodeDto {
    pub name: String,
    /// `None` for a rollup node (not a place on disk) and for a name that is
    /// not valid UTF-8. The node is still drawn with its real size — see the
    /// `spacelens` module docs on why lossy names are acceptable *here* and
    /// not in Large & Old.
    pub path: Option<String>,
    pub bytes: u64,
    pub files: u64,
    pub is_dir: bool,
    /// True when `children` is not a complete listing of what is inside.
    pub collapsed: bool,
    pub children: Vec<SpaceNodeDto>,
}

/// The measured tree, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpaceLensReportDto {
    pub roots: Vec<SpaceNodeDto>,
    pub total_bytes: u64,
    pub total_files: u64,
    pub examined: usize,
    pub truncated: bool,
    pub skipped_unreadable: usize,
    pub skipped_too_deep: usize,
    /// How many nodes this payload actually contains.
    pub nodes: usize,
    /// True if the tree stopped growing at the node budget. Like the depth cap
    /// this stops the drawing, not the measuring — so it is **not** a reason
    /// for `partial`; the affected directories are marked `collapsed` instead.
    pub node_budget_reached: bool,
    /// Files reached through more than one hard link, counted once. Not a
    /// reason for `partial` — it makes the total more accurate, not less.
    pub deduped_hardlinks: usize,
    /// True when the tree describes less than what is on disk, for any reason.
    /// The UI must present the total as a floor when this is set.
    pub partial: bool,
}

/// Read-only: measure the discovery scope for the treemap.
///
/// Authorizes nothing and returns nothing actionable.
pub fn space_lens(home: &Path) -> SpaceLensReportDto {
    let report = spacelens::measure_in(home);
    SpaceLensReportDto {
        roots: report.roots.iter().map(space_node).collect(),
        total_bytes: report.total_bytes,
        total_files: report.total_files,
        examined: report.examined,
        truncated: report.truncated,
        skipped_unreadable: report.skipped_unreadable,
        skipped_too_deep: report.skipped_too_deep,
        nodes: report.nodes,
        node_budget_reached: report.node_budget_reached,
        deduped_hardlinks: report.deduped_hardlinks,
        partial: report.is_partial(),
    }
}

fn space_node(node: &spacelens::Node) -> SpaceNodeDto {
    SpaceNodeDto {
        name: node.name.clone(),
        // `and_then(to_str)`, so a path that cannot round-trip becomes `None`
        // rather than a lossy string. The walk already declines to address such
        // a node, and this is the second half of that same decision: a name to
        // draw, never an address to trust.
        path: node
            .path
            .as_deref()
            .and_then(|p| p.to_str())
            .map(String::from),
        bytes: node.bytes,
        files: node.files,
        is_dir: node.is_dir,
        collapsed: node.collapsed,
        children: node.children.iter().map(space_node).collect(),
    }
}

/// Act on individually-chosen paths, using per-path grants.
///
/// This is the only caller that populates `Consent::granted`, and it is
/// deliberately strict, because the safety argument for grants rests on each
/// one being a real, individual, still-valid human choice:
///
/// - **An empty selection acts on nothing.** Same rule as `clean`: the caller
///   must name what it wants.
/// - **Every path is re-`guard`ed here**, so the denylist decides before
///   anything else. A path the guard refuses fails the *whole* request rather
///   than being skipped, because a partial run does not match the list the user
///   was looking at when they confirmed.
/// - **Sizes are re-read from disk, never taken from the frontend.** Trusting
///   caller-supplied sizes would let an understated total slip past the
///   mass-delete threshold.
/// - The executor still applies every other bound: exact-match grants, the
///   `MAX_GRANTS` cap, the directory refusal, Trash-not-unlink, and the audit
///   note marking each action as user-granted.
pub fn dispose_selected_with_sink(
    home: &Path,
    paths: &[String],
    expected: Option<Expected>,
    confirm_mass_delete: bool,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<CleanSummary, String> {
    if paths.is_empty() {
        return refuse_and_record(audit, "refused: nothing was selected.".to_string());
    }

    let mut actions = Vec::with_capacity(paths.len());
    let mut granted = Vec::with_capacity(paths.len());
    let mut rejected: Vec<String> = Vec::new();
    // The roots as the WALK resolves them, not as `discovery_roots` spells
    // them. Comparing against the literal spelling would refuse every row the
    // feature had just offered on any Mac whose ~/Documents is a symlink into
    // iCloud Drive — a functional break disguised as a safety check.
    let discoverable = largeold::resolve_roots(&safety::allowlist::discovery_roots(home), home);

    for raw in paths {
        let safe = match safety::guard(Path::new(raw), home) {
            Ok(s) => s,
            Err(e) => {
                rejected.push(format!("{raw}: {e}"));
                continue;
            }
        };

        // The path must already BE its canonical self.
        //
        // Without this, `granted` is minted from the same resolution as the
        // plan, in the same instant — a rubber stamp derived from the request
        // rather than independent evidence of a human choice. That neutralizes
        // the executor's TOCTOU defense: if the listed file is replaced by a
        // symlink between the walk and the confirmation, both the action and
        // the grant resolve to the *new* target, they agree, and a file the
        // user never saw is disposed of.
        //
        // The walk emits canonical paths, so demanding one here is free and
        // exact: anything that resolves elsewhere is, by definition, not the
        // file that was listed.
        //
        // Identity is by *path*, not by inode: a file replaced at the same path
        // between the walk and this call is acted on in place of the one that
        // was shown. That is recoverable (Trash) and audited under the real
        // path, and closing it would mean carrying inode+generation through the
        // UI — noted so it is a known limit rather than an assumed guarantee.
        if safe.as_path() != Path::new(raw) {
            rejected.push(format!(
                "{raw}: resolves to {safe} — not the file that was listed"
            ));
            continue;
        }

        // Disposal must never be wider than discovery.
        //
        // `guard` enforces the denylist, which is a much weaker statement than
        // "this came from the list we showed you": every ordinary file on the
        // volume passes it. Confining to the discovery scope is what keeps this
        // entry point from being a general-purpose file remover that happens to
        // be reachable from the Large & Old screen.
        //
        // Be precise about what this does and does not guarantee. The invariant
        // enforced here is **disposal ⊆ discovery_roots**, not "⊆ what this
        // particular walk offered". Two consequences, both live only once the
        // feature grows beyond its current shape:
        //
        //   * `LargeOldConfig::roots` is public, so a caller that *narrows* the
        //     walk (a future "search Downloads only" filter) would still get
        //     the full ceiling here. The first such filter must thread its
        //     resolved roots through to this call, not rely on these agreeing.
        //   * The scope is resolved again, now, rather than being carried from
        //     the walk — so a root that did not exist during the scan but does
        //     at this moment is in scope. The reverse is fail-closed.
        //
        // Both stay under the ceiling, so neither is an escape; they are the
        // reason the ceiling is stated as the ceiling.
        if !safety::allowlist::is_allowed(safe.as_path(), &discoverable) {
            rejected.push(format!("{raw}: outside the discovery scope"));
            continue;
        }
        // Re-read the size from disk. The frontend's number is a display value;
        // this one is what the mass-delete threshold is measured against.
        let size_bytes = match std::fs::symlink_metadata(safe.as_path()) {
            // The walk never returns a directory, so one arriving here means
            // the UI sent something it did not get from us. The executor would
            // refuse it anyway, but refusing the whole request says so plainly
            // instead of silently acting on the rest of the list.
            Ok(m) if m.is_dir() => {
                rejected.push(format!("{raw}: is a directory"));
                continue;
            }
            Ok(m) => m.len(),
            Err(e) => {
                rejected.push(format!("{raw}: {e}"));
                continue;
            }
        };
        // Two spellings of one file must not count as two items — that would
        // inflate the total the mass-delete threshold is measured against, and
        // the second attempt would then "fail" on a file we just moved.
        if granted.iter().any(|g: &safety::SafePath| g == &safe) {
            continue;
        }
        granted.push(safe.clone());
        actions.push(PlannedAction {
            path: safe,
            size_bytes,
            disposal: Disposal::Trash,
            category: "large-and-old".to_string(),
        });
    }

    if !rejected.is_empty() {
        // Refuse wholesale. Acting on the rest would touch a different set than
        // the one that was confirmed.
        let shown: Vec<&str> = rejected.iter().take(3).map(|s| s.as_str()).collect();
        let more = rejected.len().saturating_sub(shown.len());
        let suffix = if more > 0 {
            format!(" (and {more} more)")
        } else {
            String::new()
        };
        return refuse_and_record(
            audit,
            format!(
                "refused: {} of {} selected items are no longer valid, so nothing was \
                 touched. Scan again and review. First problems: {}{}",
                rejected.len(),
                paths.len(),
                shown.join("; "),
                suffix
            ),
        );
    }

    let plan = Plan {
        actions,
        skipped_protected: 0,
        dirs: Vec::new(),
    };

    if let Some(exp) = expected {
        if let Some(why) = selection_drifted(plan.count(), plan.total_bytes(), exp) {
            return refuse_and_record(
                audit,
                format!(
                    "refused: {why}, so nothing was touched. This would now act on {} items \
                     ({} bytes), but you confirmed {} items ({} bytes). Scan again and review.",
                    plan.count(),
                    plan.total_bytes(),
                    exp.count,
                    exp.bytes
                ),
            );
        }
    }

    let consent = Consent {
        execute: true,
        allow_permanent: false,
        confirmed_mass_delete: confirm_mass_delete,
        granted,
        granted_dirs: Vec::new(),
    };

    match execute(&plan, consent, home, sink, audit) {
        Ok(report) => Ok(CleanSummary {
            dry_run: report.dry_run,
            executed: report.executed,
            refused: report.refused,
            bytes_freed: report.bytes_executed,
            entries_freed: report.entries_executed,
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Real-app entry point for acting on a Large & Old selection.
pub fn dispose_selected(
    paths: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    let audit_path = default_audit_path().map_err(|e| e.to_string())?;
    let mut audit = AuditLog::open(&audit_path).map_err(|e| e.to_string())?;
    dispose_selected_with_sink(
        &home,
        &paths,
        expected,
        confirm_mass_delete,
        &SystemSink,
        &mut audit,
    )
}

// ---------------------------------------------------------------------------
// Uninstaller — leftover discovery, and the disposal whose ceiling is a scan
//
// The Large & Old disposal above confines a selection to the discovery roots.
// That is not enough here: `<container>/Data/Documents` sits *inside* a
// leftover location and must never be acted on. So this entry point's ceiling
// is not a set of roots but the `offerable` rows of a scan run inside the
// call — a path is accepted only if it is byte-equal to one of them.
// ---------------------------------------------------------------------------

/// What the frontend names when it asks about an application.
///
/// Both strings are validated by `macclean_core::uninstall` before they become
/// match keys. Deliberately **only** these two: a command that could set the
/// inventory roots or the home would let a frontend make an installed app look
/// uninstalled, which is the one mistake that module must never make.
#[derive(Debug, Clone, Deserialize)]
pub struct UninstallTarget {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// One leftover row, as the UI renders it. No `selected` field, for the same
/// reason Large & Old has none: every grant is a human's individual choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeftoverRowDto {
    pub path: String,
    pub location: String,
    pub matched_via: String,
    pub kind: String,
    /// Whether disposing of this row is a directory action — which always
    /// asks for the mass-delete confirmation, however small the tree.
    pub is_dir: bool,
    pub size_bytes: u64,
    pub file_count: u64,
    pub size_is_floor: bool,
    pub offerable: bool,
    pub bulk_grantable: bool,
    pub withheld: Option<String>,
    pub undisposable: Option<String>,
    pub license_suspected: bool,
}

/// Mirrors `uninstall::LeftoverReport`, with `is_partial()` flattened and the
/// offerable totals computed from the rows actually emitted — so the header
/// figure and the visible list cannot disagree when a row is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UninstallReportDto {
    pub target: String,
    /// True when the app is still installed, in which case `rows` is empty:
    /// an installed app has no leftovers, it has files.
    pub installed: bool,
    pub installed_at: Vec<String>,
    pub rows: Vec<LeftoverRowDto>,
    pub offerable_count: usize,
    pub offerable_bytes: u64,
    pub withheld_count: usize,
    pub examined: usize,
    pub truncated: bool,
    pub skipped_unreadable: usize,
    pub skipped_symlink: usize,
    pub skipped_case_variant: usize,
    pub skipped_unrepresentable: usize,
    pub skipped_uncorroborated_name: usize,
    /// Rows whose path is not valid UTF-8 and so cannot round-trip to the UI.
    pub dropped_unrepresentable_rows: usize,
    pub deferred: Vec<(String, String)>,
    pub caveats: Vec<String>,
    pub partial: bool,
}

const LEFTOVER_CATEGORY: &str = "uninstaller-leftovers";

/// The most of a frontend string that is echoed into a refusal reason — and
/// so into the append-only audit log, which is never rotated. Long enough to
/// recognise a path, short enough that a webview cannot fill the disk one
/// refusal at a time.
const ECHO_LIMIT: usize = 160;

fn clip(s: &str) -> String {
    if s.chars().count() <= ECHO_LIMIT {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(ECHO_LIMIT).collect::<String>())
    }
}

fn parse_target(target: &UninstallTarget) -> Result<(BundleId, Option<DisplayName>), String> {
    let id = BundleId::parse(&target.id)
        .ok_or_else(|| UninstallError::UnmatchableId(clip(&target.id)).to_string())?;
    let name = match &target.display_name {
        Some(raw) => Some(
            DisplayName::parse(raw)
                .ok_or_else(|| format!("refused: {:?} is not a usable display name", clip(raw)))?,
        ),
        None => None,
    };
    Ok((id, name))
}

fn row_dto(row: &Candidate) -> Option<LeftoverRowDto> {
    // `to_str`, not `display()`: the disposal path identifies a selection by
    // byte equality with the string emitted here, and a lossy conversion
    // would break that identity in a way it could not detect.
    let path = row.path.to_str()?.to_string();
    let is_dir = std::fs::symlink_metadata(&row.path)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    let matched_via = match &row.matched_via {
        MatchedVia::Id => "id".to_string(),
        MatchedVia::IdWithSuffix(s) => format!("id{s}"),
        MatchedVia::SiblingSegment(tail) => format!("sibling:{tail}"),
        MatchedVia::IdWithPrefix(prefix) => format!("prefix:{prefix}"),
        MatchedVia::DisplayName(name) => format!("name:{name}"),
    };
    let kind = match row.kind {
        Kind::Leftover => "leftover",
        Kind::UserData => "user_data",
        Kind::Shared => "shared",
    };
    Some(LeftoverRowDto {
        path,
        location: row.location.as_str().to_string(),
        matched_via,
        kind: kind.to_string(),
        is_dir,
        size_bytes: row.size_bytes,
        file_count: row.file_count,
        size_is_floor: row.size_is_floor,
        offerable: row.offerable,
        bulk_grantable: row.bulk_grantable,
        withheld: row.withheld.clone(),
        undisposable: row.undisposable.map(str::to_string),
        license_suspected: row.license_suspected,
    })
}

fn report_dto(report: &LeftoverReport) -> UninstallReportDto {
    let mut rows = Vec::with_capacity(report.rows.len());
    let mut dropped = 0usize;
    for row in &report.rows {
        match row_dto(row) {
            Some(dto) => rows.push(dto),
            None => dropped += 1,
        }
    }
    let offerable_count = rows.iter().filter(|r| r.offerable).count();
    let offerable_bytes = rows
        .iter()
        .filter(|r| r.offerable)
        .fold(0u64, |a, r| a.saturating_add(r.size_bytes));
    let (installed, installed_at) = match &report.residence {
        Residence::Installed(paths) => (
            true,
            paths.iter().map(|p| p.display().to_string()).collect(),
        ),
        Residence::NotFound { .. } => (false, Vec::new()),
    };
    UninstallReportDto {
        target: report.target.to_string(),
        installed,
        installed_at,
        rows,
        offerable_count,
        offerable_bytes,
        withheld_count: report.withheld_count,
        examined: report.examined,
        truncated: report.truncated,
        skipped_unreadable: report.skipped_unreadable,
        skipped_symlink: report.skipped_symlink,
        skipped_case_variant: report.skipped_case_variant,
        skipped_unrepresentable: report.skipped_unrepresentable,
        skipped_uncorroborated_name: report.skipped_uncorroborated_name,
        dropped_unrepresentable_rows: dropped,
        deferred: report
            .deferred
            .iter()
            .map(|(p, why)| (p.to_string(), why.to_string()))
            .collect(),
        caveats: report.caveats.iter().map(|c| c.to_string()).collect(),
        partial: report.is_partial() || dropped > 0,
    }
}

/// Read-only: what `target` left behind. The testable seam — a function that
/// resolves the real home can never be exercised by a fixture, which is how
/// an earlier empty-selection fail-open survived.
pub fn uninstall_leftovers_in(
    cfg: &UninstallConfig,
    target: &UninstallTarget,
) -> Result<UninstallReportDto, String> {
    let (id, name) = parse_target(target)?;
    let report =
        uninstall::leftovers_for_named(cfg, &id, name.as_ref()).map_err(|e| e.to_string())?;
    Ok(report_dto(&report))
}

/// Read-only, against the real home.
pub fn uninstall_leftovers(target: &UninstallTarget) -> Result<UninstallReportDto, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    uninstall_leftovers_in(&UninstallConfig::new(home), target)
}

/// Move individually-chosen leftover rows to the Trash.
///
/// Stricter than [`dispose_selected_with_sink`], because its ceiling has to be:
///
/// - **Discovery runs again, inside this call.** A path is accepted only if it
///   is byte-equal (`OsStr`, not `Path` — `Path` equality is component-wise,
///   so `/x/./y` would pass) to the `path` of a row with `offerable == true` in
///   that fresh report. So a container root, a `Data/Documents` row, a group
///   container, a withheld launch agent, a tree `guard_dir` would refuse, and
///   anything the user never saw are all rejected in one place — and an app
///   that was installed since the scan yields no rows at all, which rejects
///   everything.
/// - **A scan that could not complete refuses the whole request.** An
///   unreadable application root means "is it still installed?" has no
///   answer, and that must never become a disposal.
/// - **Every path is re-guarded individually** — `guard` for a file,
///   `guard_dir` for a directory — with the denylist first, and must already be
///   its own canonical spelling.
/// - **Any rejection refuses the whole request.** A partial run does not match
///   the list the user confirmed.
/// - **Trash only.** A directory action cannot express anything else.
///
/// `bulk_grantable == false` rows are accepted: that flag governs a select-all
/// gesture in the UI, and enforcing it here would break the individual
/// selection it exists to require.
pub fn dispose_leftovers_with_sink(
    cfg: &UninstallConfig,
    target: &UninstallTarget,
    paths: &[String],
    expected: Option<Expected>,
    confirm_mass_delete: bool,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<CleanSummary, String> {
    if paths.is_empty() {
        return refuse_and_record(audit, "refused: nothing was selected.".to_string());
    }
    // The denylist's home-relative rules — Keychains, Mail, the home root —
    // compare component-wise against the home, so a non-canonical spelling
    // would silently disable all three for the whole run. `largeold::find_in`
    // and `uninstall::leftovers_in` canonicalize; this seam refuses instead,
    // because the scan and `resolved_locations` read the same field and a
    // substitution here would leave them disagreeing about the disk.
    match safety::canonical_home(&cfg.home) {
        Ok(canonical) if canonical == cfg.home => {}
        _ => {
            return refuse_and_record(
                audit,
                "refused: the home directory is not its canonical spelling, so the \
                 denylist's home-relative rules could not be trusted for this run."
                    .to_string(),
            )
        }
    }
    let (id, name) = match parse_target(target) {
        Ok(t) => t,
        Err(e) => return refuse_and_record(audit, e),
    };
    // Fresh, now. Fail closed on any error — `InventoryIncomplete` above all.
    let report = match uninstall::leftovers_for_named(cfg, &id, name.as_ref()) {
        Ok(r) => r,
        Err(e) => return refuse_and_record(audit, format!("refused: {e}")),
    };

    // The ceiling: the offerable rows of *this* scan, keyed by bytes.
    //
    // Discovery guarantees that `offerable` already implies `Kind::Leftover`,
    // no `undisposable` reason and no `withheld` reason. This is the last
    // layer before a mutation, so that agreement is enforced here rather than
    // trusted: a one-word regression in a row constructor over there must not
    // be able to hand a container's `Data/Documents` to the Trash from here.
    let offerable: BTreeMap<&OsStr, &Candidate> = report
        .rows
        .iter()
        .filter(|r| {
            r.offerable
                && r.kind == Kind::Leftover
                && r.undisposable.is_none()
                && r.withheld.is_none()
        })
        .map(|r| (r.path.as_os_str(), r))
        .collect();
    // Defence in depth, and redundant by construction: every row is inside a
    // resolved location root. Kept as the stated outer ceiling; it cannot
    // replace the intersection above, because `Data/Documents` is inside a
    // location root too.
    let locations: Vec<PathBuf> = uninstall::resolved_locations(cfg)
        .into_iter()
        .map(|(_, p)| p)
        .collect();

    let mut actions = Vec::new();
    let mut dirs = Vec::new();
    let mut granted = Vec::new();
    let mut granted_dirs = Vec::new();
    let mut rejected: Vec<String> = Vec::new();
    let mut seen: BTreeSet<&OsStr> = BTreeSet::new();
    // What the sheet showed: rows and their sizes, for the drift check.
    let mut selected_rows = 0usize;
    let mut selected_bytes = 0u64;

    for raw in paths {
        let key = OsStr::new(raw.as_str());
        let shown = clip(raw);
        let Some(row) = offerable.get(key) else {
            rejected.push(format!("{shown}: not something this scan offers"));
            continue;
        };
        // Two spellings cannot reach here (equality is byte-exact), but the
        // same row twice must not count twice.
        if !seen.insert(key) {
            continue;
        }
        selected_rows += 1;
        selected_bytes = selected_bytes.saturating_add(row.size_bytes);

        let meta = match std::fs::symlink_metadata(&row.path) {
            Ok(m) => m,
            Err(e) => {
                rejected.push(format!("{shown}: {e}"));
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            rejected.push(format!("{shown}: is now a symlink"));
            continue;
        }
        // In each branch, in order: the denylist (through the guard), then the
        // canonical spelling, then the outer ceiling. Each can only narrow.
        if meta.is_dir() {
            // The tree walked in full, denylist at every depth, bounded. Its
            // canonical path must be the row's own spelling: anything that
            // resolves elsewhere is not the directory that was listed.
            let dir = match safety::guard_dir(&row.path, &cfg.home, DirLimits::default()) {
                Ok(d) => d,
                Err(e) => {
                    rejected.push(format!("{shown}: {e}"));
                    continue;
                }
            };
            if dir.as_path().as_os_str() != key {
                rejected.push(format!(
                    "{shown}: resolves to {} — not the directory that was listed",
                    dir.as_path().display()
                ));
                continue;
            }
            if !safety::allowlist::is_allowed(dir.as_path(), &locations) {
                rejected.push(format!("{shown}: outside the leftover locations"));
                continue;
            }
            granted_dirs.push(dir.clone());
            dirs.push(PlannedDirAction {
                dir,
                category: LEFTOVER_CATEGORY.to_string(),
            });
        } else {
            let safe = match safety::guard(&row.path, &cfg.home) {
                Ok(s) => s,
                Err(e) => {
                    rejected.push(format!("{shown}: {e}"));
                    continue;
                }
            };
            if safe.as_path().as_os_str() != key {
                rejected.push(format!(
                    "{shown}: resolves to {safe} — not the file that was listed"
                ));
                continue;
            }
            if !safety::allowlist::is_allowed(safe.as_path(), &locations) {
                rejected.push(format!("{shown}: outside the leftover locations"));
                continue;
            }
            granted.push(safe.clone());
            actions.push(PlannedAction {
                path: safe,
                size_bytes: meta.len(),
                disposal: Disposal::Trash,
                category: LEFTOVER_CATEGORY.to_string(),
            });
        }
    }

    if !rejected.is_empty() {
        let shown: Vec<&str> = rejected.iter().take(3).map(|s| s.as_str()).collect();
        let more = rejected.len().saturating_sub(shown.len());
        let suffix = if more > 0 {
            format!(" (and {more} more)")
        } else {
            String::new()
        };
        return refuse_and_record(
            audit,
            format!(
                "refused: {} of {} selected items are not something this scan offers, so \
                 nothing was touched. Scan again and review. First problems: {}{}",
                rejected.len(),
                paths.len(),
                shown.join("; "),
                suffix
            ),
        );
    }

    // Drift is measured against what the sheet showed — rows and their sizes
    // from the fresh report — not against `SafeDir` figures, which answer a
    // different question (names to unlink versus files shown).
    if let Some(exp) = expected {
        if let Some(why) = selection_drifted(selected_rows, selected_bytes, exp) {
            return refuse_and_record(
                audit,
                format!(
                    "refused: {why}, so nothing was touched. This would now act on {} rows \
                     ({} bytes), but you confirmed {} rows ({} bytes). Scan again and review.",
                    selected_rows, selected_bytes, exp.count, exp.bytes
                ),
            );
        }
    }

    let plan = Plan {
        actions,
        dirs,
        skipped_protected: 0,
    };
    let consent = Consent {
        execute: true,
        allow_permanent: false,
        confirmed_mass_delete: confirm_mass_delete,
        granted,
        granted_dirs,
    };

    match execute(&plan, consent, &cfg.home, sink, audit) {
        Ok(report) => Ok(CleanSummary {
            dry_run: report.dry_run,
            executed: report.executed,
            refused: report.refused,
            bytes_freed: report.bytes_executed,
            entries_freed: report.entries_executed,
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Dispose of leftover rows against the real home. See
/// [`dispose_leftovers_with_sink`].
pub fn dispose_leftovers(
    target: &UninstallTarget,
    paths: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    let home = default_home().map_err(|e| e.to_string())?;
    let audit_path = default_audit_path().map_err(|e| e.to_string())?;
    let mut audit = AuditLog::open(&audit_path).map_err(|e| e.to_string())?;
    dispose_leftovers_with_sink(
        &UninstallConfig::new(home),
        target,
        &paths,
        expected,
        confirm_mass_delete,
        &SystemSink,
        &mut audit,
    )
}
