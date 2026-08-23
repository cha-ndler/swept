//! Tests for the GUI command layer. Fixtures only; deletion goes through an
//! injected `DirSink` so nothing touches the real Trash.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{Consent, DirSink};
use macclean_gui_core::{clean_with_sink, gui_consent, list_login_items, scan_report, Filters};

fn fake_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/Caches/app")).unwrap();
    (dir, home)
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn scan_report_reflects_fixtures_and_filters() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/big.bin"), &[0u8; 2048]);
    write(&home.join("Library/Caches/app/small.bin"), &[0u8; 10]);

    let all = scan_report(&home, &Filters::default());
    assert_eq!(all.total_count, 2);

    let large_only = scan_report(
        &home,
        &Filters {
            min_size_bytes: Some(1024),
            ..Default::default()
        },
    );
    assert_eq!(
        large_only.total_count, 1,
        "min_size filter should drop the small file"
    );
}

#[test]
fn login_items_listed_from_fixture() {
    let (_g, home) = fake_home();
    let la = home.join("Library/LaunchAgents");
    fs::create_dir_all(&la).unwrap();
    fs::write(
        la.join("com.example.foo.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>com.example.foo</string>
  <key>RunAtLoad</key><true/>
</dict></plist>"#,
    )
    .unwrap();
    let items = list_login_items(&home);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "com.example.foo");
}

#[test]
fn clean_dry_run_is_default_and_changes_nothing() {
    let (_g, home) = fake_home();
    let f = home.join("Library/Caches/app/a.bin");
    write(&f, b"data");
    let mut audit = AuditLog::open(&home.join("audit.jsonl")).unwrap();

    let summary = clean_with_sink(
        &home,
        &Filters::default(),
        None,
        None,
        Consent::default(),
        &DirSink {
            trash_dir: home.join("t"),
        },
        &mut audit,
    )
    .unwrap();

    assert!(summary.dry_run);
    assert_eq!(summary.executed, 0);
    assert!(f.exists(), "dry run must not delete anything");
}

#[test]
fn clean_with_consent_disposes_via_injected_sink() {
    let (_g, home) = fake_home();
    let f = home.join("Library/Caches/app/a.bin");
    write(&f, b"data");
    let trash_dir = home.join("test-bin");
    let mut audit = AuditLog::open(&home.join("audit.jsonl")).unwrap();

    let summary = clean_with_sink(
        &home,
        &Filters::default(),
        None,
        None,
        Consent {
            execute: true,
            ..Default::default()
        },
        &DirSink {
            trash_dir: trash_dir.clone(),
        },
        &mut audit,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!f.exists());
    assert!(trash_dir.join("a.bin").exists());
}

#[test]
fn clean_surfaces_mass_delete_refusal_as_err() {
    let (_g, home) = fake_home();
    for i in 0..(macclean_core::plan::MASS_DELETE_COUNT + 1) {
        write(&home.join(format!("Library/Caches/app/f{i}.bin")), b"x");
    }
    let mut audit = AuditLog::open(&home.join("audit.jsonl")).unwrap();

    let err = clean_with_sink(
        &home,
        &Filters::default(),
        None,
        None,
        Consent {
            execute: true,
            ..Default::default()
        },
        &DirSink {
            trash_dir: home.join("t"),
        },
        &mut audit,
    )
    .unwrap_err();
    assert!(err.contains("mass delete"), "got: {err}");
    assert!(
        home.join("Library/Caches/app/f0.bin").exists(),
        "nothing deleted on refusal"
    );
}

#[test]
fn clean_only_disposes_selected_categories() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/c.bin"), b"cache");
    fs::create_dir_all(home.join("Library/Logs")).unwrap();
    write(&home.join("Library/Logs/x.log"), b"log");
    let trash_dir = home.join("bin");
    let mut audit = AuditLog::open(&home.join("audit.jsonl")).unwrap();

    // Select only the caches category; logs must be left untouched.
    let summary = clean_with_sink(
        &home,
        &Filters::default(),
        Some(&["user-caches".to_string()]),
        None,
        gui_consent(true),
        &DirSink {
            trash_dir: trash_dir.clone(),
        },
        &mut audit,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(
        !home.join("Library/Caches/app/c.bin").exists(),
        "selected category should be disposed"
    );
    assert!(
        home.join("Library/Logs/x.log").exists(),
        "unselected category must be left untouched"
    );
}

#[test]
fn gui_consent_is_trash_only_never_permanent() {
    let c = gui_consent(false);
    assert!(c.execute);
    assert!(!c.allow_permanent, "GUI must never permanently delete");
    assert!(!c.confirmed_mass_delete);
    assert!(gui_consent(true).confirmed_mass_delete);
    assert!(!gui_consent(true).allow_permanent);
}

#[test]
fn an_empty_selection_disposes_nothing() {
    // Goes through `clean_at`, which owns the Vec -> Option decision. Calling
    // `clean_with_sink(Some(&[]))` directly would NOT be a regression gate:
    // that function never had the bug. The fail-open was `clean()` mapping an
    // empty vec to `None`, i.e. "no filter", i.e. every category.
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/c.bin"), b"cache");
    fs::create_dir_all(home.join("Library/Logs")).unwrap();
    write(&home.join("Library/Logs/x.log"), b"log");

    let summary = macclean_gui_core::clean_at(
        &home,
        &home.join("audit.jsonl"),
        &Filters::default(),
        Vec::new(),
        None,
        true,
        &DirSink {
            trash_dir: home.join("bin"),
        },
    )
    .unwrap();

    assert_eq!(
        summary.executed, 0,
        "an empty selection must dispose nothing"
    );
    assert!(home.join("Library/Caches/app/c.bin").exists());
    assert!(home.join("Library/Logs/x.log").exists());
}

#[test]
fn a_plan_that_grew_past_the_preview_is_refused() {
    // The plan is rebuilt at execute time, so the user's "yes" must be bound to
    // a magnitude. Confirming a 1-item preview must not authorize 40 items.
    let (_g, home) = fake_home();
    for i in 0..40 {
        write(&home.join(format!("Library/Caches/app/f{i}.bin")), b"data");
    }

    let err = macclean_gui_core::clean_at(
        &home,
        &home.join("audit.jsonl"),
        &Filters::default(),
        vec!["user-caches".to_string()],
        Some(macclean_gui_core::Expected { count: 1, bytes: 4 }),
        true,
        &DirSink {
            trash_dir: home.join("bin"),
        },
    )
    .unwrap_err();

    assert!(err.contains("changed since the preview"), "got: {err}");
    assert!(
        home.join("Library/Caches/app/f0.bin").exists(),
        "a refused clean must not dispose of anything"
    );
}

#[test]
fn ordinary_churn_between_preview_and_execute_is_allowed() {
    // Caches change in the seconds a user spends reading the sheet. A couple of
    // extra files must not block the clean, or the check is unusable.
    let (_g, home) = fake_home();
    for i in 0..3 {
        write(&home.join(format!("Library/Caches/app/f{i}.bin")), b"data");
    }

    let summary = macclean_gui_core::clean_at(
        &home,
        &home.join("audit.jsonl"),
        &Filters::default(),
        vec!["user-caches".to_string()],
        Some(macclean_gui_core::Expected { count: 2, bytes: 8 }),
        true,
        &DirSink {
            trash_dir: home.join("bin"),
        },
    )
    .unwrap();

    assert_eq!(summary.executed, 3);
}
