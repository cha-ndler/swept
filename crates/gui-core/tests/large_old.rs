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

use swept_core::audit::AuditLog;
use swept_core::executor::{DirSink, MAX_GRANTS};
use swept_gui_core::{dispose_selected_with_sink, large_and_old, Expected};

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
    for i in 0..(swept_core::plan::MASS_DELETE_COUNT + 1) {
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
fn a_row_from_a_symlinked_discovery_root_is_still_actionable() {
    // The scope check must be made against the roots as the WALK resolves them,
    // not as `discovery_roots` spells them. ~/Documents is a symlink on any Mac
    // keeping Documents in iCloud Drive: the walk canonicalizes the root and so
    // emits /real/path/..., while the literal `~/Documents` prefix would never
    // match it — refusing every row the feature just offered.
    let (_g, home) = fixture_home();
    let real = home.join("CloudStorage/Docs");
    write_sized(&real.join("big.iso"), 4096);

    let linked = home.join("Documents");
    fs::remove_dir_all(&linked).unwrap();
    std::os::unix::fs::symlink(&real, &linked).unwrap();

    // Take the path from the walk itself, exactly as the UI would.
    let dto = large_and_old(&home, 1024, None);
    let offered: Vec<String> = dto.items.iter().map(|i| i.path.clone()).collect();
    assert_eq!(offered.len(), 1, "the walk must find it: {offered:?}");

    let (_p, mut audit) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &offered, None, false, &sink(&home), &mut audit).unwrap();

    assert_eq!(
        summary.executed, 1,
        "a row the walk offered must be actionable"
    );
}

#[test]
fn the_ceiling_is_the_discovery_scope_not_one_walks_results() {
    // Stating the invariant that is actually enforced, so nobody mistakes it
    // for a stronger one. A path in a discovery root the user's *current* view
    // never covered is still in scope — the confinement is to
    // `discovery_roots`, not to "what this walk offered".
    //
    // That distinction becomes live the first time Large & Old grows a root
    // filter ("search Downloads only"), which must then thread its resolved
    // roots through rather than relying on these two agreeing.
    let (_g, home) = fixture_home();
    fs::create_dir_all(home.join("Movies")).unwrap();
    let unseen = home.join("Movies/never-listed.iso");
    write_sized(&unseen, 4096);

    // A walk that only looked at Downloads offers nothing.
    let dto = large_and_old(&home, 1024, None);
    assert!(
        dto.items.iter().any(|i| i.path.contains("never-listed")),
        "with default roots the file IS offered — this test is about scope, \
         not about hiding it"
    );

    let (_p, mut audit) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &[s(&unseen)], None, false, &sink(&home), &mut audit)
            .unwrap();
    assert_eq!(summary.executed, 1, "inside the ceiling, so permitted");
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

// --- the consequence this verb has no way to ask about ----------------------
//
// `~/Library/Application Support` is a discovery root, and a Chromium profile
// lives inside it. So a browser's cookie jar is a regular file, its own
// canonical spelling, inside the discovery scope, not a directory — it passes
// every check this function had, and it was disposed of with no acknowledgement
// of any kind.
//
// M5 built a whole second consent axis for exactly this: `SignsYouOut`,
// `ErasesHistory`, `LosesOpenTabs`, `LosesSiteData`, each refused by default.
// `dispose_privacy_with_sink` will not touch a cookie jar without one. This
// verb has no such axis and cannot grow one — the Large & Old screen shows
// sizes and dates, not consequences — so the only correct answer is to refuse.

fn chromium_profile(home: &Path, name: &str) -> PathBuf {
    let p = home
        .join("Library/Application Support/Google/Chrome")
        .join(name);
    write_sized(&p.join("Preferences"), 10);
    p
}

fn firefox_profile(home: &Path, name: &str) -> PathBuf {
    let p = home
        .join("Library/Application Support/Firefox/Profiles")
        .join(name);
    write_sized(&p.join("prefs.js"), 10);
    p
}

