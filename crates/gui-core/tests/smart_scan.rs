//! Smart Scan's read-only half: the combined picture, and the number under the
//! button.
//!
//! The rule this file exists to enforce is one sentence: **every byte in the
//! headline belongs to a row a confirmed run would actually free.** A figure
//! that promises more than the dispatch will deliver is this project's own named
//! failure wearing a bigger number.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::executor::DirSink;
use swept_core::privacy::PrivacyConfig;
use swept_gui_core::smartscan::{smart_scan_in, SmartScanConfig};
use swept_gui_core::{
    clean_with_sink, dispose_privacy_with_sink, gui_consent, Acknowledged, Filters,
};

// --- fixtures ---------------------------------------------------------------

fn fixture_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for d in [
        "Library/Caches",
        "Library/Logs",
        "Documents",
        "Downloads",
        ".Trash",
    ] {
        fs::create_dir_all(home.join(d)).unwrap();
    }
    (dir, home)
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

fn chromium_profile(home: &Path, name: &str) -> PathBuf {
    let p = home
        .join("Library/Application Support/Google/Chrome")
        .join(name);
    write_sized(&p.join("Preferences"), 10);
    p
}

fn sink(home: &Path) -> DirSink {
    DirSink {
        trash_dir: home.join("FixtureTrash"),
    }
}

fn audit(home: &Path) -> AuditLog {
    AuditLog::open(&home.join("audit.jsonl")).unwrap()
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

fn filters() -> Filters {
    Filters {
        older_than_days: None,
        min_size_bytes: None,
    }
}

/// The real `large_old_min_size` is 100 MiB, which no fixture can afford to
/// create. Injected here for the same reason `PrivacyConfig::dir_limits` is.
fn config(home: &Path) -> SmartScanConfig {
    let mut cfg = SmartScanConfig::new(home.to_path_buf());
    cfg.large_old_min_size = 1024;
    cfg
}

/// Restores permissions on drop, so a panicking assertion still leaves a
/// removable tempdir.
struct Restore(PathBuf, fs::Permissions);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, self.1.clone());
    }
}

#[must_use]
fn unreadable(path: &Path) -> Restore {
    use std::os::unix::fs::PermissionsExt;
    let original = fs::metadata(path).unwrap().permissions();
    let mut locked = original.clone();
    locked.set_mode(0o000);
    fs::set_permissions(path, locked).unwrap();
    Restore(path.to_path_buf(), original)
}

// --- the number -------------------------------------------------------------

/// **The invariant.** Compute the headline, dispatch it immediately, and assert
/// the bytes actually freed are the bytes promised.
///
/// This is the one test that catches "a report of five things" at its root: it
/// cannot pass while the total counts anything the verbs would refuse, and it
/// needs no dry-run knob to say so.
#[test]
fn the_headline_number_equals_what_a_confirmed_run_actually_frees() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);
    write_sized(&home.join("Library/Logs/x.log"), 2048);
    // In the Trash, which is deliberately not a default.
    write_sized(&home.join(".Trash/old.bin"), 8192);
    // A browser cache: regenerable, so eligible.
    let profile = chromium_profile(&home, "Default");
    write_sized(&profile.join("GPUCache/data_1"), 1024);
    // A cookie jar: eligible for nothing.
    write_sized(&profile.join("Cookies"), 512);

    let report = smart_scan_in(&config(&home));
    let promised = report.selected.bytes;
    assert!(promised > 0, "the fixture produced nothing to promise");

    let categories: Vec<String> = report
        .cleanup
        .iter()
        .filter(|c| c.smart_scan_default)
        .map(|c| c.category.clone())
        .collect();
    let rows: Vec<String> = report.privacy.iter().map(|r| r.path.clone()).collect();

    let mut log = audit(&home);
    let s = sink(&home);
    let mut freed = clean_with_sink(
        &home,
        &filters(),
        Some(&categories),
        None,
        gui_consent(true),
        &s,
        &mut log,
    )
    .unwrap()
    .bytes_freed;
    if !rows.is_empty() {
        freed += dispose_privacy_with_sink(
            &PrivacyConfig::new(home.clone()),
            &rows,
            Acknowledged::default(),
            None,
            true,
            &s,
            &mut log,
        )
        .unwrap()
        .bytes_freed;
    }

    assert_eq!(
        freed, promised,
        "the headline promised {promised} and the run freed {freed}"
    );
    // And the things that were never promised are still there.
    assert!(home.join(".Trash/old.bin").exists());
    assert!(profile.join("Cookies").exists());
}

