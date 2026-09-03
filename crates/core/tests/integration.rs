//! End-to-end tests for the scan → plan → execute pipeline.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.
//! No test ever names a real user path.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{execute, Consent, DirSink};
use macclean_core::plan::Disposal;
use macclean_core::scanner::{scan, ScanConfig};

/// Build a fake `$HOME` with a Caches tree and return its canonical path.
fn fake_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/Caches/app")).unwrap();
    fs::create_dir_all(home.join("Library/Logs")).unwrap();
    (dir, home)
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn audit_at(home: &Path) -> (PathBuf, AuditLog) {
    let p = home.join("audit.jsonl");
    let log = AuditLog::open(&p).unwrap();
    (p, log)
}

#[test]
fn scan_report_serializes_a_stable_shape() {
    use macclean_core::report::ScanReport;

    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/a.bin"), b"12345"); // 5 bytes, cache
    write(&home.join("Library/Logs/x.log"), b"6789"); // 4 bytes, log

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    let report = ScanReport::from_plan(&plan);
    let json = serde_json::to_string(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(v["total_count"], 2);
    assert_eq!(v["total_bytes"], 9);
    assert_eq!(v["requires_confirmation"], false);
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    // The completeness pair travels on the wire, not just in the struct.
    // Without this the field could be renamed or `#[serde(skip)]`ed and every
    // other test would stay green while the GUI silently lost it.
    assert_eq!(v["skipped_unreadable"], 0);
    assert_eq!(v["partial"], false);

    let cats = v["by_category"].as_array().unwrap();
    let cache = cats
        .iter()
        .find(|c| c["category"] == "user-caches")
        .expect("user-caches category present");
    assert_eq!(cache["count"], 1);
    assert_eq!(cache["bytes"], 5);
    // Categories carry human-facing metadata for the GUI.
    assert!(!cache["name"].as_str().unwrap().is_empty());
    assert!(!cache["description"].as_str().unwrap().is_empty());
    assert!(cats
        .iter()
        .any(|c| c["category"] == "user-logs" && c["count"] == 1 && c["bytes"] == 4));

    // Each item carries an absolute path and a disposal label.
    let item = &v["items"].as_array().unwrap()[0];
    assert!(item["path"].as_str().unwrap().starts_with('/'));
    assert_eq!(item["disposal"], "trash");
}

#[test]
fn homebrew_files_classify_as_their_specific_category() {
    let (_g, home) = fake_home();
    // A Homebrew download (deep) and a generic app cache (shallow) both live
    // under Library/Caches — the deeper, more specific category must win.
    write(
        &home.join("Library/Caches/Homebrew/downloads/pkg.tar"),
        b"abc",
    );
    write(&home.join("Library/Caches/app/generic.bin"), b"de");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    let report = macclean_core::report::ScanReport::from_plan(&plan);
    let ids: Vec<&str> = report
        .by_category
        .iter()
        .map(|c| c.category.as_str())
        .collect();

    assert!(ids.contains(&"homebrew-downloads"), "got {ids:?}");
    assert!(ids.contains(&"user-caches"), "got {ids:?}");
}

#[test]
fn age_filter_excludes_recently_modified_files() {
    use filetime::{set_file_mtime, FileTime};
    use std::time::{Duration, SystemTime};

    let (_g, home) = fake_home();
    let old = home.join("Library/Caches/app/old.bin");
    let fresh = home.join("Library/Caches/app/fresh.bin");
    write(&old, b"old");
    write(&fresh, b"new");

    // Backdate `old` to 40 days ago; `fresh` keeps its (now) mtime.
    let forty_days_ago = SystemTime::now() - Duration::from_secs(40 * 86_400);
    set_file_mtime(&old, FileTime::from_system_time(forty_days_ago)).unwrap();

    let cfg =
        ScanConfig::with_default_roots(home.clone()).older_than(Duration::from_secs(30 * 86_400));
    let plan = scan(&cfg);

    let names: Vec<String> = plan
        .actions
        .iter()
        .filter_map(|a| {
            a.path
                .as_path()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect();
    assert!(
        names.contains(&"old.bin".to_string()),
        "old file should be planned"
    );
    assert!(
        !names.contains(&"fresh.bin".to_string()),
        "recently-modified file must be excluded by the age filter"
    );
}

#[test]
fn min_size_filter_excludes_small_files() {
    let (_g, home) = fake_home();
    let big = home.join("Library/Caches/app/big.bin");
    let small = home.join("Library/Caches/app/small.bin");
    write(&big, &[0u8; 2048]); // 2 KiB
    write(&small, &[0u8; 100]); // 100 B

    let cfg = ScanConfig::with_default_roots(home.clone()).min_size(1024);
    let plan = scan(&cfg);

    let names: Vec<String> = plan
        .actions
        .iter()
        .filter_map(|a| {
            a.path
                .as_path()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect();
    assert!(names.contains(&"big.bin".to_string()), "got {names:?}");
    assert!(
        !names.contains(&"small.bin".to_string()),
        "files below --min-size must be excluded; got {names:?}"
    );
}

#[test]
fn no_age_filter_includes_everything() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/a.bin"), b"x");
    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    assert_eq!(
        plan.count(),
        1,
        "without an age filter, all candidates are planned"
    );
}

#[test]
fn scan_finds_cache_files_and_skips_protected() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/a.bin"), b"12345");
    write(&home.join("Library/Caches/app/b.bin"), b"6789");
    // A file outside the allowlist must not appear in the plan.
    write(&home.join("Documents/keep.txt"), b"precious");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    assert_eq!(plan.count(), 2);
    assert_eq!(plan.total_bytes(), 9);
    assert!(plan.actions.iter().all(|a| a.disposal == Disposal::Trash));
    assert!(plan
        .actions
        .iter()
        .all(|a| a.path.as_path().starts_with(home.join("Library/Caches"))));
}

#[test]
fn dry_run_is_default_and_mutates_nothing() {
    let (_g, home) = fake_home();
    let f = home.join("Library/Caches/app/a.bin");
    write(&f, b"data");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    let (audit_path, mut audit) = audit_at(&home);

    // Consent::default() == dry run.
    let report = execute(
        &plan,
        Consent::default(),
        &home,
        &DirSink {
            trash_dir: home.join(".trash"),
        },
        &mut audit,
    )
    .unwrap();

    assert!(report.dry_run);
    assert_eq!(report.executed, 0);
    assert_eq!(report.planned, 1);
    assert!(f.exists(), "dry run must not delete the file");

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("\"phase\":\"planned\""));
}

#[test]
fn execute_with_consent_trashes_files() {
    let (_g, home) = fake_home();
    let f = home.join("Library/Caches/app/a.bin");
    write(&f, b"data");
    let trash_dir = home.join("test-trash");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    let (audit_path, mut audit) = audit_at(&home);

    let consent = Consent {
        execute: true,
        ..Default::default()
    };
    let report = execute(
        &plan,
        consent,
        &home,
        &DirSink {
            trash_dir: trash_dir.clone(),
        },
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 1);
    assert!(!f.exists(), "file should have left its original location");
    assert!(
        trash_dir.join("a.bin").exists(),
        "file should be in the test trash"
    );

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("\"phase\":\"executed\""));
    assert!(log.contains("\"disposition\":\"trash\""));
}

#[test]
fn mass_delete_is_refused_without_confirmation() {
    let (_g, home) = fake_home();
    for i in 0..(macclean_core::plan::MASS_DELETE_COUNT + 1) {
        write(&home.join(format!("Library/Caches/app/f{i}.bin")), b"x");
    }
    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    assert!(plan.requires_confirmation());

    let (_p, mut audit) = audit_at(&home);
    let consent = Consent {
        execute: true,
        ..Default::default()
    };
    let err = execute(
        &plan,
        consent,
        &home,
        &DirSink {
            trash_dir: home.join("t"),
        },
        &mut audit,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        macclean_core::executor::ExecError::MassDeleteUnconfirmed { .. }
    ));
    // Nothing was deleted.
    assert!(home.join("Library/Caches/app/f0.bin").exists());
}

