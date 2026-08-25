//! Large & Old Files at the command layer: the read-only report, and the
//! grant-based disposal that is the only thing in the app which acts outside
//! `allowlist::default_roots`.
//!
//! The disposal half gets the adversarial treatment, because it is where a
//! frontend bug or a compromised webview would show up. The rule being tested
//! throughout: **a partial run is never acceptable** — if any part of the
//! selection is no longer exactly what the user confirmed, nothing is touched.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{DirSink, MAX_GRANTS};
use macclean_gui_core::{dispose_selected_with_sink, large_and_old, Expected};

fn fixture_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for d in ["Documents", "Downloads", "Library/Caches"] {
        fs::create_dir_all(home.join(d)).unwrap();
    }
    (dir, home)
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

fn audit_at(home: &Path) -> (PathBuf, AuditLog) {
    let p = home.join("audit.jsonl");
    let log = AuditLog::open(&p).unwrap();
    (p, log)
}

fn sink(home: &Path) -> DirSink {
    DirSink {
        trash_dir: home.join("test-trash"),
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

// --- the read-only half -----------------------------------------------------

#[test]
fn the_report_serializes_the_shape_the_ui_expects() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/big.iso"), 4096);

    let dto = large_and_old(&home, 1024, None);
    let json = serde_json::to_string(&dto).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(!v["items"].as_array().unwrap().is_empty());
    assert!(v["matched"].is_number());
    assert!(v["matched_bytes"].is_number());
    assert!(v["partial"].is_boolean());
    // Nothing is pre-selected: there is no such field to be true.
    assert!(
        v["items"][0].get("selected").is_none(),
        "a Large & Old row must not carry a selection state"
    );
}

#[test]
fn an_absurd_age_filter_does_not_wrap_into_no_filter_at_all() {
    // `older_than_days * 86_400` panics in debug and *wraps* in release. A
    // wrapped threshold turns "older than N days" into a near-zero one, so
    // freshly-modified files are presented to the user as old — and those rows
    // are exactly what they then grant.
    //
    // The input matters. `u64::MAX` looks like the obvious adversarial value
    // but wraps to ~2^64 — still astronomically large, so the filter still
    // excludes everything and the test passes either way. (An earlier version
    // of this test used it and did not catch the bug.) `2^57` is the real one:
    // 86_400 = 2^7 · 675, so 2^57 · 86_400 ≡ 0 (mod 2^64) — the threshold
    // collapses to *zero seconds* and every file on disk becomes "old".
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/fresh.iso"), 4096);

    let dto = large_and_old(&home, 1024, Some(1u64 << 57));

    assert!(
        dto.items.is_empty(),
        "a saturated age threshold excludes everything; it must not wrap to zero \
         and present a freshly-written file as old"
    );
}

// --- the disposal half ------------------------------------------------------

#[test]
fn a_selected_file_outside_the_allowlist_is_disposed() {
    let (_g, home) = fixture_home();
    let f = home.join("Documents/big.iso");
    write_sized(&f, 4096);
    let (_p, mut audit) = audit_at(&home);

    let summary =
        dispose_selected_with_sink(&home, &[s(&f)], None, false, &sink(&home), &mut audit).unwrap();

    assert_eq!(summary.executed, 1);
    assert_eq!(summary.bytes_freed, 4096, "the size is read from disk");
    assert!(!f.exists());
    assert!(home.join("test-trash/big.iso").exists(), "recoverable");
}

#[test]
fn an_empty_selection_acts_on_nothing() {
    let (_g, home) = fixture_home();
    let f = home.join("Documents/big.iso");
    write_sized(&f, 4096);
    let (_p, mut audit) = audit_at(&home);

    let err =
        dispose_selected_with_sink(&home, &[], None, false, &sink(&home), &mut audit).unwrap_err();

    assert!(err.contains("nothing was selected"), "{err}");
    assert!(f.exists());
}

#[test]
fn a_protected_path_in_the_selection_refuses_the_whole_request() {
    // The load-bearing one. A frontend that sends a protected path alongside
    // valid ones must not get the valid ones acted on — that is a partial run
    // the user never confirmed.
    let (_g, home) = fixture_home();
    let ok = home.join("Documents/big.iso");
    let mail = home.join("Library/Mail/messages.db");
    write_sized(&ok, 4096);
    write_sized(&mail, 4096);
    let (_p, mut audit) = audit_at(&home);

    let err = dispose_selected_with_sink(
        &home,
        &[s(&ok), s(&mail)],
        None,
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("no longer valid"), "{err}");
    assert!(ok.exists(), "the valid item must NOT have been acted on");
    assert!(mail.exists());
}

#[test]
fn a_path_that_vanished_refuses_the_whole_request() {
    let (_g, home) = fixture_home();
    let present = home.join("Documents/here.iso");
    write_sized(&present, 4096);
    let gone = home.join("Documents/gone.iso");
    let (_p, mut audit) = audit_at(&home);

    let err = dispose_selected_with_sink(
        &home,
        &[s(&present), s(&gone)],
        None,
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("no longer valid"), "{err}");
    assert!(present.exists(), "nothing partial");
}

#[test]
fn a_directory_in_the_selection_refuses_the_whole_request() {
    // The walk never returns a directory, so one arriving here means the UI
    // sent something it did not get from us.
    let (_g, home) = fixture_home();
    let dir = home.join("Documents/folder");
    let inner = dir.join("inner.iso");
    write_sized(&inner, 4096);
    let ok = home.join("Documents/big.iso");
    write_sized(&ok, 4096);
    let (_p, mut audit) = audit_at(&home);

    let err = dispose_selected_with_sink(
        &home,
        &[s(&ok), s(&dir)],
        None,
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("is a directory"), "{err}");
    assert!(dir.exists());
    assert!(inner.exists());
    assert!(ok.exists(), "nothing partial");
}

#[test]
fn the_same_file_twice_counts_once() {
    // Two spellings of one file must not inflate the total the mass-delete
    // threshold is measured against, nor produce a bogus "already gone" on the
    // second pass.
    let (_g, home) = fixture_home();
    let f = home.join("Documents/big.iso");
    write_sized(&f, 4096);
    let (_p, mut audit) = audit_at(&home);

    let summary = dispose_selected_with_sink(
        &home,
        &[s(&f), s(&f)],
        None,
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert_eq!(summary.refused, 0, "the duplicate is not a failed action");
    assert_eq!(summary.bytes_freed, 4096, "and is not counted twice");
}

#[test]
fn each_disposal_is_audited_as_user_granted() {
    // A reviewer reading the log must be able to tell a Large & Old removal
    // from a routine cache sweep: only the first is one-off human judgement.
    let (_g, home) = fixture_home();
    let f = home.join("Documents/big.iso");
    write_sized(&f, 4096);
    let (audit_path, mut audit) = audit_at(&home);

    dispose_selected_with_sink(&home, &[s(&f)], None, false, &sink(&home), &mut audit).unwrap();

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("user-granted"), "in:\n{log}");
    assert!(log.contains("big.iso"));
}

#[test]
fn a_large_selection_still_needs_mass_delete_confirmation() {
    let (_g, home) = fixture_home();
    let mut paths = Vec::new();
    for i in 0..(macclean_core::plan::MASS_DELETE_COUNT + 1) {
        let f = home.join(format!("Documents/f{i}.iso"));
        write_sized(&f, 16);
        paths.push(s(&f));
    }
    let (_p, mut audit) = audit_at(&home);

    let err = dispose_selected_with_sink(&home, &paths, None, false, &sink(&home), &mut audit)
        .unwrap_err();

    assert!(err.contains("needs explicit confirmation"), "{err}");
    assert!(home.join("Documents/f0.iso").exists());

    // ...and goes through once confirmed.
    let (_p2, mut audit2) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &paths, None, true, &sink(&home), &mut audit2).unwrap();
    assert_eq!(summary.executed, paths.len());
}

