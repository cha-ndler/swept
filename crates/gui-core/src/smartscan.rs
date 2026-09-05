//! Smart Scan: one gesture, one figure, over the sources that can be acted on.
//!
//! Read-only. This module runs the other modules' *reports* and adds them up.
//! It mints no `SafePath`, holds no `Consent`, and calls nothing that mutates —
//! every action still goes through the module's own verb, which re-runs its own
//! scan inside the call and enforces its own ceiling. Smart Scan is not a second
//! disposal path, and the way to keep that true is for it to have no disposal
//! code at all.
//!
//! # Three sources, and the two that are deliberately not sources
//!
//! **Dispatchable:** the cleaner categories, Privacy's regenerable rows, and
//! Large & Old.
//!
//! **The Uninstaller is not one.** It takes a bundle id; there is no
//! whole-machine leftovers scan, and building one means an orphan sweep. The M4
//! entry records the measurement that makes that a separate question: on the
//! reference machine 747 of 822 containers are `com.apple.*` and 626 have no
//! owner in the inventory at all, because their owners live under
//! `/System/Library`, outside `inventory_roots`. A sweep over today's inventory
//! would offer every one of them. It also inverts the predicate `uninstall.rs`
//! exists to get right — from *prove this app is gone, because the caller named
//! it* to *enumerate everything and withhold what looks owned* — in the one
//! module where over-reporting is the catastrophic direction.
//!
//! **Startup is a finding, not a checkbox.** `StartupSummary` has no
//! bytes-freed field, and the comment on its absence is the argument: a field
//! that cannot exist cannot be summed into a total later. Setting a plist aside
//! is a move, not a disposal, so folding it in would put a non-disposal under a
//! disposal confirmation, contributing zero to the total it sits beneath.
//!
//! **Space Lens contributes no bytes**, and the first reason is overlap rather
//! than units. It measures the discovery scope — the same scope Large & Old
//! measures, containing the profiles Privacy's rows live in — and does not
//! measure `~/Library/Caches`, `~/Library/Logs`, DerivedData or `~/.Trash` at
//! all, which is where the default selection actually comes from. Including it
//! would double-count two sources and miss the one that matters. Its
//! allocated-vs-apparent divergence (`spacelens.rs`) is then a footnote on that
//! screen rather than something this module has to resolve.
//!
//! # Why there is no overlap-folding machinery here
//!
//! There is no overlap left to fold, and that is worth stating because an
//! earlier design assumed there was.
//!
//! `default_roots` (caches, logs, DerivedData, Trash) and `discovery_roots`
//! (Documents, Downloads, Desktop, Movies, Music, Pictures, Application
//! Support, /Applications) are disjoint, so cleanup and Large & Old cannot
//! double-count. Privacy's rows live inside browser profiles, and every path
//! inside a browser's own root is refused by `dispose_selected_with_sink` — so a
//! Large & Old row there is not something Smart Scan may count either, and this
//! module filters on exactly that predicate rather than a copy of it. Privacy's
//! own browser caches under `~/Library/Caches` are already reported without a
//! size, because `user-caches` covers them (`covered_elsewhere`).
//!
//! Pinned by [`tests`], because "the scopes are disjoint" is a property of two
//! lists that someone may widen.
//!
//! # The number
//!
//! There is no bare byte figure at the top level of the report. Every one lives
//! inside a [`Total`], which carries where it came from and what it could not
//! see — so a frontend cannot render the figure without holding its
//! completeness. That is this codebase's own idiom: `CoveredDto` deliberately
//! has no size field, `StartupSummary` deliberately has no bytes field.
//!
//! And the total is **exact for what it covers**, which is a stronger and truer
//! claim than "a floor". `treewalk::offer` guarantees `size_is_floor` implies
//! `!offerable`, so summing offerable rows never sums a floor.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use swept_core::audit::AuditLog;
use swept_core::executor::Sink;
use swept_core::privacy;
use swept_core::report::{CategorySummary, ScanReport};