/// A cookie jar big enough for Large & Old to list is still a cookie jar.
#[test]
fn a_path_a_privacy_row_owns_is_refused_by_the_verb_with_no_acknowledgement() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    // It is listed. The read-only half is honest and stays honest — hiding it
    // would be a different lie, and the file really is large.
    let report = large_and_old(&home, 1024, None);
    assert!(
        report.items.iter().any(|i| i.path == s(&cookies)),
        "the walk still shows it: {:?}",
        report.items
    );

    let (_p, mut log) = audit_at(&home);
    let err =
        dispose_selected_with_sink(&home, &[s(&cookies)], None, false, &sink(&home), &mut log)
            .unwrap_err();

    assert!(
        err.contains("signs you out"),
        "the refusal must name the consequence, not just say no: {err}"
    );
    assert!(cookies.exists(), "and the file is still there");
}

/// Not only the row's own path: a file *inside* a directory row carries the
/// row's consequence too. Disposing one leveldb file out of `Local Storage` is
/// still losing site data.
#[test]
fn a_file_inside_a_directory_row_carries_that_rows_consequence() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let leaf = profile.join("Local Storage/leveldb/000003.log");
    write_sized(&leaf, 4096);

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &[s(&leaf)], None, false, &sink(&home), &mut log)
        .unwrap_err();

    assert!(err.contains("site data"), "{err}");
    assert!(leaf.exists());
}

/// The paired negative, without which the fix is indistinguishable from
/// breaking the feature: a regenerable row has no consequence to acknowledge,
/// so a big file inside a browser cache is disposed of exactly as before.
#[test]
fn a_file_inside_a_regenerable_row_is_still_disposable() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let blob = profile.join("GPUCache/data_1");
    write_sized(&blob, 4096);

    let (_p, mut log) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &[s(&blob)], None, false, &sink(&home), &mut log)
            .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!blob.exists());
}

/// And the wider negative: `~/Library/Application Support` stays in the
/// discovery scope for everything that is not a browser's private data.
#[test]
fn an_ordinary_large_file_in_application_support_is_still_disposable() {
    let (_g, home) = fixture_home();
    let blob = home.join("Library/Application Support/Some App/render.cache");
    write_sized(&blob, 4096);

    let (_p, mut log) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &[s(&blob)], None, false, &sink(&home), &mut log)
            .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!blob.exists());
}

/// One bad path refuses the whole request, as everywhere else in this file.
#[test]
fn a_consequence_carrying_path_refuses_the_whole_selection() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);
    let ordinary = home.join("Downloads/big.iso");
    write_sized(&ordinary, 4096);

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(
        &home,
        &[s(&ordinary), s(&cookies)],
        None,
        false,
        &sink(&home),
        &mut log,
    )
    .unwrap_err();

    assert!(err.contains("signs you out"), "{err}");
    assert!(
        ordinary.exists(),
        "a partial run is never what was confirmed"
    );
    assert!(cookies.exists());
}

/// The assertion the other three disposal verbs make and this one did not.
///
/// A non-canonical home silently disables the home-relative denylist rules —
/// Keychains, Mail, the home root itself — for the whole run, because
/// `is_protected` compares against `home.join(...)`. Not reachable through
/// `dispose_selected`, which canonicalizes; reachable through this function,
/// which is public.
#[test]
fn dispose_selected_refuses_a_non_canonical_home() {
    let (_g, home) = fixture_home();
    let alias = home.join("Documents/..");
    let blob = home.join("Downloads/big.iso");
    write_sized(&blob, 4096);

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&alias, &[s(&blob)], None, false, &sink(&home), &mut log)
        .unwrap_err();

    assert!(err.contains("home"), "{err}");
    assert!(blob.exists());
}