#[test]
fn a_selection_beyond_the_grant_cap_is_refused_wholesale() {
    let (_g, home) = fixture_home();
    let mut paths = Vec::new();
    for i in 0..(MAX_GRANTS + 1) {
        let f = home.join(format!("Documents/f{i}.iso"));
        write_sized(&f, 1);
        paths.push(s(&f));
    }
    let (_p, mut audit) = audit_at(&home);

    let err = dispose_selected_with_sink(&home, &paths, None, true, &sink(&home), &mut audit)
        .unwrap_err();

    assert!(err.contains("exceeds the limit"), "{err}");
    assert!(home.join("Documents/f0.iso").exists(), "nothing touched");
}

#[test]
fn a_selection_that_grew_since_the_preview_is_refused() {
    let (_g, home) = fixture_home();
    let f = home.join("Documents/huge.iso");
    write_sized(&f, 8 * 1024 * 1024);
    let (_p, mut audit) = audit_at(&home);

    // The user confirmed a far smaller figure than what is on disk now.
    let expected = Expected {
        count: 1,
        bytes: 1024,
    };
    let err = dispose_selected_with_sink(
        &home,
        &[s(&f)],
        Some(expected),
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("grew since the preview"), "{err}");
    assert!(f.exists());
}

#[test]
fn the_selection_tolerance_is_tighter_than_the_cache_one() {
    // A regression guard on a real trap. The cache flow allows 64 MiB of drift
    // because caches churn while a sheet is on screen. A selection does not
    // churn — it is a fixed list — and Large & Old only shows files of 100 MiB
    // and up, so reusing that allowance would be wide enough for a materially
    // different file to pass as "the same one".
    let (_g, home) = fixture_home();
    let f = home.join("Documents/grew.iso");
    write_sized(&f, 16 * 1024 * 1024); // +16 MiB: inside 64 MiB, outside 1 MiB
    let (_p, mut audit) = audit_at(&home);

    let expected = Expected {
        count: 1,
        bytes: 1024,
    };
    let err = dispose_selected_with_sink(
        &home,
        &[s(&f)],
        Some(expected),
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("grew since the preview"), "{err}");
    assert!(f.exists());
}