use crate::{
    browser_root_for, build_config, clean_with_sink, dispose_privacy_with_sink,
    dispose_selected_with_sink, gui_consent, large_and_old, privacy_report_from, probe_permissions,
    refuse_and_record, startup_report_in, Acknowledged, CleanSummary, Expected, Filters,
    LargeOldReportDto, Permissions, PrivacyRowDto,
};

/// Something a source could not see, named by the source that could not see it.
///
/// A single boolean would say "some figure somewhere is short"; this says which
/// one, in that module's own vocabulary, so the notice on screen can be about
/// the thing the reader is looking at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Incompleteness {
    pub source: String,
    pub reason: String,
}

/// A byte figure that cannot be rendered without its provenance.
///
/// Every reason currently recorded in `incomplete` means the truth is *higher*
/// than `bytes` — documented rather than made a field, because a direction that
/// is always the same is a comment, not data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Total {
    pub bytes: u64,
    /// Which sources contributed, in dispatch order.
    pub from: Vec<String>,
    /// Empty when this figure describes everything there is.
    pub incomplete: Vec<Incompleteness>,
}

impl Total {
    fn new() -> Self {
        Self {
            bytes: 0,
            from: Vec::new(),
            incomplete: Vec::new(),
        }
    }

    fn add(&mut self, source: &str, bytes: u64) {
        // A source contributing nothing is still a source that was consulted,
        // and saying so is how "we looked and there was nothing" stays
        // distinguishable from "we did not look".
        self.from.push(source.to_string());
        self.bytes = self.bytes.saturating_add(bytes);
    }

    fn note(&mut self, source: &str, reason: impl Into<String>) {
        self.incomplete.push(Incompleteness {
            source: source.to_string(),
            reason: reason.into(),
        });
    }

    /// True when this figure describes everything there is to describe.
    pub fn is_complete(&self) -> bool {
        self.incomplete.is_empty()
    }
}

/// What runs at login, as a *finding*. No bytes and no selection, by
/// construction rather than by omission — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartupFindingDto {
    pub starts_at_login: usize,
    /// How many of those this app could set aside, if asked on its own screen.
    pub can_act_on: usize,
    pub modern_store_present: bool,
    pub partial: bool,
}

/// The combined, read-only picture.
#[derive(Debug, Clone, Serialize)]
pub struct SmartScanReportDto {
    /// When the **oldest** contributing scan started, stamped here rather than
    /// taken from the frontend. The dispatch half compares it against `now`.
    pub scanned_at_ms: u64,
    /// What the default gesture would free.
    pub selected: Total,
    /// What every source reported that could be acted on if it were ticked.
    /// Always at least `selected`.
    pub found: Total,
    pub cleanup: Vec<CategorySummary>,
    /// Rows this screen may offer. Never the ones that carry a consequence —
    /// those stay on Privacy, which has the acknowledgement axis for them.
    pub privacy: Vec<PrivacyRowDto>,
    pub large_old: LargeOldReportDto,
    pub startup: StartupFindingDto,
    pub permissions: Permissions,
}

/// What a Smart Scan looks at.
///
/// The two thresholds are deliberately separate fields rather than one shared
/// number. `filters` are the *cleaner* filters, exactly as the Clean screen
/// sends them. `large_old_min_size` is the Large & Old floor, and letting a
/// cleaner knob drive it would let the frontend widen what
/// `dispose_selected_with_sink` will accept by changing a control that appears
/// to be about something else. Two knobs that happen to share a name are still
/// two knobs.
pub struct SmartScanConfig {
    /// Canonical home directory (see `safety::canonical_home`).
    pub home: PathBuf,
    pub filters: Filters,
    /// Injectable only so a fixture can reach it — the same posture
    /// `PrivacyConfig::dir_limits` takes, and for the same reason: the real
    /// value is far larger than anything a test can afford to create.
    pub large_old_min_size: u64,
}