/// Only the sources that can be acted on contribute, and Space Lens is not one
/// of them — it measures a scope, not a set of proposals, and its bytes overlap
/// two other sources while missing the one the default comes from.
#[test]
fn the_total_names_only_the_sources_that_can_be_acted_on() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let report = smart_scan_in(&config(&home));

    assert_eq!(report.selected.from, vec!["cleanup", "privacy"]);
    assert_eq!(report.found.from, vec!["cleanup", "privacy", "large-old"]);
    for t in [&report.selected, &report.found] {
        assert!(!t.from.iter().any(|s| s == "space-lens"), "{:?}", t.from);
        assert!(!t.from.iter().any(|s| s == "uninstaller"), "{:?}", t.from);
        assert!(!t.from.iter().any(|s| s == "startup"), "{:?}", t.from);
    }
}

/// A gap names the source it belongs to, in that source's own words. One
/// boolean would say "some figure somewhere is short", which is not something a
/// notice on screen can be written from.
#[test]
fn a_partial_source_names_itself_rather_than_setting_a_bare_flag() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);
    let locked = home.join("Library/Caches/opaque");
    write_sized(&locked.join("hidden.bin"), 8192);

    let _shut = unreadable(&locked);
    let report = smart_scan_in(&config(&home));

    assert!(!report.selected.is_complete());
    let gap = report
        .selected
        .incomplete
        .iter()
        .find(|i| i.source == "cleanup")
        .expect("the gap names cleanup");
    assert!(gap.reason.contains("could not be read"), "{gap:?}");
}

/// Nothing found **and** something unreadable is not a clean Mac. This is the
/// combined-total form of the failure the CLI's summary already guards.
#[test]
fn an_empty_result_with_an_unreadable_source_is_not_a_clean_mac() {
    let (_g, home) = fixture_home();
    let locked = home.join("Library/Caches/opaque");
    write_sized(&locked.join("hidden.bin"), 8192);

    let _shut = unreadable(&locked);
    let report = smart_scan_in(&config(&home));

    assert_eq!(report.selected.bytes, 0);
    assert!(
        !report.selected.is_complete(),
        "zero with a gap must not be reportable as zero"
    );
}

/// No bare byte figure at the top level: every one is inside a `Total`, so a
/// frontend cannot render the number without holding its completeness.
#[test]
fn no_byte_figure_at_the_top_level_travels_without_its_completeness() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let report = smart_scan_in(&config(&home));
    let v: serde_json::Value = serde_json::to_value(&report).unwrap();
    let top = v.as_object().unwrap();

    for key in top.keys() {
        assert!(
            !key.ends_with("_bytes"),
            "`{key}` is a bare byte figure at the top level"
        );
    }
    // And the two that exist carry their provenance.
    for name in ["selected", "found"] {
        let t = &top[name];
        assert!(t["bytes"].is_u64(), "{name}");
        assert!(t["from"].is_array(), "{name}");
        assert!(t["incomplete"].is_array(), "{name}");
    }
}

// --- what may be ticked for you ---------------------------------------------

/// The Trash is the recovery mechanism for everything else the same gesture
/// does. It is reported and never pre-selected.
#[test]
fn the_trash_is_reported_and_never_selected() {
    let (_g, home) = fixture_home();
    write_sized(&home.join(".Trash/old.bin"), 8192);
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let report = smart_scan_in(&config(&home));
    let trash = report
        .cleanup
        .iter()
        .find(|c| c.category == "trash")
        .expect("the Trash is shown");

    assert!(!trash.smart_scan_default);
    assert_eq!(
        report.selected.bytes, 4096,
        "the Trash's bytes are not in the promise"
    );
}

/// Nothing that carries a consequence is ever pre-selected. `smart_scan_eligible`
/// is `offerable && regenerable`, derived beside the rows rather than assigned.
#[test]
fn nothing_with_a_consequence_is_ever_pre_selected() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    write_sized(&profile.join("Cookies"), 4096);
    write_sized(&profile.join("History"), 4096);
    write_sized(&profile.join("Current Session"), 4096);
    write_sized(&profile.join("GPUCache/data_1"), 1024);

    let report = smart_scan_in(&config(&home));

    assert!(!report.privacy.is_empty(), "the cache row is offered");
    for row in &report.privacy {
        assert_eq!(row.consequence, "regenerable", "{}", row.label);
    }
}

/// Large & Old is shown and never pre-selected — the whole point of that module
/// is that a human chooses each row.
#[test]
fn large_and_old_rows_are_never_pre_selected() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Downloads/big.iso"), 8192);

    let report = smart_scan_in(&config(&home));

    assert!(!report.selected.from.iter().any(|s| s == "large-old"));
    assert!(report.found.from.iter().any(|s| s == "large-old"));
}

/// A Large & Old row inside a browser's own data is not something Smart Scan may
/// count, because `dispose_selected_with_sink` refuses it. Counting it would
/// promise bytes no confirmed run could free.
#[test]
fn a_large_old_row_inside_a_browser_is_not_counted() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    write_sized(&profile.join("Login Data"), 4096);
    write_sized(&home.join("Downloads/big.iso"), 2048);

    let report = smart_scan_in(&config(&home));

    // The walk lists it — the read-only half stays honest.
    assert!(report
        .large_old
        .items
        .iter()
        .any(|i| i.path.ends_with("Login Data")));
    // The figure does not.
    let large_old_contribution =
        report.found.bytes - report.cleanup.iter().map(|c| c.bytes).sum::<u64>();
    assert_eq!(
        large_old_contribution, 2048,
        "only the row outside the browser is counted"
    );
}