#[test]
fn a_file_that_appended_slightly_is_still_allowed() {
    // The tolerance is not zero: an active log can legitimately append in the
    // seconds a confirmation sheet is open, and refusing that would make the
    // feature fire spuriously.
    let (_g, home) = fixture_home();
    let f = home.join("Documents/active.log");
    write_sized(&f, 4096);
    let (_p, mut audit) = audit_at(&home);

    let expected = Expected {
        count: 1,
        bytes: 2048,
    };
    let summary = dispose_selected_with_sink(
        &home,
        &[s(&f)],
        Some(expected),
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
}

#[test]
fn a_selection_whose_count_does_not_match_is_refused() {
    // Unlike a rescan, the set here is exactly the paths the caller sent. Any
    // count difference means the UI and the backend disagree about what was
    // chosen — the precise state in which nothing should happen.
    let (_g, home) = fixture_home();
    let a = home.join("Documents/a.iso");
    let b = home.join("Documents/b.iso");
    write_sized(&a, 4096);
    write_sized(&b, 4096);
    let (_p, mut audit) = audit_at(&home);

    let expected = Expected {
        count: 1, // the sheet said one; two arrived
        bytes: 16384,
    };
    let err = dispose_selected_with_sink(
        &home,
        &[s(&a), s(&b)],
        Some(expected),
        false,
        &sink(&home),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("not the one you confirmed"), "{err}");
    assert!(a.exists() && b.exists());
}

#[test]
fn a_symlink_swapped_in_after_selection_cannot_redirect_the_disposal() {
    // The grant is matched against the freshly re-resolved path inside the
    // executor, so a link pointing somewhere else no longer matches what was
    // authorized. End-to-end version of the executor-level test.
    let (_g, home) = fixture_home();
    let picked = home.join("Documents/picked.iso");
    let other = home.join("Documents/other.iso");
    write_sized(&picked, 4096);
    write_sized(&other, 4096);

    let selection = vec![s(&picked)];
    fs::remove_file(&picked).unwrap();
    std::os::unix::fs::symlink(&other, &picked).unwrap();

    let (_p, mut audit) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &selection, None, false, &sink(&home), &mut audit)
        .unwrap_err();

    // An earlier version of this test asserted the opposite — that the redirect
    // succeeded — and rationalized it as "nothing outside the selection's
    // *resolved* target is touched". That reasoning is circular: the resolved
    // target is exactly what an attacker controls. The test carried the name of
    // a safety property while pinning its absence.
    assert!(err.contains("not the file that was listed"), "{err}");
    assert!(
        other.exists(),
        "a file the user never selected must survive"
    );
    assert!(
        !home.join("test-trash/other.iso").exists(),
        "and must not have been trashed under any name"
    );
}