impl SmartScanConfig {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            filters: Filters::default(),
            large_old_min_size: swept_core::largeold::DEFAULT_MIN_SIZE,
        }
    }

    pub fn with_filters(mut self, filters: Filters) -> Self {
        self.filters = filters;
        self
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Gather every source. Read-only; mutates nothing, authorizes nothing.
pub fn smart_scan_in(cfg: &SmartScanConfig) -> SmartScanReportDto {
    let home = cfg.home.as_path();
    // Stamped before the first scan, so it describes the oldest contribution
    // rather than the youngest.
    let scanned_at_ms = now_ms();

    // --- cleaners ----------------------------------------------------------
    let plan = swept_core::scanner::scan(&build_config(home, &cfg.filters));
    let cleanup = ScanReport::from_plan_without_items(&plan);

    // --- privacy -----------------------------------------------------------
    let private = privacy::scan(&privacy::PrivacyConfig::new(home.to_path_buf()));
    let privacy_dto = privacy_report_from(&private);

    // --- large & old -------------------------------------------------------
    //
    // Its own threshold, and no age filter: the Clean screen's "older than"
    // control is about caches that may still be in use, which is not a question
    // about a large file in Downloads. Large & Old's own default is any age.
    let large_old = large_and_old(home, cfg.large_old_min_size, None);

    // --- the two figures ---------------------------------------------------
    let mut selected = Total::new();
    let mut found = Total::new();

    let default_cleanup: u64 = cleanup
        .by_category
        .iter()
        .filter(|c| c.smart_scan_default)
        .map(|c| c.bytes)
        .sum();
    selected.add("cleanup", default_cleanup);
    found.add(
        "cleanup",
        cleanup.by_category.iter().map(|c| c.bytes).sum::<u64>(),
    );
    if cleanup.partial {
        let what = format!(
            "{} place{} could not be read",
            cleanup.skipped_unreadable,
            if cleanup.skipped_unreadable == 1 {
                ""
            } else {
                "s"
            }
        );
        selected.note("cleanup", what.clone());
        found.note("cleanup", what);
    }

    let eligible: Vec<PrivacyRowDto> = privacy_dto
        .rows
        .iter()
        .filter(|r| r.smart_scan_eligible)
        .cloned()
        .collect();
    selected.add("privacy", eligible.iter().map(|r| r.size_bytes).sum());
    found.add(
        "privacy",
        privacy_dto
            .rows
            .iter()
            .filter(|r| r.offerable)
            .map(|r| r.size_bytes)
            .sum(),
    );
    if privacy_dto.partial {
        let what = "some browser data could not be read";
        selected.note("privacy", what);
        found.note("privacy", what);
    }

    // Large & Old is never pre-selected — the whole point of that module is
    // that a human chooses each row — so it contributes to `found` only, and
    // only the rows its own verb would actually accept. Counting a row the
    // dispatch would refuse would promise bytes no confirmed run could free.
    let acceptable: u64 = large_old
        .items
        .iter()
        .filter(|i| browser_root_for(home, Path::new(&i.path)).is_none())
        .map(|i| i.size_bytes)
        .sum();
    found.add("large-old", acceptable);
    if large_old.partial {
        found.note("large-old", "some folders could not be read");
    }
    if large_old.truncated {
        found.note(
            "large-old",
            format!(
                "the list stops at {} of {} matches",
                large_old.items.len(),
                large_old.matched
            ),
        );
    }

    // --- read-only findings ------------------------------------------------
    let startup = startup_report_in(&swept_core::loginitems::StartupConfig::new(
        home.to_path_buf(),
    ));

    SmartScanReportDto {
        scanned_at_ms,
        selected,
        found,
        cleanup: cleanup.by_category,
        privacy: eligible,
        large_old,
        startup: StartupFindingDto {
            starts_at_login: startup.starts_at_login,
            can_act_on: startup.items.iter().filter(|i| i.offerable).count(),
            modern_store_present: startup.modern_store_present,
            partial: startup.partial,
        },
        permissions: probe_permissions(home),
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------
//
// One confirmed gesture spanning three modules. It cannot be atomic — nothing
// here can roll back a `trash::delete` — so the honest design is sequential,
// fail-fast, and ledgered, and the claim is exactly:
//
//     **no step begins after a step refused.**
//
// Not "the run is atomic", which would be false: `executor::execute` already
// continues past a failed *action* inside step one and reports it as `refused`.
// Overstating that would be the same class of error as a total that counts rows
// the verbs would decline.
//
// Pre-flighting every module in dry-run first was considered and rejected. Each
// verb's drift check compares `Expected` against a scan run *inside* the call,
// so a green pre-flight and the real run are two different scans and the first
// says nothing about the second — at double the cost, and requiring a `dry_run`
// bit on three destructive entry points that could be flipped the wrong way.

/// The order steps run in, and it satisfies two criteria at once.
///
/// **Loosest drift tolerance first.** Cleanup's is `grew_beyond` — ±10 % or
/// 64 MiB, the cache-churn allowance — while privacy and Large & Old match
/// exact rows within 1 MiB. The step most likely to refuse therefore runs last,
/// where its refusal strands nothing.
///
/// **Smallest blast radius first.** Cleanup is confined to the disposal
/// allowlist; privacy to each row's own `profile_root`; Large & Old reaches into
/// `~/Downloads`, where a human is also working — which is both the widest scope
/// and the one most likely to have drifted, for the same reason.
pub const DISPATCH_ORDER: [&str; 3] = ["cleanup", "privacy", "large-old"];

/// How stale a report may be when it is confirmed.
///
/// This is a guard against our own UI holding a report open, **not**
/// authentication. `Deserialize` is a constructor, so a frontend can send any
/// number it likes, and it can call the module verbs directly regardless.
/// Over-claiming here would be worse than not having it.
///
/// It is additive: deleting this check should leave every other refusal in this
/// file intact, which is what
/// `the_freshness_check_is_additive_and_a_stale_selection_is_still_refused`
/// pins.
///
/// **Ten minutes, not five, because the clock starts before the gathering.**
/// `scanned_at_ms` is stamped ahead of the first of four scans — which is the
/// honest answer to "how old is this data" — and a cold cleaner scan alone is
/// ~37 s on a real home, before privacy, Large & Old and startup. A five-minute
/// budget would therefore have given the reader materially less than five
/// minutes, and a guard that fires on ordinary reading trains people to re-scan
/// and confirm without looking, which is the opposite of what it is for.
///
/// The value is a judgement, not a derivation.
pub const MAX_REPORT_AGE_MS: u64 = 10 * 60 * 1000;

/// What the frontend confirmed, per source.
///
/// **No aggregate `Expected`.** A combined count and byte total could not be
/// checked against any single verb's rescan, and inventing a combined tolerance
/// would be inventing a new, looser one. Each of these is handed to its verb
/// unchanged.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmartScanExpected {
    pub cleanup: Option<Expected>,
    pub privacy: Option<Expected>,
    pub large_old: Option<Expected>,
}

/// Which sources the user confirmed a mass delete for.
///
/// **One boolean cannot answer three questions.** `Plan::requires_confirmation`
/// is evaluated independently by each verb against its own count and bytes, so a
/// single flag lets a person who confirmed one combined figure cross
/// `MASS_DELETE_COUNT` inside a module whose own count they never saw — which is
/// exactly what SAFETY CONTRACT item 5 ("show count + size and require
/// confirmation") is about.
///
/// The same argument that rules out an aggregate [`Expected`] rules out an
/// aggregate confirmation, and missing that the first time is why this is a
/// struct rather than a `bool`.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmartScanConfirm {
    pub cleanup: bool,
    pub privacy: bool,
    pub large_old: bool,
}

/// One confirmed Smart Scan gesture.
///
/// **Three separately named path fields, not one tagged list.** This is the only
/// structural defence against the hazard M7 actually introduces: three sources
/// that used to live in three components now sit in one frontend state object,
/// so a routing bug could hand a privacy row to the Large & Old verb. There is
/// no field a privacy path can occupy that routes it to `dispose_selected_with_sink`
/// except `large_old_paths`.
///
/// `deny_unknown_fields` is the second half of that: a frontend sending a field
/// this backend does not know gets a refusal rather than a silent omission — and
/// it is what stops a `leftover_paths` field appearing later without a
/// deliberate edit here.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmartScanRequest {
    /// Echoed back from the report. Compared against now; see
    /// [`MAX_REPORT_AGE_MS`].
    pub scanned_at_ms: u64,
    /// The filters the **report** was built with, echoed back unchanged.
    ///
    /// Carried on the request rather than taken from the backend's defaults,
    /// because otherwise the preview and the action are built from two
    /// different configurations — and the divergence is always in the widening
    /// direction, so a run would remove files the filter had excluded and the
    /// user never saw. Nothing else in this codebase lets a preview and its
    /// action disagree, and this is the seam where it nearly did.
    #[serde(default)]
    pub filters: Filters,
    pub categories: Vec<String>,
    pub privacy_paths: Vec<String>,
    pub large_old_paths: Vec<String>,
    /// Required for every non-empty source — see [`dispatch_smart_scan_with_sink`].
    #[serde(default)]
    pub expected: SmartScanExpected,
    #[serde(default)]
    pub confirm_mass_delete: SmartScanConfirm,
    /// Refused-by-default, unchanged from M5. Smart Scan never pre-selects a row
    /// that carries a consequence, so in the default gesture this stays empty —
    /// but the axis is threaded through rather than bypassed.
    #[serde(default)]
    pub acknowledged: Acknowledged,
}

