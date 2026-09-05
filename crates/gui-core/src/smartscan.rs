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

use serde::Serialize;
use swept_core::privacy;
use swept_core::report::{CategorySummary, ScanReport};

use crate::{
    browser_root_for, build_config, large_and_old, privacy_report_from, probe_permissions,
    startup_report_in, Filters, LargeOldReportDto, Permissions, PrivacyRowDto,
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