// --- what the first version of this fix got wrong ---------------------------
//
// The first attempt asked "does a privacy row name this path?", which is an
// *exclusion* list — precisely the posture `privacy.rs`'s own module doc argues
// against: "An exclusion list would fail open the next time a vendor adds a
// file." It failed open four different ways, each reproduced by the reviewer.
//
// The rule now is inclusion-by-permission: inside a browser's own root, only a
// `Regenerable` row grants passage. Refusing needs far less knowledge than
// offering, so "we could not corroborate this" stops meaning "go ahead".

/// Restores permissions on drop, so a panicking assertion still leaves a
/// tempdir that can be cleaned up.
struct Restore(PathBuf, fs::Permissions);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, self.1.clone());
    }
}

#[must_use]
fn with_mode(path: &Path, mode: u32) -> Restore {
    use std::os::unix::fs::PermissionsExt;
    let original = fs::metadata(path).unwrap().permissions();
    let mut next = original.clone();
    next.set_mode(mode);
    fs::set_permissions(path, next).unwrap();
    Restore(path.to_path_buf(), original)
}

fn refusal(home: &Path, path: &Path) -> String {
    let (_p, mut log) = audit_at(home);
    dispose_selected_with_sink(home, &[s(path)], None, false, &sink(home), &mut log)
        .expect_err("expected a refusal")
}

/// **F1.** `privacy::scan` is infallible and documents that it can only
/// under-report — sound for the Privacy screen, where under-reporting means
/// fewer *offers*. Used as a prohibition oracle it means fewer *vetoes*, so the
/// user whose browser data is behind a permission wall would get strictly less
/// protection than the user whose is not. Exactly backwards.
#[test]
fn a_browser_root_that_could_not_be_read_still_refuses() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    // Traversable but not enumerable — what a TCC denial and an ordinary mode
    // bit both look like to `read_dir`. The file itself is still stat-able, so
    // every other check in the verb passes.
    let root = home.join("Library/Application Support/Google/Chrome");
    let _shut = with_mode(&root, 0o111);

    let err = refusal(&home, &cookies);
    assert!(err.contains("could not"), "{err}");
    assert!(cookies.exists());
}

/// **F2.** The inclusion lists in `privacy.rs` enumerate what can be *offered*,
/// not everything that matters. `Login Data` and `key4.db` are named by no row,
/// and losing them is worse than losing the cookie jar this fix set out to
/// protect.
#[test]
fn a_file_no_privacy_row_names_is_still_refused_inside_a_profile() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    for name in ["Login Data", "Web Data", "Bookmarks", "Favicons"] {
        let p = profile.join(name);
        write_sized(&p, 4096);
        let err = refusal(&home, &p);
        assert!(err.contains("Chrome"), "{name}: {err}");
        assert!(p.exists(), "{name} was disposed of");
    }
}

/// **F2, the sharpest case.** `places.sqlite` holds bookmarks as well as
/// history, which is exactly why `privacy.rs` deliberately has no Firefox
/// history entry. So the one Firefox file the module withholds *because it is
/// too consequential* was the one the first version of this fix left
/// disposable — while the commit message cited it as an example of the hole
/// being closed.
#[test]
fn firefox_places_is_refused_although_no_row_could_ever_name_it() {
    let (_g, home) = fixture_home();
    let profile = firefox_profile(&home, "abc.default");
    let places = profile.join("places.sqlite");
    write_sized(&places, 4096);

    let err = refusal(&home, &places);
    assert!(err.contains("Firefox"), "{err}");
    assert!(places.exists());
}

/// **F3.** A profile directory whose `Preferences` is missing produces no rows
/// at all. No permission weirdness needed: the walk lists it normally, the user
/// selects it, and the first version disposed of it.
#[test]
fn a_profile_that_could_not_be_corroborated_still_refuses() {
    let (_g, home) = fixture_home();
    let orphan = home
        .join("Library/Application Support/Google/Chrome")
        .join("Profile 7");
    let cookies = orphan.join("Cookies");
    write_sized(&cookies, 4096);

    let err = refusal(&home, &cookies);
    assert!(err.contains("Chrome"), "{err}");
    assert!(cookies.exists());
}