/// What happened to one source.
///
/// The third variant is the point of the type. **"We did not try" must not
/// serialize like "we tried and there was nothing"** — that is the ledger form
/// of this project's own named failure, where a report of five things invites
/// the reader to conclude their Mac is clean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StepOutcome {
    /// The verb ran. Its own summary, unmodified.
    Executed { summary: CleanSummary },
    /// The verb refused. Its own reason string, unmodified.
    Refused { reason: String },
    /// An earlier step refused, so this one was never attempted.
    NotAttempted { because: String },
    /// The user chose nothing from this source.
    NotSelected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    pub source: String,
    #[serde(flatten)]
    pub outcome: StepOutcome,
}

/// The ledger.
///
/// Deliberately absent: any "partially succeeded" boolean. [`Self::completed`]
/// is true only when every attempted step executed and none was skipped, and
/// anything more nuanced than that belongs in the steps themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SmartScanRunReport {
    pub steps: Vec<Step>,
    /// Every step either executed or had nothing selected, **and** no
    /// individual action inside an executed step was refused.
    ///
    /// The second half matters: `executor::execute` continues past a failed
    /// action and reports it in `CleanSummary::refused`, so without it this
    /// could read "done" over a run that left files behind.
    pub completed: bool,
    /// Summed from the steps that actually executed.
    pub bytes_freed: u64,
    pub entries_freed: u64,
    /// Individual actions refused inside steps that otherwise executed.
    /// Distinct from a step-level [`StepOutcome::Refused`].
    pub actions_refused: u64,
}

