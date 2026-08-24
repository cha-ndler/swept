//! The per-path grant escape hatch, and every bound on it.
//!
//! M1 splits *discovery* scope from *disposal* scope: read-only walkers may
//! look at `~/Documents`, but the executor still refuses to touch anything
//! outside `allowlist::default_roots` unless the user has picked that exact
//! path out by hand. `Consent::granted` carries those picks.
//!
//! This is the only relaxation of the disposal boundary in the codebase, so it
//! gets the most adversarial tests in it. The shape of the argument:
//!
//! - a grant works (otherwise the feature is pointless);
//! - and it works for *exactly* one file, *exactly* once, only when the path
//!   still passes the denylist at the moment of disposal.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{execute, Consent, DirSink, ExecError, Sink, MAX_GRANTS};
use macclean_core::plan::{Disposal, Plan, PlannedAction};
use safety::guard;

fn fake_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/Caches/app")).unwrap();
    fs::create_dir_all(home.join("Documents")).unwrap();
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

/// Build a one-action plan naming `path`. The scanner would never produce this
/// for a path outside the allowlist — which is the point: we are testing the
/// executor's own boundary, not the scanner's.
fn plan_for(path: &Path, home: &Path, size_bytes: u64) -> Plan {
    Plan {
        actions: vec![PlannedAction {
            path: guard(path, home).expect("fixture path must be guardable"),
            size_bytes,
            disposal: Disposal::Trash,
            category: "large-and-old".to_string(),
        }],
        skipped_protected: 0,
    }
}

fn consenting(granted: Vec<safety::SafePath>) -> Consent {
    Consent {
        execute: true,
        granted,
        ..Default::default()
    }
}

fn sink(home: &Path) -> DirSink {
    DirSink {
        trash_dir: home.join("test-trash"),
    }
}

#[test]
fn a_granted_file_outside_the_allowlist_is_disposed() {
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"pretend this is 8 GiB");

    let plan = plan_for(&f, &home, 21);
    let grant = guard(&f, &home).unwrap();
    let (_p, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting(vec![grant]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(
        report.executed, 1,
        "an explicitly granted path is disposable"
    );
    assert_eq!(report.refused, 0);
    assert!(!f.exists());
    assert!(home.join("test-trash/huge.iso").exists());
}

#[test]
fn the_same_file_without_a_grant_is_refused() {
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"pretend this is 8 GiB");

    let plan = plan_for(&f, &home, 21);
    let (audit_path, mut audit) = audit_at(&home);

    // Identical plan, identical consent — minus the grant.
    let report = execute(&plan, consenting(vec![]), &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(
        f.exists(),
        "an ungranted path outside the allowlist survives"
    );
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("outside allowlist at execution time"));
}