/// **F7.** `System Profile` and `Guest Profile` are skipped by the scan on
/// purpose, so they produced no rows either.
#[test]
fn a_chromium_non_profile_directory_is_refused() {
    let (_g, home) = fixture_home();
    let p = home.join("Library/Application Support/Google/Chrome/System Profile/Network/Cookies");
    write_sized(&p, 4096);

    let err = refusal(&home, &p);
    assert!(err.contains("Chrome"), "{err}");
    assert!(p.exists());
}

/// **F4.** `Class::SiteStorage` sets `offerable = false` unconditionally, so
/// Privacy will never offer it either. Sending someone there would be a dead
/// end dressed as a route — worse than saying plainly that no screen here will
/// do it.
#[test]
fn a_row_privacy_will_never_offer_does_not_point_at_privacy() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let blob = profile.join("Service Worker/CacheStorage/abc/def_0");
    write_sized(&blob, 4096);

    let err = refusal(&home, &blob);
    assert!(
        !err.contains("use Privacy"),
        "pointed at a screen that will not ask either: {err}"
    );
    assert!(err.contains("site data"), "{err}");
    assert!(blob.exists());
}

/// The consequence a screen *can* route to still routes there.
#[test]
fn a_row_privacy_will_offer_names_that_screen() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    let err = refusal(&home, &cookies);
    assert!(err.contains("signs you out"), "{err}");
    assert!(err.contains("Privacy"), "{err}");
    assert!(cookies.exists());
}

/// **F6.** The predicate compares byte-exactly, and its case-safety is borrowed
/// from the identity check two branches above — `fs::canonicalize` normalizes
/// case on APFS, so a differently-cased spelling is refused before the
/// predicate ever runs. Borrowed safety is worth pinning, or a later edit to
/// either check silently un-borrows it.
#[test]
fn a_differently_cased_spelling_never_reaches_the_predicate() {
    let (_g, home) = fixture_home();
    let profile = chromium_profile(&home, "Default");
    write_sized(&profile.join("Cookies"), 4096);

    let err = refusal(&home, &profile.join("cookies"));
    assert!(
        err.contains("not the file that was listed"),
        "the identity check must be what refuses this: {err}"
    );
}

/// **F9.** The boundary is a prefix test on the *canonical* path, and the
/// browser root is the one place a user plausibly puts a symlink — a profile
/// relocated off the internal SSD. Canonicalization is a TOCTOU defence
/// everywhere else in this verb; here it was what defeated the protection, and
/// the blast radius was not one file but everything the boundary covers.
#[test]
fn a_symlinked_browser_root_is_still_that_browsers_data() {
    let (_g, home) = fixture_home();
    let real = home.join("Documents/ChromeData");
    let profile = real.join("Default");
    write_sized(&profile.join("Preferences"), 10);
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    let link = home.join("Library/Application Support/Google/Chrome");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    // The walk lists the canonical spelling, which is what the user selects.
    let err = refusal(&home, &cookies);
    assert!(err.contains("Chrome"), "{err}");
    assert!(cookies.exists());
}

/// **F10.** `BrowserSpec::root` means "where the profiles are" — it was chosen
/// for *offering*, and Arc's is deliberately one level down. Reusing it as the
/// *refusal* boundary drew that boundary too narrowly and left Arc's own data
/// files outside it. `StorableSidebar.json` holds every space, pinned tab and
/// folder; `StorableArchiveItems.json` accumulates archived tabs and is the one
/// that actually grows into this screen's range. Neither is a cache.
///
/// Naming a directory is not guessing a layout, so the refusal boundary is
/// allowed to be coarser than the scan root.
#[test]
fn a_browsers_own_data_above_its_profile_root_is_refused() {
    let (_g, home) = fixture_home();
    let arc = home.join("Library/Application Support/Arc");
    for name in ["StorableSidebar.json", "StorableArchiveItems.json"] {
        let p = arc.join(name);
        write_sized(&p, 4096);
        let err = refusal(&home, &p);
        assert!(err.contains("Arc"), "{name}: {err}");
        assert!(p.exists(), "{name} was disposed of");
    }
}

