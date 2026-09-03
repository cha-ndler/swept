//! Directory actions: the second shape a plan can carry, and every bound on it.
//!
//! Before M4 the executor refused every directory target. It still does, for a
//! *file* action — what changes is that a `PlannedDirAction` carrying a
//! `SafeDir` (a tree walked in full by `safety::guard_dir`) can be moved to the
//! Trash as one recoverable unit, by explicit per-path grant only. The argument
//! this file makes, in order: a directory grant works; it works for exactly the
//! tree that was walked, exactly once, only if the tree still passes
//! `guard_dir` at the moment of disposal, only if it has not grown since it was
//! confirmed, never permanently, and always counting every name beneath it
//! against the mass-delete threshold.
//!
//! This is the first time the executor may hand a directory to `Sink::trash`.
//! The tests marked load-bearing are the ones that fail open if their rule is
//! deleted.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{execute, Consent, DirSink, ExecError, MAX_GRANTS};
use macclean_core::plan::{Disposal, Plan, PlannedAction, PlannedDirAction, MASS_DELETE_COUNT};
use macclean_core::scanner::{scan, ScanConfig};
use safety::{guard, guard_dir, DirLimits, SafeDir};

// --- fixtures --------------------------------------------------------------

fn fake_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/Caches")).unwrap();
    fs::create_dir_all(home.join("Library/Mail")).unwrap();
    fs::create_dir_all(home.join("Documents")).unwrap();
    (dir, home)
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// A leftover-shaped tree, three levels deep. Returns its byte total.
fn leftover_tree(dir: &Path) -> u64 {
    write(&dir.join("a.bin"), b"aaaa");
    write(&dir.join("nested/b.bin"), b"bbbbbb");
    write(&dir.join("nested/deeper/c.bin"), b"cc");
    12
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

/// The tree walked in full — the only way to get a `SafeDir`.
fn vouch(dir: &Path, home: &Path) -> SafeDir {
    guard_dir(dir, home, DirLimits::default()).expect("fixture tree must be guardable")
}

/// A one-directory plan. The scanner never produces one of these — pinned
/// below — so this is the shape the Uninstaller's command layer will build.
fn dir_plan(dir: &SafeDir) -> Plan {
    Plan {
        actions: Vec::new(),
        dirs: vec![PlannedDirAction {
            dir: dir.clone(),
            category: "uninstaller-leftovers".to_string(),
        }],
        skipped_protected: 0,
        skipped_unreadable: 0,
    }
}

/// Executing consent for a directory selection. `confirmed_mass_delete` is
/// set because a recursive removal *always* needs it — see
/// `a_one_directory_plan_under_both_thresholds_still_requires_confirmation`,
/// which is the one test that deliberately leaves it unset.
fn consenting_dirs(granted_dirs: Vec<SafeDir>) -> Consent {
    Consent {
        execute: true,
        confirmed_mass_delete: true,
        granted_dirs,
        ..Default::default()
    }
}

/// Every path under `root`, sorted, for before/after comparisons.
fn snapshot(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn go(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            out.push(e.path());
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                go(&e.path(), out);
            }
        }
    }
    go(root, &mut out);
    out.sort();
    out
}