#[test]
fn a_grant_does_not_extend_to_a_sibling() {
    let (_g, home) = fake_home();
    let granted = home.join("Documents/granted.bin");
    let sibling = home.join("Documents/sibling.bin");
    write(&granted, b"ok");
    write(&sibling, b"no");

    let plan = plan_for(&sibling, &home, 2);
    let (_p, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting(vec![guard(&granted, &home).unwrap()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.refused, 1);
    assert!(sibling.exists(), "granting one file must not grant another");
    assert!(granted.exists(), "and must not dispose of the granted one");
}

#[test]
fn a_grant_on_a_directory_does_not_authorize_its_contents() {
    // The critical no-prefix-matching case. If grant matching used
    // `starts_with` instead of equality, granting a directory would silently
    // authorize its entire subtree — a whole-folder delete disguised as one
    // hand-picked item.
    let (_g, home) = fake_home();
    let dir = home.join("Documents/project");
    let inner = dir.join("inner.bin");
    write(&inner, b"data");

    let plan = plan_for(&inner, &home, 4);
    let (_p, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting(vec![guard(&dir, &home).unwrap()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.refused, 1);
    assert!(
        inner.exists(),
        "a directory grant confers nothing downwards"
    );
}

#[test]
fn a_directory_grant_is_refused_outright() {
    // Disposing of a directory means a recursive removal. The refusal is now
    // general rather than grant-specific — *every* directory target is refused,
    // granted or allowlisted — which subsumes this case and is strictly
    // stronger, so the reason string changed with it.
    let (_g, home) = fake_home();
    let dir = home.join("Documents/project");
    let inner = dir.join("inner.bin");
    write(&inner, b"data");

    let plan = plan_for(&dir, &home, 4);
    let (audit_path, mut audit) = audit_at(&home);

    let report = execute(
        &plan,
        consenting(vec![guard(&dir, &home).unwrap()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(dir.exists(), "the directory itself survives");
    assert!(inner.exists(), "and so does everything under it");
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("directory target"));
}

#[test]
fn a_directory_inside_the_allowlist_is_refused_too() {
    // The residual M1 left open, now closed. An allowlisted directory used to
    // reach `remove_dir_all` with nothing having looked inside it. No planner
    // produces such an action today — which is exactly why it was worth
    // refusing before one does.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/app");
    let inner = dir.join("deep/nested.bin");
    write(&inner, b"data");

    let plan = plan_for(&dir, &home, 4);
    let (audit_path, mut audit) = audit_at(&home);

    let report = execute(&plan, consenting(vec![]), &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(
        dir.exists(),
        "an allowlisted directory is still not removable"
    );
    assert!(inner.exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("directory target"));
}

#[test]
fn a_preview_refuses_a_directory_the_same_way_execution_does() {
    // Preview/execute parity: both branches run the same `authorize`, so the
    // directory refusal cannot show up in one and not the other.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/app");
    write(&dir.join("inner.bin"), b"data");

    let plan = plan_for(&dir, &home, 4);
    let (_p, mut audit) = audit_at(&home);

    let report = execute(&plan, Consent::default(), &home, &sink(&home), &mut audit).unwrap();

    assert!(report.dry_run);
    assert_eq!(report.planned, 0);
    assert_eq!(report.refused, 1, "the preview predicts the refusal");
}

#[test]
fn the_sink_can_never_remove_a_directory() {
    // The atomic backstop. `authorize`'s check and the sink call cannot be made
    // atomic, so `delete` must fail closed on its own if a directory is swapped
    // onto the name in between.
    let (_g, home) = fake_home();
    let dir = home.join("Library/Caches/app/subdir");
    let inner = dir.join("inner.bin");
    write(&inner, b"data");

    let err = sink(&home).delete(&dir).unwrap_err();

    assert!(dir.exists(), "the directory survives, {err}");
    assert!(inner.exists(), "and so does its contents");
}

#[test]
fn a_grant_cannot_be_minted_for_a_protected_path() {
    // `Consent::granted` is a `Vec<SafePath>`, and `guard` is the only way to
    // make one. So the denylist is not merely consulted for grants — it is
    // upstream of their very existence.
    let (_g, home) = fake_home();
    let mail = home.join("Library/Mail/messages.db");
    write(&mail, b"private");

    assert!(
        guard(&mail, &home).is_err(),
        "no SafePath exists for ~/Library/Mail, so no grant can name it"
    );
    assert!(guard(&home, &home).is_err(), "nor for the home root");
}

#[test]
fn a_granted_path_that_becomes_protected_is_still_refused() {
    // TOCTOU, with a grant in hand. The grant is not a bypass of the
    // pre-mutation re-guard; it sits behind it.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");

    let plan = plan_for(&f, &home, 4);
    let grant = guard(&f, &home).unwrap();

    // Between plan and execution the path becomes a symlink to the home root.
    fs::remove_file(&f).unwrap();
    std::os::unix::fs::symlink(&home, &f).unwrap();

    let (_p, mut audit) = audit_at(&home);
    let report = execute(
        &plan,
        consenting(vec![grant]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 0);
    assert_eq!(report.refused, 1);
    assert!(home.join("Documents").exists(), "home must be untouched");
}

#[test]
fn a_grant_is_matched_against_the_re_resolved_path() {
    // Subtler TOCTOU: the planned path still guards cleanly, but now resolves
    // somewhere else. The grant must be compared against what the path means
    // *now*, not what it meant at scan time, or a swapped symlink redirects an
    // authorization onto a file the user never picked.
    let (_g, home) = fake_home();
    let picked = home.join("Documents/picked.bin");
    let other = home.join("Documents/other.bin");
    write(&picked, b"data");
    write(&other, b"precious");

    let plan = plan_for(&picked, &home, 4);
    let grant = guard(&picked, &home).unwrap();

    fs::remove_file(&picked).unwrap();
    std::os::unix::fs::symlink(&other, &picked).unwrap();

    let (_p, mut audit) = audit_at(&home);
    let report = execute(
        &plan,
        consenting(vec![grant]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.refused, 1);
    assert!(other.exists(), "the redirected-to file was never granted");
}

#[test]
fn too_many_grants_refuses_the_entire_run() {
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");
    let cache = home.join("Library/Caches/app/a.bin");
    write(&cache, b"junk");

    // A plan of a perfectly ordinary allowlisted file: the point is that an
    // over-long grant list aborts the run *wholesale*, not that the grants
    // themselves are acted on. Repeating one guarded path keeps the test about
    // list length, which is what the cap actually bounds.
    let plan = plan_for(&cache, &home, 4);
    let grant = guard(&f, &home).unwrap();
    let grants = vec![grant; MAX_GRANTS + 1];

    let (_p, mut audit) = audit_at(&home);
    let err = execute(&plan, consenting(grants), &home, &sink(&home), &mut audit).unwrap_err();

    assert!(
        matches!(err, ExecError::TooManyGrants { count, max }
            if count == MAX_GRANTS + 1 && max == MAX_GRANTS),
        "got {err:?}"
    );
    assert!(cache.exists(), "an over-long grant list mutates nothing");
    assert!(f.exists());
}

#[test]
fn an_over_long_grant_list_stops_a_dry_run_too() {
    // A preview that quietly succeeds while the real run would be refused is a
    // preview that lies. Fail the same way in both modes.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");
    let plan = plan_for(&f, &home, 4);

    let consent = Consent {
        granted: vec![guard(&f, &home).unwrap(); MAX_GRANTS + 1],
        ..Default::default() // execute: false — a dry run
    };
    let (_p, mut audit) = audit_at(&home);

    assert!(matches!(
        execute(&plan, consent, &home, &sink(&home), &mut audit),
        Err(ExecError::TooManyGrants { .. })
    ));
}

#[test]
fn exactly_max_grants_is_accepted() {
    // The boundary the other way: the cap must not be off by one.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");

    let plan = plan_for(&f, &home, 4);
    let grants = vec![guard(&f, &home).unwrap(); MAX_GRANTS];
    let (_p, mut audit) = audit_at(&home);

    let report = execute(&plan, consenting(grants), &home, &sink(&home), &mut audit).unwrap();
    assert_eq!(report.executed, 1);
}

#[test]
fn a_granted_disposal_is_audited_distinctly() {
    // Item 6. A reviewer reading the log must be able to tell "this was removed
    // because it sat in a cache directory" from "this was removed because the
    // user pointed at it", because only the second one is unrecoverable
    // judgement rather than policy.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");
    let cache = home.join("Library/Caches/app/a.bin");
    write(&cache, b"junk");

    let mut plan = plan_for(&f, &home, 4);
    plan.actions.push(PlannedAction {
        path: guard(&cache, &home).unwrap(),
        size_bytes: 4,
        disposal: Disposal::Trash,
        category: "user-caches".to_string(),
    });

    let (audit_path, mut audit) = audit_at(&home);
    let report = execute(
        &plan,
        consenting(vec![guard(&f, &home).unwrap()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();
    assert_eq!(report.executed, 2);

    let log = fs::read_to_string(&audit_path).unwrap();
    let granted_lines: Vec<&str> = log.lines().filter(|l| l.contains("user-granted")).collect();
    assert_eq!(
        granted_lines.len(),
        1,
        "exactly the granted disposal carries the note, in:\n{log}"
    );
    assert!(granted_lines[0].contains("huge.iso"));
}

#[test]
fn a_grant_does_not_make_a_dry_run_mutate() {
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");

    let plan = plan_for(&f, &home, 4);
    let consent = Consent {
        granted: vec![guard(&f, &home).unwrap()],
        ..Default::default() // execute: false
    };
    let (_p, mut audit) = audit_at(&home);

    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.executed, 0);
    assert!(f.exists(), "dry-run default outranks any grant");
}

#[test]
fn grants_do_not_bypass_mass_delete_confirmation() {
    let (_g, home) = fake_home();
    let mut plan = Plan::default();
    let mut grants = Vec::new();
    for i in 0..(macclean_core::plan::MASS_DELETE_COUNT + 1) {
        let f = home.join(format!("Documents/f{i}.bin"));
        write(&f, b"data");
        let safe = guard(&f, &home).unwrap();
        grants.push(safe.clone());
        plan.actions.push(PlannedAction {
            path: safe,
            size_bytes: 4,
            disposal: Disposal::Trash,
            category: "large-and-old".to_string(),
        });
    }

    let (_p, mut audit) = audit_at(&home);
    let err = execute(&plan, consenting(grants), &home, &sink(&home), &mut audit).unwrap_err();

    assert!(
        matches!(err, ExecError::MassDeleteUnconfirmed { .. }),
        "got {err:?}"
    );
    assert!(home.join("Documents/f0.bin").exists());
}

#[test]
fn a_grant_does_not_confer_permanent_deletion_even_with_allow_permanent() {
    // The bound that matters, and the one an earlier version of this file only
    // *appeared* to test: it asserted the fallback with `allow_permanent`
    // false, which proves nothing about grants. With the flag actually set, a
    // granted path must still land in the Trash.
    //
    // Irreversible removal stays confined to the allowlist because that is the
    // safer way round: the allowlist covers caches, which regenerate; a grant
    // covers a hand-picked file in ~/Documents, which does not.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");

    let plan = Plan {
        actions: vec![PlannedAction {
            path: guard(&f, &home).unwrap(),
            size_bytes: 4,
            disposal: Disposal::Permanent,
            category: "large-and-old".to_string(),
        }],
        skipped_protected: 0,
    };

    let (audit_path, mut audit) = audit_at(&home);
    let consent = Consent {
        execute: true,
        allow_permanent: true,
        granted: vec![guard(&f, &home).unwrap()],
        ..Default::default()
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 1);
    assert!(
        home.join("test-trash/huge.iso").exists(),
        "a granted path must be recoverable even under --permanent"
    );
    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(
        !log.contains("\"disposition\":\"permanent\""),
        "nothing granted may be recorded as an irreversible removal, in:\n{log}"
    );
}

#[test]
fn allow_permanent_still_works_inside_the_allowlist() {
    // The other direction: confining irreversible removal to the allowlist must
    // not have disabled it there.
    let (_g, home) = fake_home();
    let cache = home.join("Library/Caches/app/a.bin");
    write(&cache, b"junk");

    let plan = Plan {
        actions: vec![PlannedAction {
            path: guard(&cache, &home).unwrap(),
            size_bytes: 4,
            disposal: Disposal::Permanent,
            category: "user-caches".to_string(),
        }],
        skipped_protected: 0,
    };

    let (audit_path, mut audit) = audit_at(&home);
    let consent = Consent {
        execute: true,
        allow_permanent: true,
        ..Default::default()
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 1);
    assert!(!cache.exists());
    assert!(
        !home.join("test-trash/a.bin").exists(),
        "an allowlisted permanent action is not a move to the Trash"
    );
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("\"disposition\":\"permanent\""));
}

#[test]
fn a_preview_refuses_what_the_real_run_would_refuse() {
    // A dry run that reports "would be trashed" for a path the executor would
    // refuse is a preview that overstates what is about to happen. The
    // authorization runs in both modes so the two agree.
    let (_g, home) = fake_home();
    let ungranted = home.join("Documents/thesis.pdf");
    let granted = home.join("Documents/huge.iso");
    write(&ungranted, b"data");
    write(&granted, b"data");

    let mut plan = plan_for(&ungranted, &home, 4);
    plan.actions.push(PlannedAction {
        path: guard(&granted, &home).unwrap(),
        size_bytes: 4,
        disposal: Disposal::Trash,
        category: "large-and-old".to_string(),
    });

    let (audit_path, mut audit) = audit_at(&home);
    let consent = Consent {
        granted: vec![guard(&granted, &home).unwrap()],
        ..Default::default() // execute: false — a preview
    };
    let report = execute(&plan, consent, &home, &sink(&home), &mut audit).unwrap();

    assert!(report.dry_run);
    assert_eq!(report.planned, 1, "only the granted path would be acted on");
    assert_eq!(
        report.refused, 1,
        "and the ungranted one is previewed as refused"
    );
    assert!(ungranted.exists(), "a preview still mutates nothing");
    assert!(granted.exists());

    // The preview's log distinguishes them too.
    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("\"disposition\":\"refused\""));
    assert!(
        log.lines().filter(|l| l.contains("user-granted")).count() == 1,
        "a previewed escalation is marked as one, in:\n{log}"
    );
}

#[test]
fn a_wholesale_refusal_is_recorded_in_the_audit_log() {
    // The most decisive thing the executor can do — refusing an entire run —
    // used to be the one thing the log never mentioned.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");
    let plan = plan_for(&f, &home, 4);

    let (audit_path, mut audit) = audit_at(&home);
    let grants = vec![guard(&f, &home).unwrap(); MAX_GRANTS + 1];
    assert!(execute(&plan, consenting(grants), &home, &sink(&home), &mut audit).is_err());

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("\"disposition\":\"refused\""), "in:\n{log}");
    assert!(
        log.contains("exceeds the limit"),
        "with a reason, in:\n{log}"
    );
}

#[test]
fn a_refused_mass_delete_is_recorded_too() {
    let (_g, home) = fake_home();
    let mut plan = Plan::default();
    for i in 0..(macclean_core::plan::MASS_DELETE_COUNT + 1) {
        let f = home.join(format!("Library/Caches/app/f{i}.bin"));
        write(&f, b"data");
        plan.actions.push(PlannedAction {
            path: guard(&f, &home).unwrap(),
            size_bytes: 4,
            disposal: Disposal::Trash,
            category: "user-caches".to_string(),
        });
    }

    let (audit_path, mut audit) = audit_at(&home);
    let consent = Consent {
        execute: true,
        ..Default::default()
    };
    assert!(execute(&plan, consent, &home, &sink(&home), &mut audit).is_err());

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(log.contains("needs explicit confirmation"), "in:\n{log}");
    assert!(home.join("Library/Caches/app/f0.bin").exists());
}

#[test]
fn a_refusal_says_whether_grants_were_offered_at_all() {
    // "Outside the allowlist, nothing was granted" is routine confinement.
    // "Grants were offered and this path was not among them" is a caller trying
    // to dispose of something the user did not pick — worth finding later.
    let (_g, home) = fake_home();
    let picked = home.join("Documents/picked.bin");
    let other = home.join("Documents/other.bin");
    write(&picked, b"ok");
    write(&other, b"no");

    let plan = plan_for(&other, &home, 2);
    let (audit_path, mut audit) = audit_at(&home);
    execute(
        &plan,
        consenting(vec![guard(&picked, &home).unwrap()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("not among the granted paths"));
}

#[test]
fn a_grant_does_not_confer_permanent_deletion() {
    // Grants widen *where* we may act, never *how*. Without `allow_permanent`
    // a Permanent action still falls back to the recoverable path.
    let (_g, home) = fake_home();
    let f = home.join("Documents/huge.iso");
    write(&f, b"data");

    let plan = Plan {
        actions: vec![PlannedAction {
            path: guard(&f, &home).unwrap(),
            size_bytes: 4,
            disposal: Disposal::Permanent,
            category: "large-and-old".to_string(),
        }],
        skipped_protected: 0,
    };

    let (_p, mut audit) = audit_at(&home);
    let report = execute(
        &plan,
        consenting(vec![guard(&f, &home).unwrap()]),
        &home,
        &sink(&home),
        &mut audit,
    )
    .unwrap();

    assert_eq!(report.executed, 1);
    assert!(
        home.join("test-trash/huge.iso").exists(),
        "must be recoverable, not unlinked"
    );
}

#[test]
fn allowlisted_disposal_is_unchanged_by_the_presence_of_grants() {
    // The regression direction: adding the grant check must not have made the
    // ordinary path depend on grants.
    let (_g, home) = fake_home();
    let cache = home.join("Library/Caches/app/a.bin");
    write(&cache, b"junk");

    let plan = plan_for(&cache, &home, 4);
    let (audit_path, mut audit) = audit_at(&home);

    let report = execute(&plan, consenting(vec![]), &home, &sink(&home), &mut audit).unwrap();

    assert_eq!(report.executed, 1);
    assert!(home.join("test-trash/a.bin").exists());
    assert!(
        !fs::read_to_string(&audit_path)
            .unwrap()
            .contains("user-granted"),
        "an allowlisted disposal is not a granted one"
    );
}