/// The coarser boundary must not become a vendor-wide one. `Google` holds
/// `GoogleSoftwareUpdate`, which is a genuine disposable and nobody's private
/// data.
#[test]
fn the_refusal_boundary_does_not_widen_to_the_vendor_directory() {
    let (_g, home) = fixture_home();
    let p = home.join("Library/Application Support/Google/GoogleSoftwareUpdate/blob.bin");
    write_sized(&p, 4096);

    let (_a, mut log) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &[s(&p)], None, false, &sink(&home), &mut log).unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!p.exists());
}

/// A canary, because the property is currently an accident of the table rather
/// than something anyone stated.
///
/// `consequence_of` searches every row of the scan, not only the rows belonging
/// to the browser whose root matched. That is safe *only* while no browser's
/// boundary contains another's: if one did, a `Regenerable` row in the inner
/// browser could grant passage to a consequence-carrying file in the outer one.
/// Adding a root spelled `.../Google` would create exactly that nesting.
#[test]
fn no_browser_boundary_contains_another() {
    use swept_core::privacy::BROWSERS;

    let home = Path::new("/Users/fixture");
    let bounds: Vec<_> = BROWSERS
        .iter()
        .map(|b| (b.id, home.join(b.data_root.unwrap_or(b.root))))
        .collect();

    for (id, outer) in &bounds {
        for (other, inner) in &bounds {
            if id == other {
                continue;
            }
            assert!(
                !inner.starts_with(outer),
                "{other}'s boundary is inside {id}'s, so one browser's cache row \
                 could authorize the other's private data"
            );
        }
    }
}

// --- the two shape rules the walk has and the verb did not ------------------

/// **A file with a second name frees nothing when this one goes.**
///
/// `largeold::find` refuses to *offer* a hard-linked file and says why —
/// removing one name reclaims no space — and until now nothing mirrored that
/// where it mattered. The verb would move the name to the Trash and report the
/// full size as freed, while the data sat untouched at the other link. That is
/// the same misreport that keeps the Trash category out of Smart Scan.
#[test]
fn a_hard_linked_file_is_refused_because_removing_one_name_frees_nothing() {
    let (_g, home) = fixture_home();
    let first = home.join("Downloads/big.iso");
    let second = home.join("Documents/keeper.iso");
    write_sized(&first, 4096);
    fs::create_dir_all(second.parent().unwrap()).unwrap();
    fs::hard_link(&first, &second).unwrap();

    // The walk never offered it.
    let report = large_and_old(&home, 1024, None);
    assert!(report.items.is_empty(), "{:?}", report.items);
    assert_eq!(report.skipped_hardlinked, 2);

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &[s(&first)], None, false, &sink(&home), &mut log)
        .unwrap_err();

    assert!(err.contains("more than one name"), "{err}");
    assert!(first.exists());
    assert!(second.exists());
}

/// The paired negative: one name, and it goes.
#[test]
fn a_file_with_one_name_is_still_disposable() {
    let (_g, home) = fixture_home();
    let only = home.join("Downloads/big.iso");
    write_sized(&only, 4096);

    let (_p, mut log) = audit_at(&home);
    let summary =
        dispose_selected_with_sink(&home, &[s(&only)], None, false, &sink(&home), &mut log)
            .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!only.exists());
}

/// `is_dir()` alone let a socket, FIFO or device node take the file branch — and
/// a length of zero for something that is not a file is not a fact about how
/// much space it uses.
#[test]
fn a_path_that_is_not_an_ordinary_file_is_refused() {
    let (_g, home) = fixture_home();
    let sock = home.join("Downloads/live.sock");
    fs::create_dir_all(sock.parent().unwrap()).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &[s(&sock)], None, false, &sink(&home), &mut log)
        .unwrap_err();

    assert!(err.contains("not an ordinary file"), "{err}");
    assert!(sock.exists());
}