#[test]
fn scanner_never_plans_a_symlink_escaping_the_allowlist() {
    // A symlink inside Caches pointing at a precious file outside the allowlist
    // must not appear in the plan: WalkDir (follow_links=false) treats it as a
    // non-file, and even if it didn't, `guard` canonicalizes to the target and
    // the allowlist re-check drops it.
    let (_g, home) = fake_home();
    let precious = home.join("Documents/important.txt");
    write(&precious, b"do not touch");
    let link = home.join("Library/Caches/app/escape");
    std::os::unix::fs::symlink(&precious, &link).unwrap();

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    assert!(
        plan.actions
            .iter()
            .all(|a| !a.path.as_path().ends_with("important.txt")),
        "a symlink target outside the allowlist must never be planned"
    );
    assert!(precious.exists());
}

#[test]
fn execution_refuses_path_that_became_protected() {
    // Simulate TOCTOU: a path is in the plan, but by execution time it resolves
    // (via a swapped symlink) to the protected home root.
    let (_g, home) = fake_home();
    let real = home.join("Library/Caches/app/a.bin");
    write(&real, b"data");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    assert_eq!(plan.count(), 1);

    // Replace the real file with a symlink to the protected home root.
    fs::remove_file(&real).unwrap();
    std::os::unix::fs::symlink(&home, &real).unwrap();

    let (audit_path, mut audit) = audit_at(&home);
    let consent = Consent {
        execute: true,
        ..Default::default()
    };
    let report = execute(
        &plan,
        consent,
        &home,
        &DirSink {
            trash_dir: home.join("t"),
        },
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(home.exists(), "home root must be untouched");

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("\"disposition\":\"refused\""));
}