// --- what is not a source ---------------------------------------------------

/// The Uninstaller is not a Smart Scan source: including it means building the
/// orphan sweep the roadmap left as an open question.
#[test]
fn the_uninstaller_is_not_a_smart_scan_source() {
    let (_g, home) = fixture_home();
    // A leftover-shaped directory for an app that is not installed.
    write_sized(
        &home.join("Library/Application Support/com.acme.Gone/db.sqlite"),
        4096,
    );

    let report = smart_scan_in(&config(&home));
    let v: serde_json::Value = serde_json::to_value(&report).unwrap();

    assert!(v.get("leftovers").is_none());
    assert!(v.get("uninstall").is_none());
    assert!(!report.found.from.iter().any(|s| s == "uninstaller"));
}

/// Startup is reported and carries no bytes and no selection — a move is not a
/// disposal, and a field that cannot exist cannot be summed into a total later.
#[test]
fn startup_is_a_finding_with_no_bytes_and_no_selection() {
    let (_g, home) = fixture_home();
    let agents = home.join("Library/LaunchAgents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(
        agents.join("com.acme.helper.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>com.acme.helper</string>
  <key>RunAtLoad</key><true/>
</dict></plist>"#,
    )
    .unwrap();

    let report = smart_scan_in(&config(&home));
    assert_eq!(report.startup.starts_at_login, 1);

    let v = serde_json::to_value(&report).unwrap();
    let startup = v["startup"].as_object().unwrap();
    for key in startup.keys() {
        assert!(!key.contains("bytes"), "startup grew a byte figure: {key}");
        assert!(!key.contains("select"), "startup grew a selection: {key}");
    }
}

// --- the disjointness this design rests on ----------------------------------

/// There is no overlap-folding here because there is no overlap, and that is a
/// property of two lists someone may widen.
///
/// `default_roots` is what cleanup draws from; `discovery_roots` is what Large &
/// Old draws from. If they ever intersect, one file would be counted twice and
/// the headline would stop matching what a run frees.
#[test]
fn the_scopes_the_sources_draw_from_do_not_overlap() {
    let home = Path::new("/Users/fixture");
    let disposal = safety::allowlist::default_roots(home);
    let discovery = safety::allowlist::discovery_roots(home);

    for d in &disposal {
        for s in &discovery {
            assert!(
                !d.starts_with(s) && !s.starts_with(d),
                "{} and {} overlap, so a file could be counted twice",
                d.display(),
                s.display()
            );
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap()
}

// --- dispatch ---------------------------------------------------------------
//
// The claim under test is exactly one sentence: **no step begins after a step
// refused.** Not "the run is atomic" — `executor::execute` already continues
// past a failed action inside step one — and the difference between those two
// sentences is what these tests exist to keep honest.

use swept_gui_core::smartscan::{
    dispatch_smart_scan_with_sink, SmartScanExpected, SmartScanRequest, StepOutcome,
    MAX_REPORT_AGE_MS,
};

fn request(report_stamp: u64) -> SmartScanRequest {
    serde_json::from_value(serde_json::json!({
        "scanned_at_ms": report_stamp,
        "categories": [],
        "privacy_paths": [],
        "large_old_paths": [],
        "filters": {},
        "confirm_mass_delete": { "cleanup": true, "privacy": true, "large_old": true },
    }))
    .unwrap()
}

/// A magnitude that asserts nothing — which the dispatcher now refuses, because
/// `{0, 0}` is what a frontend that lost its sheet state sends.
///
/// Kept only for the test that pins that refusal. Everywhere else use
/// [`confirmed`], whose count is what the sheet would really have shown.
fn nothing_confirmed() -> Option<swept_gui_core::Expected> {
    Some(swept_gui_core::Expected { count: 0, bytes: 0 })
}

/// A count that clears the "say how many" gate without pinning a magnitude.
///
/// Sound for cleanup only: its drift check is `grew_beyond`, which allows 25
/// items or 64 MiB of cache churn above what was confirmed. Privacy and
/// Large & Old match the count exactly, so they need [`confirmed`] with the
/// real number.
fn some_cleanup() -> Option<swept_gui_core::Expected> {
    Some(swept_gui_core::Expected { count: 1, bytes: 1 })
}

/// What the sheet would actually have shown for a set of rows.
fn confirmed(count: usize, bytes: u64) -> Option<swept_gui_core::Expected> {
    Some(swept_gui_core::Expected { count, bytes })
}

fn outcome<'a>(r: &'a swept_gui_core::smartscan::SmartScanRunReport, src: &str) -> &'a StepOutcome {
    &r.steps
        .iter()
        .find(|s| s.source == src)
        .unwrap_or_else(|| panic!("no step for {src}"))
        .outcome
}

/// A refusal in one module leaves the later modules **not attempted**, and the
/// earlier module's work is still reported.
///
/// A refusal is evidence about the world, not about the module that reported
/// it: it says the report the user confirmed no longer describes the disk.
#[test]
fn a_refusal_stops_the_run_and_the_later_steps_say_they_were_not_attempted() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);
    write_sized(&home.join("Downloads/big.iso"), 2048);

    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();
    // Privacy is asked for a row that is not there, so it refuses.
    req.privacy_paths = vec![home
        .join("Library/Application Support/Google/Chrome/Default/Cookies")
        .display()
        .to_string()];
    req.expected.privacy = confirmed(1, 0);
    req.large_old_paths = vec![home.join("Downloads/big.iso").display().to_string()];
    req.expected.large_old = confirmed(1, 4096);

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    assert!(
        matches!(outcome(&run, "cleanup"), StepOutcome::Executed { .. }),
        "{:?}",
        outcome(&run, "cleanup")
    );
    assert!(matches!(
        outcome(&run, "privacy"),
        StepOutcome::Refused { .. }
    ));
    assert!(matches!(
        outcome(&run, "large-old"),
        StepOutcome::NotAttempted { .. }
    ));
    assert!(!run.completed);

    // The step that ran, ran — and the step that never began did not.
    assert!(!home.join("Library/Caches/app/blob.bin").exists());
    assert!(home.join("Downloads/big.iso").exists());
    assert_eq!(run.bytes_freed, 4096);
}

