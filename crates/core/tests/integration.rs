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