#[test]
fn scan_with_progress_reports_monotonic_counts_and_matches_plain_scan() {
    use macclean_core::scanner::{scan_with_progress, Progress};

    let (_g, home) = fake_home();
    for i in 0..40 {
        write(&home.join(format!("Library/Caches/app/f{i}.bin")), b"data");
    }
    let cfg = ScanConfig::with_default_roots(home.clone());

    let mut updates: Vec<Progress> = Vec::new();
    let plan = scan_with_progress(&cfg, &mut |p| updates.push(p));

    assert!(
        !updates.is_empty(),
        "progress must be reported at least once"
    );
    for w in updates.windows(2) {
        assert!(
            w[1].examined >= w[0].examined,
            "examined must not go backwards"
        );
        assert!(
            w[1].planned >= w[0].planned,
            "planned must not go backwards"
        );
        assert!(w[1].bytes >= w[0].bytes, "bytes must not go backwards");
    }

    // The final update must describe the plan that was actually returned.
    let last = *updates.last().unwrap();
    assert_eq!(
        last.planned,
        plan.count(),
        "final progress must match the plan"
    );
    assert_eq!(last.bytes, plan.total_bytes());

    // Adding progress reporting must not change what is planned.
    let plain = scan(&cfg);
    assert_eq!(plan.count(), plain.count());
    assert_eq!(plan.total_bytes(), plain.total_bytes());
    assert_eq!(plan.skipped_protected, plain.skipped_protected);
}

// --- what the scan could not see -------------------------------------------
//
// The scan walks with `filter_map(Result::ok)`, so a directory it cannot read
// contributes nothing and says nothing. On a stock Mac that is not a corner
// case: `~/.Trash` is a disposal root *and* TCC-gated, so without Full Disk
// Access the whole of it is missing from a total that presents itself as
// complete. This is the failure the project has already hit twice — "a report
// of five things invites the reader to conclude their Mac is clean" — in the
// oldest module in the tree.

/// Restores `path`'s permissions when dropped, so an assertion that panics
/// inside the scope still leaves a tempdir that can be cleaned up. Without it,
/// a mode-000 directory survives `TempDir::drop`, whose error is discarded.
struct Restore(PathBuf, fs::Permissions);

impl Drop for Restore {
    fn drop(&mut self) {
        // The one place a discarded error is right: the alternative is a
        // second panic while unwinding from the first.
        let _ = fs::set_permissions(&self.0, self.1.clone());
    }
}

