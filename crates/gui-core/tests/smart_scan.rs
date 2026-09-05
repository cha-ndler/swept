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