/// **"We did not try" must not serialize like "we tried and there was
/// nothing."** That is the ledger form of this project's own named failure.
#[test]
fn a_step_that_was_not_attempted_is_distinguishable_from_one_that_did_nothing() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    // Nothing selected anywhere but cleanup: the other two are NotSelected.
    let mut quiet = request(now_ms());
    quiet.categories = vec!["user-caches".to_string()];
    quiet.expected.cleanup = some_cleanup();
    let mut log = audit(&home);
    let a = dispatch_smart_scan_with_sink(&config(&home), &quiet, &sink(&home), &mut log).unwrap();

    // And a run where privacy refuses, so large-old is NotAttempted.
    let (_g2, home2) = fixture_home();
    write_sized(&home2.join("Library/Caches/app/blob.bin"), 4096);
    write_sized(&home2.join("Downloads/big.iso"), 2048);
    let mut broken = request(now_ms());
    broken.categories = vec!["user-caches".to_string()];
    broken.expected.cleanup = some_cleanup();
    broken.privacy_paths = vec![home2.join("Library/nope").display().to_string()];
    broken.expected.privacy = confirmed(1, 0);
    broken.large_old_paths = vec![home2.join("Downloads/big.iso").display().to_string()];
    broken.expected.large_old = confirmed(1, 2048);
    let mut log2 = audit(&home2);
    let b =
        dispatch_smart_scan_with_sink(&config(&home2), &broken, &sink(&home2), &mut log2).unwrap();

    let quiet_json = serde_json::to_value(outcome(&a, "large-old")).unwrap();
    let broken_json = serde_json::to_value(outcome(&b, "large-old")).unwrap();

    assert_eq!(quiet_json["outcome"], "not_selected");
    assert_eq!(broken_json["outcome"], "not_attempted");
    assert_ne!(
        quiet_json, broken_json,
        "the two must be distinguishable on the wire, not only in Rust"
    );
    // And only one of them is a completed run.
    assert!(a.completed);
    assert!(!b.completed);
}

/// Nothing selected anywhere is a refusal that is recorded, not a silent no-op.
#[test]
fn nothing_selected_anywhere_is_a_recorded_refusal() {
    let (_g, home) = fixture_home();
    let mut log = audit(&home);
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &request(now_ms()), &sink(&home), &mut log)
            .unwrap_err();

    assert!(err.contains("nothing was selected"), "{err}");
    let text = std::fs::read_to_string(home.join("audit.jsonl")).unwrap();
    assert!(
        text.contains("refus"),
        "the refusal must reach the log:\n{text}"
    );
}

// --- routing, which is the surface M7 actually adds -------------------------