/// Make `path` unopenable (mode 000) until the returned guard is dropped.
#[must_use]
fn unreadable(path: &Path) -> Restore {
    use std::os::unix::fs::PermissionsExt;
    let original = fs::metadata(path).unwrap().permissions();
    let mut locked = original.clone();
    locked.set_mode(0o000);
    fs::set_permissions(path, locked).unwrap();
    Restore(path.to_path_buf(), original)
}

/// Make `path` listable but not searchable (mode 444) until the guard drops:
/// `read_dir` returns its entries, and `stat` on any of them fails.
#[must_use]
fn unsearchable(path: &Path) -> Restore {
    use std::os::unix::fs::PermissionsExt;
    let original = fs::metadata(path).unwrap().permissions();
    let mut ro = original.clone();
    ro.set_mode(0o444);
    fs::set_permissions(path, ro).unwrap();
    Restore(path.to_path_buf(), original)
}

/// A directory inside a cleaner root that cannot be opened is *counted*, not
/// dropped. Without this the bytes behind it are simply absent.
#[test]
fn a_directory_the_scan_cannot_read_is_counted() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/seen.bin"), b"12345");
    let locked = home.join("Library/Caches/locked");
    write(&locked.join("unseen.bin"), b"0123456789");

    let cfg = ScanConfig::with_default_roots(home.clone());
    let _shut = unreadable(&locked);
    let plan = scan(&cfg);

    assert_eq!(plan.count(), 1, "only the readable file is planned");
    assert_eq!(plan.total_bytes(), 5);
    assert_eq!(
        plan.skipped_unreadable, 1,
        "the directory it could not open is counted"
    );
    assert_eq!(
        plan.skipped_protected, 0,
        "and it is not conflated with a deliberate refusal"
    );
}

/// `skipped_unreadable` and `skipped_protected` answer different questions.
/// A protected path is one the scan *chose* not to plan and knows all about; an
/// unreadable one is a hole in what it knows. Only the second makes a total a
/// floor, so conflating them would either hide real gaps or cry wolf on every
/// ordinary scan.
#[test]
fn a_path_refused_by_the_allowlist_is_not_reported_as_unreadable() {
    let (_g, home) = fake_home();
    write(&home.join("Documents/keep.txt"), b"private");

    // A root outside the disposal allowlist: every file under it is walked,
    // guarded, and then refused by `is_allowed` — seen in full, planned never.
    let mut cfg = ScanConfig::with_default_roots(home.clone());
    cfg.roots = vec![home.join("Documents")];
    let plan = scan(&cfg);

    assert_eq!(plan.count(), 0);
    assert_eq!(plan.skipped_protected, 1, "refused, and understood");
    assert_eq!(
        plan.skipped_unreadable, 0,
        "nothing here was invisible to the scan"
    );
}

/// A scan that saw everything says so. The flag has to be able to be *false*,
/// or the UI learns to ignore it.
#[test]
fn a_complete_scan_reports_no_gap() {
    use macclean_core::report::ScanReport;

    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/a.bin"), b"12345");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));
    let report = ScanReport::from_plan(&plan);

    assert_eq!(plan.skipped_unreadable, 0);
    assert!(!report.partial, "nothing was missed, so nothing is claimed");
}

/// The gap has to survive the trip to the UI, and it has to arrive as a
/// *statement about the total* rather than a raw counter the frontend may or
/// may not think to render.
#[test]
fn the_report_presents_an_incomplete_total_as_partial() {
    use macclean_core::report::ScanReport;

    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/seen.bin"), b"12345");
    let locked = home.join("Library/Logs/locked");
    write(&locked.join("unseen.log"), b"0123456789");

    let cfg = ScanConfig::with_default_roots(home.clone());
    let _shut = unreadable(&locked);
    let plan = scan(&cfg);
    let report = ScanReport::from_plan(&plan);

    assert_eq!(report.skipped_unreadable, 1);
    assert!(
        report.partial,
        "a total missing a directory is a floor, and must say so"
    );
    assert_eq!(
        report.total_bytes, 5,
        "the figure itself is unchanged — it is the claim about it that changes"
    );
}