impl SmartScanRunReport {
    fn from_steps(steps: Vec<Step>) -> Self {
        let mut bytes_freed = 0u64;
        let mut entries_freed = 0u64;
        let mut actions_refused = 0u64;
        let mut completed = true;
        for step in &steps {
            match &step.outcome {
                StepOutcome::Executed { summary } => {
                    bytes_freed = bytes_freed.saturating_add(summary.bytes_freed);
                    entries_freed = entries_freed.saturating_add(summary.entries_freed);
                    actions_refused = actions_refused.saturating_add(summary.refused as u64);
                    // A step can execute and still have left something behind.
                    completed &= summary.refused == 0;
                }
                StepOutcome::NotSelected => {}
                // A module-level refusal and a step never attempted both mean
                // the gesture did not happen as confirmed.
                StepOutcome::Refused { .. } | StepOutcome::NotAttempted { .. } => {
                    completed = false;
                }
            }
        }
        Self {
            steps,
            completed,
            bytes_freed,
            entries_freed,
            actions_refused,
        }
    }
}

/// Act on a confirmed Smart Scan, one source at a time.
///
/// Adds **no** disposal capability: every path still goes through that module's
/// own verb, which re-runs its own scan inside the call, re-guards every path,
/// and enforces its own ceiling. Nothing here constructs a `SafePath`, a
/// `Consent`, or a `Sink`.
pub fn dispatch_smart_scan_with_sink(
    cfg: &SmartScanConfig,
    req: &SmartScanRequest,
    sink: &dyn Sink,
    audit: &mut AuditLog,
) -> Result<SmartScanRunReport, String> {
    let home = cfg.home.as_path();

    // The assertion every disposal verb in this layer makes. Repeated rather
    // than delegated, because the refusal below has to happen before any step
    // runs — a run that got halfway on an untrustworthy home is worse than one
    // that never started.
    match safety::canonical_home(home) {
        Ok(canonical) if canonical == home => {}
        _ => {
            return refuse_and_record(
                audit,
                "refused: the home directory is not its canonical spelling, so the \
                 denylist's home-relative rules could not be trusted for this run."
                    .to_string(),
            )
        }
    }

    if let Some(why) = staleness(req.scanned_at_ms, now_ms()) {
        return refuse_and_record(audit, format!("refused: {why}"));
    }

    if req.categories.is_empty() && req.privacy_paths.is_empty() && req.large_old_paths.is_empty() {
        return refuse_and_record(audit, "refused: nothing was selected.".to_string());
    }

    // Every non-empty source must say what it confirmed.
    //
    // The per-verb `Expected` is `Option` because each screen may legitimately
    // act without one. A *combined* gesture may not: it is the only place where
    // one confirmation stands for three magnitudes, so a request that names rows
    // without saying how many is a frontend that lost its sheet state, not a
    // deliberate unchecked run.
    for (name, empty, expected) in [
        ("cleanup", req.categories.is_empty(), req.expected.cleanup),
        (
            "privacy",
            req.privacy_paths.is_empty(),
            req.expected.privacy,
        ),
        (
            "large-old",
            req.large_old_paths.is_empty(),
            req.expected.large_old,
        ),
    ] {
        if !empty && expected.is_none() {
            return refuse_and_record(
                audit,
                format!(
                    "refused: {name} named rows but did not say what was confirmed, so \
                     the selection could not be checked against the disk. Scan again and \
                     review."
                ),
            );
        }
    }

    // A category id the registry does not know cannot have been on the report,
    // so naming one means the frontend and the backend disagree about what
    // exists. `clean_with_sink` would filter it out silently and the ledger
    // would read `Executed { executed: 0 }` — the shape of a successful run over
    // nothing, which is the reading this project keeps refusing to allow.
    if let Some(unknown) = req
        .categories
        .iter()
        .find(|id| swept_core::categories::by_id(id).is_none())
    {
        return refuse_and_record(
            audit,
            format!("refused: {unknown:?} is not a category this scan offers."),
        );
    }

    // Smart Scan's Large & Old contribution excludes anything inside a browser's
    // own data — the aggregator filters on exactly this predicate, so a request
    // naming one is the frontend and the disk disagreeing about what was
    // offered, which is a refusal rather than a no-op.
    //
    // This is deliberately *not* delegated to `dispose_selected_with_sink`'s
    // browser boundary. That check grants passage to `Regenerable` rows — and
    // `smart_scan_eligible` is `offerable && regenerable`, so **every privacy row
    // Smart Scan offers is exactly the class that boundary waves through.**
    // Relying on it would leave the mis-routing hazard covered only by an
    // incidental `is_dir: true` in a spec table in another crate.
    if let Some(bad) = req
        .large_old_paths
        .iter()
        .find(|raw| browser_root_for(home, Path::new(raw)).is_some())
    {
        return refuse_and_record(
            audit,
            format!(
                "refused: {bad:?} is inside a browser's own data, which this gesture \
                 never offers as a large file. Use Privacy for it."
            ),
        );
    }

    let mut steps: Vec<Step> = Vec::with_capacity(DISPATCH_ORDER.len());
    let mut stopped: Option<String> = None;

    for source in DISPATCH_ORDER {
        // Fail-fast. A refusal is evidence about the *world*, not about the
        // module that reported it: it says the report the user confirmed no
        // longer describes the disk. Continuing on the strength of it would
        // maximize the number of actions taken on a premise just shown to be
        // false — and the classes that are genuinely not module-local (an audit
        // failure, a grant cap, a non-canonical home) make continuing
        // indefensible.
        if let Some(because) = &stopped {
            steps.push(Step {
                source: source.to_string(),
                outcome: StepOutcome::NotAttempted {
                    because: because.clone(),
                },
            });
            continue;
        }

        let outcome = match source {
            "cleanup" if req.categories.is_empty() => StepOutcome::NotSelected,
            // `req.filters`, not `cfg.filters`. The request carries what the
            // *report* was built with, so the preview and the action cannot be
            // built from two different configurations — a divergence that is
            // always in the widening direction, removing files the filter
            // excluded and the user never saw. Taking it from the config would
            // leave that only enforceable at the real entry point, where no
            // test can reach it.
            "cleanup" => run(clean_with_sink(
                home,
                &req.filters,
                Some(&req.categories),
                req.expected.cleanup,
                gui_consent(req.confirm_mass_delete.cleanup),
                sink,
                audit,
            )),
            "privacy" if req.privacy_paths.is_empty() => StepOutcome::NotSelected,
            "privacy" => run(dispose_privacy_with_sink(
                &privacy::PrivacyConfig::new(home.to_path_buf()),
                &req.privacy_paths,
                req.acknowledged,
                req.expected.privacy,
                req.confirm_mass_delete.privacy,
                sink,
                audit,
            )),
            "large-old" if req.large_old_paths.is_empty() => StepOutcome::NotSelected,
            "large-old" => run(dispose_selected_with_sink(
                home,
                &req.large_old_paths,
                req.expected.large_old,
                req.confirm_mass_delete.large_old,
                sink,
                audit,
            )),
            // `DISPATCH_ORDER` is a constant in this file; a value not matched
            // above is a programming error, and treating it as "nothing to do"
            // would hide it. Refuse the whole run instead.
            // `DISPATCH_ORDER` is a constant in this file, so this is a
            // programming error rather than a reachable input. It still refuses
            // *into the ledger* rather than returning `Err` and discarding it:
            // an earlier step may already have moved files, and a caller that
            // gets no record of that has no way to tell the user what happened.
            other => StepOutcome::Refused {
                reason: format!("unknown Smart Scan source {other:?}"),
            },
        };

        if let StepOutcome::Refused { reason } = &outcome {
            stopped = Some(format!("{source} refused: {reason}"));
        }
        steps.push(Step {
            source: source.to_string(),
            outcome,
        });
    }

    Ok(SmartScanRunReport::from_steps(steps))
}