fn log_lines(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

// --- the positive, so the rest cannot pass vacuously ------------------------

#[test]
fn a_granted_directory_is_trashed_as_one_unit() {
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    let bytes = leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (_p, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting_dirs(vec![safe.clone()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 1);
    assert_eq!(report.refused, 0);
    assert_eq!(report.bytes_executed, bytes);
    assert_eq!(report.entries_executed, safe.entries() as u64);
    assert!(!dir.exists());
    // Recoverable *as a unit*: the tree keeps its shape in the trash.
    assert!(home
        .join("test-trash/com.acme.App/nested/deeper/c.bin")
        .exists());
}

// --- authorization: by grant, and by nothing else ---------------------------

#[test]
fn an_ungranted_directory_is_refused_even_inside_the_allowlist() {
    // [LB] The allowlist is a statement about files in cleanable locations,
    // never about trees. Inside `~/Library/Caches` and still refused.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let plan = dir_plan(&vouch(&dir, &home));
    let (audit_path, mut audit) = audit_at(&home);
    let before = snapshot(&home);

    let report = execute(
        &plan,
        consenting_dirs(Vec::new()),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert_eq!(snapshot(&home), before, "nothing moved");
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("explicit per-path grant"));
}

#[test]
fn a_file_grant_does_not_authorize_a_directory() {
    // [LB] The two grant lists are not interchangeable: a `SafePath` naming
    // the directory says nothing about the tree beneath it.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let plan = dir_plan(&vouch(&dir, &home));
    let (_p, mut audit) = audit_at(&home);

    let consent = Consent {
        execute: true,
        confirmed_mass_delete: true,
        granted: vec![guard(&dir, &home).unwrap()],
        ..Default::default()
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(dir.join("nested/deeper/c.bin").exists());
}

#[test]
fn a_directory_grant_does_not_authorize_a_file_action_on_the_same_path() {
    // [LB] The other direction. A *file* action naming a directory is refused
    // by the untouched file path, however the directory was granted.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = Plan {
        actions: vec![PlannedAction {
            path: guard(&dir, &home).unwrap(),
            size_bytes: 12,
            disposal: Disposal::Trash,
            category: "uninstaller-leftovers".to_string(),
        }],
        dirs: Vec::new(),
        skipped_protected: 0,
        skipped_unreadable: 0,
    };
    let (audit_path, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(dir.join("nested/deeper/c.bin").exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("file action cannot name a directory"));
}

#[test]
fn a_directory_grant_does_not_extend_to_a_subdirectory() {
    // [LB] Exact equality, never `starts_with` — restated for `SafeDir`.
    let (_g, home) = fake_home();
    let parent = home.join("Library/Caches/com.acme.App");
    leftover_tree(&parent);
    let child = parent.join("nested");
    let plan = dir_plan(&vouch(&child, &home));
    let (_p, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting_dirs(vec![vouch(&parent, &home)]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(child.join("deeper/c.bin").exists());
}

#[test]
fn a_protected_directory_can_never_be_granted() {
    // [LB] There is no `SafeDir` to put in `granted_dirs` for any of these,
    // because `guard_dir` is the only constructor and it runs the denylist on
    // the root before it walks. The grant mechanism cannot be handed what the
    // kernel refuses to vouch for.
    let (_g, home) = fake_home();
    write(&home.join("Library/Mail/V10/INBOX.mbox/x"), b"mail");

    for protected in [
        home.join("Library/Mail"),
        home.join("Library"),
        home.clone(),
    ] {
        assert!(
            guard_dir(&protected, &home, DirLimits::default()).is_err(),
            "{} must not be vouchable",
            protected.display()
        );
    }
}

// --- re-validation at the moment of disposal --------------------------------

#[test]
fn a_directory_replaced_by_a_symlink_after_planning_is_refused() {
    // [LB] The executor-level TOCTOU. Between planning and disposal the
    // directory is swapped for a symlink into the user's documents. The
    // re-walk resolves the link, finds a perfectly guardable tree there — and
    // refuses, because it is not the directory that was planned.
    let (_g, home) = fake_home();
    let precious = home.join("Documents/precious");
    write(&precious.join("thesis.pages"), b"years of work");
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    fs::rename(&dir, home.join("Library/Caches/moved-aside")).unwrap();
    std::os::unix::fs::symlink(&precious, &dir).unwrap();

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(
        precious.join("thesis.pages").exists(),
        "the target is untouched"
    );
    assert!(home
        .join("Library/Caches/moved-aside/nested/deeper/c.bin")
        .exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("resolves elsewhere"));
}

#[test]
fn a_directory_that_gained_an_entry_since_planning_is_refused() {
    // [LB] The mass-delete gate measured the planned figures. A tree that
    // gained a name afterwards could cross the count threshold the user never
    // confirmed — so one more entry, of zero bytes, is a refusal. Zero bytes
    // on purpose: this half of the rule must hold on its own.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    write(&dir.join("arrived-later.bin"), b"");

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(dir.join("arrived-later.bin").exists());
    assert!(fs::read_to_string(&audit_path).unwrap().contains("grew"));
}

#[test]
fn a_directory_that_gained_bytes_since_planning_is_refused() {
    // [LB] The other half. Same names, one of them rewritten much larger — a
    // cache file that ballooned after the sheet was confirmed. The count is
    // unchanged, so only the byte comparison can catch it.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    fs::write(dir.join("a.bin"), vec![0u8; 64 * 1024]).unwrap();
    assert_eq!(
        vouch(&dir, &home).entries(),
        safe.entries(),
        "the fixture must grow in bytes only"
    );

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(dir.join("a.bin").exists());
    assert!(fs::read_to_string(&audit_path).unwrap().contains("grew"));
}

#[test]
fn a_directory_that_shrank_since_planning_is_disposed_and_audited_as_found() {
    // The comment in the executor promises that shrinking is fine and that the
    // fresh figures are what get audited. Both halves pinned: the disposal
    // proceeds, and the record carries the smaller figures, not the confirmed
    // ones — a log that overstates what was removed is its own dishonesty.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    fs::remove_file(dir.join("nested/b.bin")).unwrap();
    let shrunk = vouch(&dir, &home);
    assert!(shrunk.entries() < safe.entries() && shrunk.bytes() < safe.bytes());

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 1);
    assert_eq!(report.entries_executed, shrunk.entries() as u64);
    assert_eq!(report.bytes_executed, shrunk.bytes());
    let lines = log_lines(&audit_path);
    let rec = lines
        .iter()
        .find(|l| l["phase"] == "executed")
        .expect("one executed record");
    assert_eq!(rec["entries"], shrunk.entries() as u64);
    assert_eq!(rec["size_bytes"], shrunk.bytes());
}

#[test]
fn a_git_checkout_appearing_after_planning_refuses_the_directory() {
    // [LB] The re-walk runs the denylist at every depth, so a `.git` that
    // arrived after the plan was built refuses the whole tree.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    write(&dir.join("vendor/.git/HEAD"), b"ref: refs/heads/main");

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(dir.join("nested/deeper/c.bin").exists());
    assert!(dir.join("vendor/.git/HEAD").exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("protected"));
}

#[test]
fn an_over_deep_directory_is_refused_at_execute_time() {
    // `DirLimits::max_depth` is 32. A tree that deepened past it after
    // planning is refused by the re-walk, whichever rule fires first.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (_p, mut audit) = audit_at(&home);

    let mut deep = dir.clone();
    for i in 0..(DirLimits::default().max_depth + 1) {
        deep = deep.join(format!("d{i}"));
    }
    fs::create_dir_all(&deep).unwrap();

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(deep.exists());
}

// --- SAFETY CONTRACT item 5: one directory is not one item -------------------

#[test]
fn the_recursive_count_is_what_the_mass_delete_threshold_sees() {
    // [LB] One directory action stands for every name beneath it. If
    // `Plan::count` ever returns 1 per directory, this fails — which is the
    // point.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    for i in 0..MASS_DELETE_COUNT {
        write(&dir.join(format!("f{i}.bin")), b"x");
    }
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (_p, mut audit) = audit_at(&home);

    assert_eq!(
        plan.count(),
        safe.entries() + 1,
        "the tree plus its own name"
    );
    assert_eq!(
        plan.total_bytes(),
        safe.bytes(),
        "and every byte beneath it"
    );
    assert!(plan.requires_confirmation());

    let unconfirmed = Consent {
        execute: true,
        granted_dirs: vec![safe.clone()],
        ..Default::default()
    };
    let err = execute(&plan, unconfirmed, &home, &sink(&home), &mut audit).unwrap_err();

    match err {
        ExecError::MassDeleteUnconfirmed { count, .. } => {
            assert_eq!(count, safe.entries() + 1)
        }
        other => panic!("expected MassDeleteUnconfirmed, got {other}"),
    }
    assert!(dir.join("f0.bin").exists(), "nothing touched");
}

#[test]
fn a_confirmed_mass_delete_of_one_directory_proceeds() {
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    for i in 0..MASS_DELETE_COUNT {
        write(&dir.join(format!("f{i}.bin")), b"x");
    }
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (_p, mut audit) = audit_at(&home);

    let consent = Consent {
        execute: true,
        confirmed_mass_delete: true,
        granted_dirs: vec![safe],
        ..Default::default()
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 1);
    assert_eq!(report.entries_executed, MASS_DELETE_COUNT as u64);
    assert!(!dir.exists());
}

#[test]
fn a_one_directory_plan_under_both_thresholds_still_requires_confirmation() {
    // [LB] Item 5 says recursive removals require confirmation — not large
    // ones. The numbers alone cannot enforce that: `DirLimits::max_bytes`
    // equals `MASS_DELETE_BYTES` and `guard_dir` refuses on `>`, so no single
    // tree can exceed the byte threshold, and one with fewer than
    // `MASS_DELETE_COUNT` entries slips under both. Three files, twelve bytes,
    // and still a confirmation.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    assert!(plan.count() < MASS_DELETE_COUNT);
    assert!(
        plan.requires_confirmation(),
        "a recursive removal, however small"
    );

    let unconfirmed = Consent {
        execute: true,
        granted_dirs: vec![safe.clone()],
        ..Default::default()
    };
    assert!(matches!(
        execute(&plan, unconfirmed, &home, &sink(&home), &mut audit),
        Err(ExecError::MassDeleteUnconfirmed { .. })
    ));
    assert!(dir.join("nested/deeper/c.bin").exists(), "nothing touched");
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("needs explicit confirmation"));

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();
    assert_eq!(report.executed, 1);
}

// --- SAFETY CONTRACT item 4: never permanently -------------------------------

#[test]
fn a_directory_is_never_removed_permanently_even_with_allow_permanent() {
    // [LB] There is no `Disposal` on a directory action, so there is nothing
    // for `allow_permanent` to unlock. The tree lands in the trash and the log
    // never says otherwise.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    let consent = Consent {
        execute: true,
        allow_permanent: true,
        confirmed_mass_delete: true,
        granted_dirs: vec![safe],
        ..Default::default()
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 1);
    assert!(home
        .join("test-trash/com.acme.App/nested/deeper/c.bin")
        .exists());
    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(!log.contains("\"disposition\":\"permanent\""));
    assert!(log.contains("\"disposition\":\"trash\""));
}

// --- SAFETY CONTRACT item 1: dry run ----------------------------------------

#[test]
fn a_dry_run_previews_a_directory_and_mutates_nothing() {
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);
    let before = snapshot(&home);

    let consent = Consent {
        granted_dirs: vec![safe.clone()],
        ..Default::default() // execute: false
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert!(report.dry_run);
    assert_eq!(report.planned, 1);
    assert_eq!(report.entries_planned, safe.entries() as u64);
    assert_eq!(report.executed, 0);
    // The audit file is the one thing that may change.
    let after: Vec<PathBuf> = snapshot(&home)
        .into_iter()
        .filter(|p| p != &audit_path)
        .collect();
    let before: Vec<PathBuf> = before.into_iter().filter(|p| p != &audit_path).collect();
    assert_eq!(after, before);

    let lines = log_lines(&audit_path);
    let planned = lines
        .iter()
        .find(|l| l["path"] == dir.display().to_string())
        .expect("the preview names the directory");
    assert_eq!(planned["phase"], "planned");
    assert_eq!(planned["entries"], safe.entries() as u64);
}

#[test]
fn a_dry_run_refuses_what_the_real_run_would_and_never_claims_to_have_executed() {
    // [LB] Two things at once. The preview refuses an ungranted directory the
    // same way execution does; and no line of a dry run may say "executed" —
    // refusals included. The second half is the fix for a defect the file
    // path had since #27: every preview refusal was logged as executed, and
    // nothing pinned the phase.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let stray = home.join("Documents/stray.bin");
    write(&stray, b"outside the allowlist, no grant");
    let mut plan = dir_plan(&vouch(&dir, &home));
    plan.actions.push(PlannedAction {
        path: guard(&stray, &home).unwrap(),
        size_bytes: 31,
        disposal: Disposal::Trash,
        category: "large-and-old".to_string(),
    });
    let (audit_path, mut audit) = audit_at(&home);

    let report = execute(&plan, Consent::default(), &home, &sink(&home), &mut audit).unwrap();

    assert!(report.dry_run);
    assert_eq!(report.refused, 2, "the directory and the file");
    assert_eq!(report.planned, 0);
    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(
        !log.contains("\"phase\":\"executed\""),
        "a preview wrote a line claiming execution:\n{log}"
    );
    assert!(log.contains("\"disposition\":\"refused\""));
}

// --- SAFETY CONTRACT item 6: the record ------------------------------------

#[test]
fn the_audit_record_for_a_directory_carries_its_recursive_count_and_size() {
    // [LB] One log line standing for a whole tree must say so as data.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    let bytes = leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (audit_path, mut audit) = audit_at(&home);

    execute(
        &plan,
        consenting_dirs(vec![safe.clone()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    let lines = log_lines(&audit_path);
    let executed: Vec<&serde_json::Value> =
        lines.iter().filter(|l| l["phase"] == "executed").collect();
    assert_eq!(executed.len(), 1);
    let rec = executed[0];
    assert_eq!(rec["path"], dir.display().to_string());
    assert_eq!(rec["disposition"], "trash");
    assert_eq!(rec["size_bytes"], bytes);
    assert_eq!(rec["entries"], safe.entries() as u64);
    assert!(rec["note"]
        .as_str()
        .unwrap()
        .contains("user-granted directory"));
}

#[test]
fn a_file_record_serializes_exactly_as_before() {
    // The new field is absent for a file, so nothing that reads the log —
    // restore, in particular — sees a changed shape for the records it knows.
    let (_g, home) = fake_home();
    let f = home.join("Library/Caches/app/blob.bin");
    write(&f, b"1234");
    let plan = Plan {
        actions: vec![PlannedAction {
            path: guard(&f, &home).unwrap(),
            size_bytes: 4,
            disposal: Disposal::Trash,
            category: "caches".to_string(),
        }],
        dirs: Vec::new(),
        skipped_protected: 0,
        skipped_unreadable: 0,
    };
    let (audit_path, mut audit) = audit_at(&home);

    execute(
        &plan,
        Consent {
            execute: true,
            ..Default::default()
        },
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    let line = &log_lines(&audit_path)[0];
    assert!(line.get("entries").is_none(), "{line}");
}

// --- the grant cap ---------------------------------------------------------

#[test]
fn the_combined_grant_cap_cannot_be_doubled_by_splitting_the_lists() {
    // [LB] `MAX_GRANTS` files plus one directory is one grant too many. A cap
    // per list would let a caller hand over twice the bound.
    let (_g, home) = fake_home();
    let f = home.join("Documents/one.bin");
    write(&f, b"1");
    let dir = home.join("Library/Caches/com.acme.App");
    leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (_p, mut audit) = audit_at(&home);

    for execute_flag in [true, false] {
        let consent = Consent {
            execute: execute_flag,
            granted: vec![guard(&f, &home).unwrap(); MAX_GRANTS],
            granted_dirs: vec![safe.clone()],
            ..Default::default()
        };
        match execute(&plan, consent, &home, &sink(&home), &mut audit) {
            Err(ExecError::TooManyGrants { count, max }) => {
                assert_eq!(count, MAX_GRANTS + 1);
                assert_eq!(max, MAX_GRANTS);
            }
            other => panic!("execute={execute_flag}: expected TooManyGrants, got {other:?}"),
        }
    }
    assert!(dir.join("nested/deeper/c.bin").exists());
}

// --- shapes and counts -----------------------------------------------------

#[test]
fn mixed_file_and_directory_actions_are_both_honoured() {
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"12345678");
    let dir = home.join("Library/Caches/com.acme.App");
    let bytes = leftover_tree(&dir);
    let safe = vouch(&dir, &home);
    let mut plan = dir_plan(&safe);
    plan.actions.push(PlannedAction {
        path: guard(&f, &home).unwrap(),
        size_bytes: 8,
        disposal: Disposal::Trash,
        category: "large-and-old".to_string(),
    });
    let (audit_path, mut audit) = audit_at(&home);

    let consent = Consent {
        execute: true,
        confirmed_mass_delete: true,
        granted: vec![guard(&f, &home).unwrap()],
        granted_dirs: vec![safe],
        ..Default::default()
    };
    assert_eq!(plan.total_bytes(), bytes + 8, "the plan sees both shapes");
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 2);
    assert_eq!(report.bytes_executed, bytes + 8);
    assert!(!f.exists() && !dir.exists());
    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("user-granted path outside the allowlist"));
    assert!(log.contains("user-granted directory"));
}

#[test]
fn hard_links_inside_a_directory_are_counted_once_per_name() {
    // [LB] A disposal unlinks names. Two names of one inode are two entries
    // and twice the bytes — the figure the user was shown, and the figure the
    // threshold sees. A `(dev, ino)` dedup anywhere on this path fails here.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/com.acme.App");
    write(&dir.join("a.bin"), b"abcd");
    fs::hard_link(dir.join("a.bin"), dir.join("b.bin")).unwrap();
    let safe = vouch(&dir, &home);
    let plan = dir_plan(&safe);
    let (_p, mut audit) = audit_at(&home);

    assert_eq!(safe.entries(), 2);
    assert_eq!(safe.bytes(), 8);

    let report = execute(
        &plan,
        consenting_dirs(vec![safe]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.entries_executed, 2);
    assert_eq!(report.bytes_executed, 8);
}

#[test]
fn scan_produces_no_directory_actions() {
    // [LB] A canary on the scanner. `ScanReport` has no directory
    // representation, so a scanner that started planning directories would
    // inflate `total_count` with no rows to explain it — and would be planning
    // trees nobody granted.
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/blob.bin"), b"junk");
    write(&home.join("Library/Caches/app/nested/more.bin"), b"junk");

    let plan = scan(&ScanConfig::with_default_roots(home.clone()));

    assert!(plan.dirs.is_empty());
    assert_eq!(plan.count(), plan.actions.len());
    assert_eq!(plan.actions.len(), 2);
}