/// A cleaner root that is entirely unreadable is the real-world case: on a Mac
/// without Full Disk Access, `~/.Trash` is exactly this. Reporting zero from it
/// while claiming completeness is the worst available answer.
#[test]
fn a_root_that_cannot_be_opened_at_all_is_a_gap_not_a_zero() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash")).unwrap();
    write(&home.join(".Trash/big.bin"), b"0123456789");

    let cfg = ScanConfig::with_default_roots(home.clone());
    let _shut = unreadable(&home.join(".Trash"));
    let plan = scan(&cfg);

    assert_eq!(plan.count(), 0, "nothing in it could be seen");
    assert!(
        plan.skipped_unreadable >= 1,
        "so the root itself is the gap, not an empty Trash"
    );
}

/// `Path::exists()` is `metadata().is_ok()`, so it answers `false` to *any*
/// error — including "you may not look". Routing that into the branch meaning
/// "this root is not there" skips the walk entirely, so the error counter never
/// sees it, and the scan reports a complete nothing over a hole bigger than any
/// single unreadable subtree.
#[test]
fn a_root_that_cannot_be_stat_ed_is_a_gap_not_an_absence() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/a.bin"), b"12345");

    // Non-searchable `Library` — every root beneath it fails to stat, while
    // `~/.Trash` (a sibling) is unaffected and simply does not exist.
    let cfg = ScanConfig::with_default_roots(home.clone());
    let _shut = unreadable(&home.join("Library"));
    let plan = scan(&cfg);

    assert_eq!(plan.count(), 0, "nothing could be reached");
    assert!(
        plan.skipped_unreadable >= 1,
        "a root it could not even stat is a gap, not an empty machine"
    );
}

/// A root that genuinely is not there is *knowledge*, not a gap. `~/.Trash`
/// absent means nothing was missed, and setting the flag for it would fire on
/// every ordinary fixture — which is how a flag stops being read.
#[test]
fn a_root_that_is_really_absent_is_not_a_gap() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/a.bin"), b"12345");
    assert!(!home.join(".Trash").exists());

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));

    assert_eq!(plan.count(), 1);
    assert_eq!(plan.skipped_unreadable, 0);
}

/// `guard` fails for three different reasons and only two of them are
/// decisions. `Unresolvable` wraps an `io::Error`, so an `EACCES` from
/// `canonicalize` is a hole in what the scan knows — filing it under
/// `skipped_protected` is worse than silence, because the UI renders that
/// counter as "N protected items skipped": a decision claimed over a gap.
#[test]
fn a_path_that_cannot_be_resolved_is_a_gap_not_a_refusal() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/seen.bin"), b"12345");
    let dir = home.join("Library/Caches/opaque");
    write(&dir.join("one.bin"), b"0123456789");
    write(&dir.join("two.bin"), b"0123456789");

    // Readable, so `read_dir` lists both names; not searchable, so
    // `canonicalize` on either of them fails.
    let cfg = ScanConfig::with_default_roots(home.clone());
    let _ro = unsearchable(&dir);
    let plan = scan(&cfg);

    assert_eq!(plan.count(), 1, "only the file outside it is planned");
    assert_eq!(
        plan.skipped_protected, 0,
        "nothing here was refused — the scan simply could not resolve it"
    );
    assert!(
        plan.skipped_unreadable >= 2,
        "both unresolvable names are gaps, got {}",
        plan.skipped_unreadable
    );
}

/// A denylist refusal stays a decision. The point of splitting the counters is
/// lost if everything drifts into the gap bucket instead.
#[test]
fn a_denylist_refusal_is_still_counted_as_a_decision() {
    let (_g, home) = fake_home();
    // Keychains is protected, and a root pointed at it walks real files that
    // `guard` refuses by name rather than failing to resolve.
    write(&home.join("Library/Keychains/login.keychain-db"), b"secret");

    let mut cfg = ScanConfig::with_default_roots(home.clone());
    cfg.roots = vec![home.join("Library/Keychains")];
    let plan = scan(&cfg);

    assert_eq!(plan.count(), 0);
    assert_eq!(plan.skipped_protected, 1, "refused by name, and understood");
    assert_eq!(plan.skipped_unreadable, 0);
}