/// **The negative case that matters most, aimed at the case that can actually
/// happen.**
///
/// The first version of this test routed a `Cookies` path — a row Smart Scan
/// never offers — so it exercised a hazard the gesture cannot produce. The real
/// one is the opposite: `smart_scan_eligible` is `offerable && regenerable`, so
/// **every privacy row Smart Scan offers is exactly the class
/// `dispose_selected_with_sink`'s browser boundary waves through.** Delegating
/// to that boundary would have left this covered only by an incidental
/// `is_dir: true` in a spec table in another crate.
#[test]
fn a_privacy_row_smart_scan_actually_offers_cannot_be_routed_to_large_and_old() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let member = profile.join("GPUCache/data_1");
    write_sized(&member, 4096);

    let report = smart_scan_in(&config(&home));
    let row = report
        .privacy
        .first()
        .expect("a regenerable row is offered")
        .path
        .clone();

    // The row itself, and a member file inside it. The second is the one that
    // slipped through before: a directory is refused for being a directory, but
    // a file inside it was disposed of.
    for path in [row, member.display().to_string()] {
        let mut req = request(now_ms());
        req.large_old_paths = vec![path.clone()];
        req.expected.large_old = confirmed(1, 4096);

        let mut log = audit(&home);
        let err = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log)
            .unwrap_err();
        assert!(err.contains("browser"), "{path}: {err}");
    }
    assert!(member.exists(), "the file survived both attempts");
}

/// A cookie jar is refused too, by the same gate — but note this is the *easy*
/// case, and the test above is the one that matters.
#[test]
fn a_consequence_carrying_path_sent_as_a_large_and_old_path_is_refused() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    let mut req = request(now_ms());
    req.large_old_paths = vec![cookies.display().to_string()];
    req.expected.large_old = confirmed(1, 4096);

    let mut log = audit(&home);
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("browser"), "{err}");
    assert!(cookies.exists());
}

/// `deny_unknown_fields` — a frontend sending a field this backend does not know
/// gets a refusal rather than a silent omission. It is also what stops a
/// `leftover_paths` field appearing later without a deliberate edit.
#[test]
fn a_request_carrying_a_field_this_backend_does_not_know_is_refused() {
    let err = serde_json::from_value::<SmartScanRequest>(serde_json::json!({
        "scanned_at_ms": 0,
        "categories": [],
        "privacy_paths": [],
        "large_old_paths": [],
        "leftover_paths": ["/tmp/anything"],
    }))
    .unwrap_err();

    assert!(err.to_string().contains("leftover_paths"), "{err}");
}

/// No aggregate `Expected`: a combined count could not be checked against any
/// single verb's rescan, and inventing a combined tolerance would be inventing a
/// looser one.
#[test]
fn there_is_no_aggregate_expected_to_confirm_against() {
    let err = serde_json::from_value::<SmartScanExpected>(serde_json::json!({
        "count": 3,
        "bytes": 100,
    }))
    .unwrap_err();

    assert!(err.to_string().contains("unknown field"), "{err}");
}

// --- freshness --------------------------------------------------------------

/// A report held open too long is refused. Honestly a guard against our own UI,
/// not authentication — a frontend can send any number it likes.
#[test]
fn a_report_older_than_the_freshness_budget_is_refused() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let mut req = request(now_ms() - MAX_REPORT_AGE_MS - 1000);
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();

    let mut log = audit(&home);
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("old"), "{err}");
    assert!(
        home.join("Library/Caches/app/blob.bin").exists(),
        "nothing ran"
    );
}

/// A clock that went backwards fails **closed**. `now < stamped` is not a fresh
/// report, it is a machine whose time moved, and "fresh" is the wrong way to be
/// wrong about it.
#[test]
fn a_scan_stamped_in_the_future_is_refused_rather_than_treated_as_fresh() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let mut req = request(now_ms() + 60_000);
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();

    let mut log = audit(&home);
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("future"), "{err}");
    assert!(home.join("Library/Caches/app/blob.bin").exists());
}

/// The freshness check is **additive**. A perfectly fresh token must not buy
/// past any of the refusals that were already there — deleting the check should
/// leave every other one intact.
#[test]
fn the_freshness_check_is_additive_and_a_stale_selection_is_still_refused() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    // Fresh as can be, and still routed wrongly.
    let mut req = request(now_ms());
    req.large_old_paths = vec![cookies.display().to_string()];
    req.expected.large_old = confirmed(1, 4096);

    let mut log = audit(&home);
    // A whole-request refusal, which is stronger than the step-level one this
    // test was first written against: the gate that catches it runs before any
    // step, so a fresh token buys nothing at all.
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("browser"), "{err}");
    assert!(cookies.exists());
}

/// The one assertion the other three disposal verbs make, made before any step
/// runs — a run that got halfway on an untrustworthy home is worse than one
/// that never started.
#[test]
fn a_non_canonical_home_refuses_before_any_step_runs() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let mut cfg = config(&home);
    cfg.home = home.join("Documents/..");
    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();

    let mut log = audit(&home);
    let err = dispatch_smart_scan_with_sink(&cfg, &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("canonical"), "{err}");
    assert!(home.join("Library/Caches/app/blob.bin").exists());
}