#[test]
fn a_path_outside_the_discovery_scope_is_refused() {
    // Disposal must never be wider than discovery. `guard` only enforces the
    // denylist, which every ordinary file on the volume passes — so without a
    // scope check this entry point is a general-purpose file remover that
    // happens to be reachable from the Large & Old screen.
    let (_g, home) = fixture_home();
    let secret = home.join("Projects/keys/private-material.txt");
    write_sized(&secret, 4096);
    let (_p, mut audit) = audit_at(&home);

    let err =
        dispose_selected_with_sink(&home, &[s(&secret)], None, false, &sink(&home), &mut audit)
            .unwrap_err();

    assert!(err.contains("outside the discovery scope"), "{err}");
    assert!(
        secret.exists(),
        "a path no walk could have offered survives"
    );
}

#[test]
fn a_non_canonical_spelling_of_a_valid_file_is_refused() {
    // The same guarantee from the other direction: routing a real, in-scope
    // file through a symlinked intermediate directory produces a spelling the
    // walk never emitted, so it cannot be what the user was shown.
    let (_g, home) = fixture_home();
    let real = home.join("Documents/nested/big.iso");
    write_sized(&real, 4096);
    std::os::unix::fs::symlink(home.join("Documents/nested"), home.join("Documents/alias"))
        .unwrap();

    let aliased = home.join("Documents/alias/big.iso");
    let (_p, mut audit) = audit_at(&home);

    let err =
        dispose_selected_with_sink(&home, &[s(&aliased)], None, false, &sink(&home), &mut audit)
            .unwrap_err();

    assert!(err.contains("not the file that was listed"), "{err}");
    assert!(real.exists());
}

#[test]
fn every_refusal_leaves_a_record_in_the_audit_log() {
    // A frontend sending a protected or out-of-scope path is precisely the
    // signal worth having afterwards, and it was the one thing the log never
    // mentioned: all of these returned before the executor was reached.
    let (_g, home) = fixture_home();
    let ok = home.join("Documents/big.iso");
    write_sized(&ok, 4096);
    let outside = home.join("Projects/thing.bin");
    write_sized(&outside, 4096);

    for (selection, expect) in [
        (vec![], "nothing was selected"),
        (vec![s(&outside)], "outside the discovery scope"),
        (vec![s(&home.join("Library/Mail/x.db"))], "no longer valid"),
    ] {
        let (audit_path, mut audit) = audit_at(&home);
        fs::write(&audit_path, "").unwrap();
        let err =
            dispose_selected_with_sink(&home, &selection, None, false, &sink(&home), &mut audit)
                .unwrap_err();
        assert!(err.contains(expect), "{err}");

        let log = fs::read_to_string(&audit_path).unwrap();
        assert!(
            log.contains("\"disposition\":\"refused\""),
            "refusal not recorded for {selection:?}, log was:\n{log}"
        );
    }
}

#[test]
fn a_refused_request_leaves_the_disk_untouched() {
    // Belt and braces across every refusal path at once.
    let (_g, home) = fixture_home();
    let a = home.join("Documents/a.iso");
    let b = home.join("Documents/b.iso");
    write_sized(&a, 4096);
    write_sized(&b, 4096);
    let before = fs::read_dir(home.join("Documents")).unwrap().count();

    for selection in [
        vec![],
        vec![s(&home)],                                   // the home root
        vec![s(&a), s(&home.join("Documents/nope.iso"))], // one missing
        vec![s(&a), s(&home.join("Library/Caches"))],     // a directory
    ] {
        let (_p, mut audit) = audit_at(&home);
        let result =
            dispose_selected_with_sink(&home, &selection, None, false, &sink(&home), &mut audit);
        assert!(result.is_err(), "expected refusal for {selection:?}");
    }

    assert_eq!(
        fs::read_dir(home.join("Documents")).unwrap().count(),
        before,
        "no refusal path may act on anything"
    );
    assert!(a.exists() && b.exists());
}