/// Real-app entry point: the system Trash, and the default audit log.
pub fn dispatch_smart_scan(req: SmartScanRequest) -> Result<SmartScanRunReport, String> {
    let home = crate::default_home().map_err(|e| e.to_string())?;
    let mut audit = AuditLog::open(&crate::default_audit_path().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    dispatch_smart_scan_with_sink(
        &SmartScanConfig::new(home),
        &req,
        &swept_core::executor::SystemSink,
        &mut audit,
    )
}

fn run(result: Result<CleanSummary, String>) -> StepOutcome {
    match result {
        Ok(summary) => StepOutcome::Executed { summary },
        Err(reason) => StepOutcome::Refused { reason },
    }
}

/// Why this report is too old to act on, if it is.
///
/// A clock that went backwards fails closed rather than computing an age of
/// zero: `now < stamped` is not a fresh report, it is a machine whose time
/// moved, and "fresh" is the wrong way to be wrong about it.
fn staleness(stamped_ms: u64, now: u64) -> Option<String> {
    if stamped_ms > now {
        return Some(
            "this scan is stamped in the future, so its age cannot be judged. Scan again \
             and review."
                .to_string(),
        );
    }
    let age = now - stamped_ms;
    if age > MAX_REPORT_AGE_MS {
        return Some(format!(
            "this scan is {} minutes old and the disk may have changed since. Scan again \
             and review.",
            age / 60_000
        ));
    }
    None
}