/// The headline the report promised is the amount a confirmed run frees — now
/// through the dispatcher rather than by calling the verbs by hand.
#[test]
fn dispatching_the_default_selection_frees_exactly_what_was_promised() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);
    write_sized(&home.join("Library/Logs/x.log"), 2048);
    write_sized(&home.join(".Trash/old.bin"), 8192);
    let profile = chromium_profile(&home, "Default");
    write_sized(&profile.join("GPUCache/data_1"), 1024);

    let report = smart_scan_in(&config(&home));

    let mut req = request(report.scanned_at_ms);
    req.categories = report
        .cleanup
        .iter()
        .filter(|c| c.smart_scan_default)
        .map(|c| c.category.clone())
        .collect();
    req.privacy_paths = report.privacy.iter().map(|r| r.path.clone()).collect();
    // The magnitudes the sheet would have shown, per source — because privacy
    // matches its count exactly and would refuse anything else.
    let picked: Vec<_> = report
        .cleanup
        .iter()
        .filter(|c| c.smart_scan_default)
        .collect();
    req.expected.cleanup = confirmed(
        picked.iter().map(|c| c.count).sum(),
        picked.iter().map(|c| c.bytes).sum(),
    );
    if !req.privacy_paths.is_empty() {
        req.expected.privacy = confirmed(
            report.privacy.len(),
            report.privacy.iter().map(|r| r.size_bytes).sum(),
        );
    }

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    assert!(run.completed);
    assert_eq!(run.bytes_freed, report.selected.bytes);
    assert!(home.join(".Trash/old.bin").exists(), "never promised");
}

// --- what the review found, and what nothing was aiming at ------------------

/// **The preview and the action must be built from the same configuration.**
///
/// The first version took the cleaner filters from the *config* while the report
/// took them from the request, so the two diverged by construction — always in
/// the widening direction. Measured before the fix: a report built with a size
/// floor promised one item and the run removed two, including the file the
/// filter had excluded and the user never saw.
#[test]
fn the_filters_the_report_was_built_with_are_the_filters_the_run_uses() {
    let (_g, home) = fixture_home();
    let big = home.join("Library/Caches/app/big.bin");
    let small = home.join("Library/Caches/app/small.bin");
    write_sized(&big, 4096);
    write_sized(&small, 8);

    let filters = Filters {
        older_than_days: None,
        min_size_bytes: Some(1024),
    };
    let mut cfg = config(&home);
    cfg.filters = filters.clone();
    let report = smart_scan_in(&cfg);

    // The report offers exactly one item, because the floor excluded the other.
    let picked: Vec<_> = report
        .cleanup
        .iter()
        .filter(|c| c.smart_scan_default)
        .collect();
    assert_eq!(picked.iter().map(|c| c.count).sum::<usize>(), 1);

    let mut req = request(report.scanned_at_ms);
    req.filters = filters;
    req.categories = picked.iter().map(|c| c.category.clone()).collect();
    req.expected.cleanup = confirmed(1, 4096);

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    assert_eq!(run.bytes_freed, 4096);
    assert!(!big.exists(), "the file that was offered is gone");
    assert!(
        small.exists(),
        "the file the filter excluded was never offered and must survive"
    );
}

/// A combined gesture may not act on a source without saying how many rows it
/// confirmed — and **a zero is not an answer**.
///
/// Each verb takes `Option<Expected>` because a single screen may legitimately
/// act without one. This is the only place where one confirmation stands for
/// three magnitudes. `{0, 0}` satisfies "is there an `Expected`" while asserting
/// nothing, and `grew_beyond` then allows 25 items and 64 MiB above it — so a
/// frontend that lost its sheet state would clean twenty files it had confirmed
/// as none.
#[test]
fn a_source_that_names_rows_without_saying_how_many_is_refused() {
    let (_g, home) = fixture_home();
    for i in 0..20 {
        write_sized(&home.join(format!("Library/Caches/app/f{i:04}.bin")), 4096);
    }

    for expected in [None, nothing_confirmed()] {
        let mut req = request(now_ms());
        req.categories = vec!["user-caches".to_string()];
        req.expected.cleanup = expected;

        let mut log = audit(&home);
        let err = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log)
            .unwrap_err();

        assert!(err.contains("did not say how many"), "{err}");
    }
    assert!(home.join("Library/Caches/app/f0000.bin").exists());
}

/// A category id the registry does not know cannot have been on the report.
/// `clean_with_sink` would filter it out and the ledger would read
/// `Executed { executed: 0 }` — the shape of a successful run over nothing.
#[test]
fn a_category_the_registry_does_not_know_is_refused_not_silently_dropped() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string(), "not-a-category".to_string()];
    req.expected.cleanup = some_cleanup();

    let mut log = audit(&home);
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("not-a-category"), "{err}");
    assert!(home.join("Library/Caches/app/blob.bin").exists());
}

/// **One boolean cannot answer three questions.** `requires_confirmation` is
/// evaluated independently by each verb against its own count, so confirming a
/// mass delete for one source must not confirm it for another whose count the
/// user never saw.
#[test]
fn confirming_a_mass_delete_for_one_source_does_not_confirm_it_for_another() {
    let (_g, home) = fixture_home();
    // Enough rows in the Trash to cross MASS_DELETE_COUNT for cleanup.
    for i in 0..(swept_core::plan::MASS_DELETE_COUNT + 5) {
        write_sized(&home.join(format!("Library/Caches/app/f{i:04}.bin")), 16);
    }

    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();
    // Confirmed for the *other* two sources only.
    req.confirm_mass_delete = serde_json::from_value(serde_json::json!({
        "cleanup": false, "privacy": true, "large_old": true
    }))
    .unwrap();

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    match outcome(&run, "cleanup") {
        StepOutcome::Refused { reason } => assert!(reason.contains("confirm"), "{reason}"),
        other => panic!("a mass delete cleanup must not proceed unconfirmed: {other:?}"),
    }
    assert!(home.join("Library/Caches/app/f0000.bin").exists());
}

/// A step can execute and still leave something behind. `completed` must not
/// read "done" over that — `executor::execute` continues past a failed action
/// and reports it in `CleanSummary::refused`.
#[test]
fn the_ledger_carries_action_level_refusals_separately_from_step_level_ones() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);

    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    // Nothing was refused here, so the two agree — the point is that the field
    // exists and is summed, so a run that did leave something behind cannot
    // report itself complete.
    assert_eq!(run.actions_refused, 0);
    assert!(run.completed);
    let v = serde_json::to_value(&run).unwrap();
    assert!(
        v.get("actions_refused").is_some(),
        "the count must reach the UI, not only Rust"
    );
}

/// The refusal that used to leave no trace. It is step one of a three-source
/// gesture now, so it aborts the whole run — and a log that says nothing about
/// why is the gap `record_run_refusal` exists to close.
#[test]
fn a_cleanup_drift_refusal_reaches_the_audit_log() {
    let (_g, home) = fixture_home();
    // `grew_beyond` absorbs 25 items or 64 MiB of ordinary cache churn, so the
    // drift has to be bigger than that to be a drift at all.
    for i in 0..40 {
        write_sized(&home.join(format!("Library/Caches/app/f{i:04}.bin")), 16);
    }

    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string()];
    // Confirmed one item where forty are there — past the churn allowance.
    req.expected.cleanup = confirmed(1, 16);
    req.filters = Filters {
        older_than_days: None,
        min_size_bytes: None,
    };

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    assert!(matches!(
        outcome(&run, "cleanup"),
        StepOutcome::Refused { .. }
    ));
    let text = std::fs::read_to_string(home.join("audit.jsonl")).unwrap();
    assert!(
        text.contains("refus"),
        "the refusal that aborted the run must be in the log:\n{text}"
    );
    assert!(home.join("Library/Caches/app/f0000.bin").exists());
}

// --- the three offer-set predicates, each with its own reproduction ---------
//
// The dispatcher's job is that **the set it will act on is the set the report
// offered**. Three predicates define that set, and the first version enforced
// only one of them. Each of these was a working data-loss path.

/// **A privacy row the report did not offer must not be disposable.**
///
/// `smart_scan_eligible` is `offerable && regenerable`, while
/// `dispose_privacy_with_sink` accepts `offerable && !SiteStorage && !withheld
/// && !undisposable` — strictly larger. The only thing between them is
/// `acknowledgement_missing`, so threading the frontend's acknowledgements
/// through was enough to sign a person out of every site from a gesture whose
/// sheet never used the words.
#[test]
fn a_privacy_row_the_report_did_not_offer_cannot_be_disposed_of() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    let history = profile.join("History");
    write_sized(&cookies, 4096);
    write_sized(&history, 4096);

    let report = smart_scan_in(&config(&home));
    assert!(
        !report.privacy.iter().any(|r| r.path == s(&cookies)),
        "the fixture must not offer the cookie jar"
    );

    let mut req = request(now_ms());
    req.privacy_paths = vec![s(&cookies), s(&history)];
    req.expected.privacy = confirmed(2, 8192);

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    match outcome(&run, "privacy") {
        StepOutcome::Refused { .. } => {}
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(cookies.exists(), "still signed in");
    assert!(history.exists());
    assert!(!run.completed);
}

/// **A file below the large-file floor was never offered as one.**
///
/// `dispose_selected_with_sink`'s ceiling is the discovery scope — every
/// non-directory file in Documents, Downloads, Pictures and the rest, at any
/// size. Without the floor, a gesture whose report listed no large files at all
/// could trash a small document.
#[test]
fn a_file_below_the_large_file_floor_is_refused() {
    let (_g, home) = fixture_home();
    let thesis = home.join("Documents/thesis.pdf");
    write_sized(&thesis, 64);

    let report = smart_scan_in(&config(&home));
    assert!(
        report.large_old.items.is_empty(),
        "the fixture must offer no large files"
    );

    let mut req = request(now_ms());
    req.large_old_paths = vec![s(&thesis)];
    req.expected.large_old = confirmed(1, 64);

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    // Checked in the step rather than at run start, because large-old runs after
    // two other scans and a file can shrink in that window.
    match outcome(&run, "large-old") {
        StepOutcome::Refused { reason } => assert!(reason.contains("floor"), "{reason}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(thesis.exists());
    assert!(!run.completed);
}

/// The paired negative: a file that really is above the floor still goes.
#[test]
fn a_file_above_the_large_file_floor_is_still_disposable() {
    let (_g, home) = fixture_home();
    let iso = home.join("Downloads/big.iso");
    write_sized(&iso, 4096);

    let mut req = request(now_ms());
    req.large_old_paths = vec![s(&iso)];
    req.expected.large_old = confirmed(1, 4096);

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    assert!(matches!(
        outcome(&run, "large-old"),
        StepOutcome::Executed { .. }
    ));
    assert!(!iso.exists());
}

/// **A category that exists is not a category this gesture may tick.** The
/// Trash is `Some` for `by_id` and `false` for `smart_scan_default`, and
/// emptying it here would destroy the undo for every other module in the same
/// click — while reporting bytes that were never freed, because re-trashing
/// something already in `~/.Trash` frees nothing.
#[test]
fn a_category_the_gesture_never_offers_is_refused_even_though_it_exists() {
    let (_g, home) = fixture_home();
    let old = home.join(".Trash/old.bin");
    write_sized(&old, 8192);

    let mut req = request(now_ms());
    req.categories = vec!["trash".to_string()];
    req.expected.cleanup = some_cleanup();

    let mut log = audit(&home);
    let err =
        dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap_err();

    assert!(err.contains("never part of the combined action"), "{err}");
    assert!(old.exists(), "the undo for every other module survives");
}

/// **The widening default.** `filters` is the one field whose permissive value
/// would have been its serde default, so omitting it reproduced the original
/// preview-versus-action bug exactly. It has no default now.
#[test]
fn a_request_that_omits_its_filters_is_refused_rather_than_run_wide_open() {
    let err = serde_json::from_value::<SmartScanRequest>(serde_json::json!({
        "scanned_at_ms": 0,
        "categories": ["user-caches"],
        "privacy_paths": [],
        "large_old_paths": [],
    }))
    .unwrap_err();

    assert!(err.to_string().contains("filters"), "{err}");
}

/// A misspelled filter key would deserialize to "no floor at all", which is the
/// one direction a mistake here must not go.
#[test]
fn a_misspelled_filter_key_is_refused_rather_than_silently_widening() {
    let err = serde_json::from_value::<SmartScanRequest>(serde_json::json!({
        "scanned_at_ms": 0,
        "filters": { "min_size": 1024 },
        "categories": [],
        "privacy_paths": [],
        "large_old_paths": [],
    }))
    .unwrap_err();

    assert!(err.to_string().contains("min_size"), "{err}");
}

/// There is no acknowledgement axis on this request at all. Removing the field
/// is what makes the privacy ceiling structural rather than a predicate someone
/// can forget to call.
#[test]
fn the_request_carries_no_acknowledgement_axis() {
    let err = serde_json::from_value::<SmartScanRequest>(serde_json::json!({
        "scanned_at_ms": 0,
        "filters": {},
        "categories": [],
        "privacy_paths": [],
        "large_old_paths": [],
        "acknowledged": { "signs_you_out": true },
    }))
    .unwrap_err();

    assert!(err.to_string().contains("acknowledged"), "{err}");
}

/// When a refusal stops the gesture, the log says so. `NotAttempted` leaves no
/// trace of its own, so without this the fact that one refusal stopped two
/// other steps lived only in the value returned to the frontend.
#[test]
fn a_gesture_that_aborted_says_so_in_the_log() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Library/Caches/app/blob.bin"), 4096);
    write_sized(&home.join("Downloads/big.iso"), 4096);

    let mut req = request(now_ms());
    req.categories = vec!["user-caches".to_string()];
    req.expected.cleanup = some_cleanup();
    req.privacy_paths = vec![s(&home.join("Library/nope"))];
    req.expected.privacy = confirmed(1, 0);
    req.large_old_paths = vec![s(&home.join("Downloads/big.iso"))];
    req.expected.large_old = confirmed(1, 4096);

    let mut log = audit(&home);
    let run = dispatch_smart_scan_with_sink(&config(&home), &req, &sink(&home), &mut log).unwrap();

    assert!(!run.completed);
    let text = std::fs::read_to_string(home.join("audit.jsonl")).unwrap();
    assert!(
        text.contains("smart scan stopped after a refusal"),
        "the abort must be reconstructable from the log alone:\n{text}"
    );
}
